# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Tokenizer
"""The GPT-2 byte-level BPE tokenizer, rebuilt from the published merges.txt alone.

GPT-2 tokenizes bytes, not characters. Each byte is first mapped to a printable
stand-in code point so merges can be written as text; the 256 stand-ins take ids
0..255 in `bytes_to_unicode` order, and merge number i of merges.txt produces
token 256 + i. Decoding never needs vocab.json for that reason: the merge list IS
the vocabulary.
"""

__all__ = ["Bpe", "load_bpe", "encode", "ids_text", "decode"]

NO_RANK = 2147483647


def byte_to_char(b):
    """A byte's stand-in code point: printable bytes stand for themselves, the
    others take U+0100 onward in byte order."""
    if b < 33:
        return 256 + b
    elif b < 127 or (b > 160 and b != 173):
        return b
    elif b == 173:
        return 323
    else:
        return 162 + b


def char_to_byte(cp):
    """The inverse of byte_to_char."""
    if cp < 256:
        return cp
    elif cp < 289:
        return cp - 256
    elif cp == 323:
        return 173
    else:
        return cp - 162


def byte_to_id(b):
    """The vocabulary id of a single byte, in the order bytes_to_unicode lists them."""
    if b > 32 and b < 127:
        return b - 33
    elif b > 160 and b < 173:
        return 94 + b - 161
    elif b > 173:
        return 106 + b - 174
    elif b < 33:
        return 188 + b
    elif b == 173:
        return 255
    else:
        return 221 + b - 127


def id_to_byte(token):
    """The byte a base vocabulary id (below 256) stands for."""
    if token < 94:
        return token + 33
    elif token < 106:
        return 161 + token - 94
    elif token < 188:
        return 174 + token - 106
    elif token < 221:
        return token - 188
    elif token == 255:
        return 173
    else:
        return 127 + token - 221


class Bpe:
    merges: list[str]
    ranks: dict[str, int]
    ids: dict[str, int]


def merge_row(rows, acc, i):
    pair = unwrap_or(listGet(rows, i), "")
    token = unwrap_or(replace(pair, " ", ""), pair)
    return Bpe(merges=listAppend(acc.merges, token), ranks=mapSet(acc.ranks, pair, i - 1), ids=mapSet(acc.ids, token, 255 + i))


def load_bpe(path):
    """The merge table from merges.txt: line i (after the version line) is the pair
    "a b" with rank i - 1, producing token "ab" with id 255 + i."""
    rows = lines(unwrap_or(readFile(path), ""))
    return fold(range(1, len(rows)), Bpe(merges=[], ranks={}, ids={}), lambda acc, i: merge_row(rows, acc, i))


def is_letter(b):
    """Whether byte b is a letter: ASCII letters, and every non-ASCII byte, since
    UTF-8 continuation bytes belong to the word they are inside."""
    return (b > 64 and b < 91) or (b > 96 and b < 123) or b > 127


def is_digit(b):
    return b > 47 and b < 58


def is_space(b):
    return b == 32 or b == 10 or b == 13 or b == 9


def class_of(b):
    """The character class a byte starts: 1 letters, 2 digits, 3 punctuation."""
    return 1 if is_letter(b) else 2 if is_digit(b) else 3


def byte_of(s, i):
    return unwrap_or(byteAt(s, i), 0)


def stand_in(b):
    return unwrap_or(fromCodePoint(byte_to_char(b)), "")


def contraction_at(s, i):
    """The length of a contraction suffix starting at i ('s 't 're 've 'm 'll 'd),
    or 0: these are their own pieces in GPT-2's pre-tokenizer."""
    two = unwrap_or(substring(s, i, i + 2), "")
    three = unwrap_or(substring(s, i, i + 3), "")
    if two == "'s" or two == "'t" or two == "'m" or two == "'d":
        return 2
    elif three == "'re" or three == "'ve" or three == "'ll":
        return 3
    else:
        return 0


class Scan:
    pieces: list[str]
    current: str
    kind: int
    spaces: str


def close_piece(sc):
    return sc.pieces if sc.current == "" else listAppend(sc.pieces, sc.current)


def flush_spaces(sc, next_is_word):
    """A whitespace run ends: every space but a trailing single ' ' is its own piece,
    and that trailing space opens the next word (the regex's `\\s+(?!\\S)`)."""
    return sc if sc.spaces == "" else spaces_end(sc, next_is_word)


def spaces_end(sc, next_is_word):
    carry = "Ġ" if next_is_word and endsWith(sc.spaces, "Ġ") else ""
    standalone = unwrap_or(substring(sc.spaces, 0, byteLength(sc.spaces) - byteLength(carry)), "")
    pieces = sc.pieces if standalone == "" else listAppend(sc.pieces, standalone)
    return Scan(pieces=pieces, current=carry, kind=0, spaces="")


def scan_pieces(s, i, sc):
    """Split the prompt into GPT-2's pre-tokenization pieces, as stand-in strings."""
    if i >= byteLength(s):
        return close_piece(flush_spaces(sc, False))
    else:
        b = byte_of(s, i)
        contraction = contraction_at(s, i) if b == 39 else 0
        if is_space(b):
            return scan_pieces(s, i + 1, Scan(pieces=close_piece(sc), current="", kind=0, spaces=sc.spaces + stand_in(b)))
        elif contraction > 0:
            return scan_pieces(s, i + contraction, Scan(pieces=listAppend(close_piece(flush_spaces(sc, False)), unwrap_or(substring(s, i, i + contraction), "")), current="", kind=0, spaces=""))
        else:
            return scan_pieces(s, i + 1, extend(flush_spaces(sc, True), b))


def extend(sc, b):
    """Append byte b to the open piece, or start a new piece when its class changes."""
    kind = class_of(b)
    continues = sc.kind == kind or (sc.kind == 0 and sc.current != "")
    if continues:
        return Scan(pieces=sc.pieces, current=sc.current + stand_in(b), kind=kind, spaces="")
    else:
        return Scan(pieces=close_piece(sc), current=stand_in(b), kind=kind, spaces="")


def symbols_of(piece, i, acc):
    """The stand-in characters of a piece, one string each."""
    if i >= byteLength(piece):
        return acc
    else:
        cp = unwrap_or(codePointAt(piece, i), 63)
        return symbols_of(piece, i + unwrap_or(codePointWidth(cp), 1), listAppend(acc, unwrap_or(fromCodePoint(cp), "?")))


def symbol_at(syms, i):
    return unwrap_or(listGet(syms, i), "")


class Best:
    rank: int
    index: int


def better(bpe, syms, best, i):
    rank = unwrap_or(mapGet(bpe.ranks, symbol_at(syms, i) + " " + symbol_at(syms, i + 1)), NO_RANK)
    return Best(rank=rank, index=i) if rank < best.rank else best


def best_pair(bpe, syms):
    """The adjacent pair with the lowest merge rank, or index -1 when none merges."""
    return fold(range(len(syms) - 1), Best(rank=NO_RANK, index=-1), lambda best, i: better(bpe, syms, best, i))


def merge_all(syms, first, second, i, acc):
    """Merge every non-overlapping occurrence of (first, second), left to right."""
    if i >= len(syms):
        return acc
    else:
        s = symbol_at(syms, i)
        if s == first and i + 1 < len(syms) and symbol_at(syms, i + 1) == second:
            return merge_all(syms, first, second, i + 2, listAppend(acc, s + second))
        else:
            return merge_all(syms, first, second, i + 1, listAppend(acc, s))


def bpe_encode(bpe, syms):
    """Byte-pair encode one piece: merge the best-ranked pair until none remains."""
    best = best_pair(bpe, syms)
    if best.index < 0:
        return syms
    else:
        return bpe_encode(bpe, merge_all(syms, symbol_at(syms, best.index), symbol_at(syms, best.index + 1), 0, []))


def id_of(bpe, sym):
    """A symbol's vocabulary id: a lone stand-in is a base byte, anything longer a merge."""
    return unwrap_or(mapGet(bpe.ids, sym), byte_to_id(char_to_byte(unwrap_or(codePointAt(sym, 0), 63))))


def ids_of(bpe, syms):
    return fold(range(len(syms)), [], lambda acc, i: listAppend(acc, id_of(bpe, symbol_at(syms, i))))


def encode_piece(bpe, piece):
    return ids_of(bpe, bpe_encode(bpe, symbols_of(piece, 0, [])))


def encode(bpe, text):
    """The token ids of a prompt."""
    pieces = scan_pieces(text, 0, Scan(pieces=[], current="", kind=0, spaces=""))
    return fold(range(len(pieces)), [], lambda acc, i: listConcat(acc, encode_piece(bpe, unwrap_or(listGet(pieces, i), ""))))


def bytes_of_token(bpe, token, acc):
    """The bytes a token id stands for, appended to acc."""
    if token < 256:
        return listAppend(acc, id_to_byte(token))
    else:
        syms = symbols_of(unwrap_or(listGet(bpe.merges, token - 256), ""), 0, [])
        return fold(range(len(syms)), acc, lambda octets, i: listAppend(octets, char_to_byte(unwrap_or(codePointAt(symbol_at(syms, i), 0), 63))))


def sequence_length(b):
    return 1 if b < 128 else 2 if b < 224 else 3 if b < 240 else 4


def lead_bits(b, n):
    return b if n == 1 else b - 192 if n == 2 else b - 224 if n == 3 else b - 240


def text_of(octets, i, acc):
    """Decode UTF-8 bytes into text; a malformed sequence prints as '?'."""
    if i >= len(octets):
        return acc
    else:
        b = unwrap_or(listGet(octets, i), 63)
        n = sequence_length(b)
        cp = fold(range(1, n), lead_bits(b, n), lambda code, k: code * 64 + unwrap_or(listGet(octets, i + k), 128) - 128)
        return text_of(octets, i + n, acc + unwrap_or(fromCodePoint(cp), "?"))


def ids_text(ids):
    """The ids as the space-led list both programs print after `tokens:`."""
    return fold(range(len(ids)), "", lambda acc, i: acc + " " + toString(unwrap_or(listGet(ids, i), 0)))


def decode(bpe, ids):
    octets = fold(range(len(ids)), [], lambda acc, i: bytes_of_token(bpe, unwrap_or(listGet(ids, i), 0), acc))
    return text_of(octets, 0, "")
