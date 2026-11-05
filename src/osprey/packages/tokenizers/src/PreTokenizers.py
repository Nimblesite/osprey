# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=PreTokenizers
"""The pre-tokenizers: BertPreTokenizer, Whitespace, WhitespaceSplit,
Punctuation, Digits, CharDelimiterSplit and Metaspace.

Each takes a `Norm` (the normalized text with byte alignments) and returns its
splits as a list of `Norm`, one per piece. The split behaviours mirror
tokenizers' SplitDelimiterBehavior: Removed (CharDelimiterSplit, Whitespace*),
Isolated (punctuation, individual digits), Contiguous (digit runs) and
MergedWithNext (Metaspace's `▁`).
"""
from tiktoken.Unicode import utf8_of_cp
from tokenizers import Category
from tokenizers.Unicode import is_numeric, is_punctuation, is_whitespace, is_word_char, text_cps
from tokenizers.Normalizers import Norm, append_norm, char_at, empty_norm, end_at, prepend_cp, replace_space, start_at

__all__ = [
    "bert_pretokenize", "whitespace_pretokenize", "whitespace_split",
    "punctuation_pretokenize", "digits_pretokenize", "char_delimiter_split",
    "metaspace_pretokenize", "META_CP",
    "QWEN_PATTERN", "qwen_split", "qwen_piece_len", "qwen_pretokenize",
    "bytelevel_piece", "bytelevel_text", "bytelevel_raw", "byte_of_char", "BYTE_CHARS",
]

META_CP = 9601
"""The codepoint of `▁` (U+2581), the metaspace marker SentencePiece-style pre-tokenizers insert for spaces."""


def close(cur, acc):
    return listAppend(acc, cur) if len(cur.chars) > 0 else acc


def split_removed(n, is_delim, i, cur, acc):
    """Split on a delimiter that is dropped."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = char_at(n, i)
        s = start_at(n, i)
        e = end_at(n, i)
        if is_delim(cp):
            return split_removed(n, is_delim, i + 1, empty_norm(), close(cur, acc))
        else:
            return split_removed(n, is_delim, i + 1, append_norm(cur, cp, s, e), acc)


def split_isolated(n, is_delim, i, cur, acc):
    """Split on a delimiter that becomes its own single-char piece."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = char_at(n, i)
        s = start_at(n, i)
        e = end_at(n, i)
        if is_delim(cp):
            acc2 = listAppend(close(cur, acc), Norm(chars=[cp], starts=[s], ends=[e]))
            return split_isolated(n, is_delim, i + 1, empty_norm(), acc2)
        else:
            return split_isolated(n, is_delim, i + 1, append_norm(cur, cp, s, e), acc)


def split_contiguous(n, is_delim, i, kind, cur, acc):
    """Split into maximal runs of delimiter and maximal runs of non-delimiter."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = char_at(n, i)
        s = start_at(n, i)
        e = end_at(n, i)
        k = 1 if is_delim(cp) else 0
        if len(cur.chars) > 0 and k != kind:
            return split_contiguous(n, is_delim, i + 1, k, append_norm(empty_norm(), cp, s, e), listAppend(acc, cur))
        else:
            return split_contiguous(n, is_delim, i + 1, k, append_norm(cur, cp, s, e), acc)


def split_merged_next(n, is_delim, i, cur, acc):
    """Split so each delimiter opens the following piece (Metaspace)."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = char_at(n, i)
        s = start_at(n, i)
        e = end_at(n, i)
        if is_delim(cp):
            return split_merged_next(n, is_delim, i + 1, Norm(chars=[cp], starts=[s], ends=[e]), close(cur, acc))
        else:
            return split_merged_next(n, is_delim, i + 1, append_norm(cur, cp, s, e), acc)


def bert_pretokenize(n):
    """Whitespace split (dropped) then punctuation split (isolated)."""
    ws = split_removed(n, is_whitespace, 0, empty_norm(), [])
    return fold(range(len(ws)), [], lambda acc, i: listConcat(acc, split_isolated(unwrap_or(listGet(ws, i), empty_norm()), is_punctuation, 0, empty_norm(), [])))


def whitespace_pretokenize(n):
    """The regex `\\w+|[^\\w\\s]+` inverted: word runs and symbol runs, whitespace dropped."""
    return whitespace_scan(n, 0, -1, empty_norm(), [])


def whitespace_scan(n, i, kind, cur, acc):
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = char_at(n, i)
        s = start_at(n, i)
        e = end_at(n, i)
        if is_whitespace(cp):
            return whitespace_scan(n, i + 1, -1, empty_norm(), close(cur, acc))
        else:
            k = 0 if is_word_char(cp) else 1
            if len(cur.chars) > 0 and k != kind:
                return whitespace_scan(n, i + 1, k, append_norm(empty_norm(), cp, s, e), listAppend(acc, cur))
            else:
                return whitespace_scan(n, i + 1, k, append_norm(cur, cp, s, e), acc)


def whitespace_split(n):
    """Split on every whitespace character, dropped."""
    return split_removed(n, is_whitespace, 0, empty_norm(), [])


def punctuation_pretokenize(n):
    """Split on punctuation (ASCII punct plus Unicode P*), isolated."""
    return split_isolated(n, is_punctuation, 0, empty_norm(), [])


def digits_pretokenize(n, individual):
    """Split on numeric characters: contiguous runs or each digit alone."""
    if individual:
        return split_isolated(n, is_numeric, 0, empty_norm(), [])
    else:
        return split_contiguous(n, is_numeric, 0, -1, empty_norm(), [])


def char_delimiter_split(n, delim):
    """Split on a single delimiter character, dropped."""
    return split_removed(n, lambda cp: cp == delim, 0, empty_norm(), [])


def starts_with_meta(n):
    return len(n.chars) > 0 and unwrap_or(listGet(n.chars, 0), 0) == META_CP


def metaspace_pretokenize(n, replacement, scheme, split):
    """Metaspace: replace spaces with the meta char, prepend per scheme, split."""
    replaced = replace_space(n, replacement)
    with_prefix = prepend_meta(replaced, scheme)
    if split:
        return split_merged_next(with_prefix, lambda cp: cp == replacement, 0, empty_norm(), [])
    else:
        return [with_prefix]


def prepend_meta(n, scheme):
    """scheme 0=always, 1=first, 2=never."""
    if scheme == 0:
        return n if starts_with_meta(n) else prepend_cp(n, META_CP)
    elif scheme == 1:
        at_zero = len(n.chars) > 0 and unwrap_or(listGet(n.starts, 0), -1) == 0
        return n if starts_with_meta(n) or not at_zero else prepend_cp(n, META_CP)
    else:
        return n


# ---------------------------------------------------------------------------
# The Qwen pre-tokenizer: a regex Split followed by ByteLevel
# ---------------------------------------------------------------------------

QWEN_PATTERN = "(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\\r\\n\\p{L}\\p{N}]?[\\p{L}\\p{M}]+|\\p{N}| ?[^\\s\\p{L}\\p{M}\\p{N}]+[\\r\\n]*|\\s*[\\r\\n]+|\\s+(?!\\S)|\\s+"
"""The pattern Qwen3.8's tokenizer.json splits with, `Split` in isolated mode.

Written out because the port scans it rather than running a regex engine: the
alternatives are tried left to right at each position and the first that matches
wins, with greedy quantifiers backtracking exactly as the `regex` crate does.
"""

BYTE_CHARS = [256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 287, 288, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 320, 321, 322, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 323, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255]
"""GPT-2's byte alphabet: byte b is spelled by the code point BYTE_CHARS[b]."""

CHAR_BYTES = [-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, -1, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 173]
"""The inverse of BYTE_CHARS, indexed by code point; -1 where no byte maps here."""


def piece_of(n, start, stop):
    """One piece as a Norm: a contiguous run of the input's characters."""
    return fold(range(start, stop), empty_norm(), lambda acc, i: append_norm(acc, char_at(n, i), start_at(n, i), end_at(n, i)))


class Pieces:
    """The split so far, and the index the next piece starts at."""
    pieces: list[Norm]
    next: int


def qwen_split(n):
    """The reference Split pre-tokenizer's pieces, left to right.

    A fold over the characters, so a long prompt can't exhaust the stack. A
    position no alternative matches becomes a one-character piece, which is how
    `Split` treats the text between matches; the pattern is designed not to leave
    any, so this keeps the split total.
    """
    return fold(range(len(n.chars)), Pieces(pieces=[], next=0), lambda acc, i: split_step(n, acc, i)).pieces


def split_step(n, acc, i):
    if i < acc.next:
        return acc
    else:
        width = span(qwen_piece_len(n, i))
        return Pieces(pieces=listAppend(acc.pieces, piece_of(n, i, i + width)), next=i + width)


def span(width):
    """A match width, where 0 means no alternative matched."""
    return width if width > 0 else 1


def qwen_piece_len(n, i):
    """How many characters the leftmost match at i takes, or 0 for none."""
    contract = contraction_len(n, i)
    if contract > 0:
        return contract
    else:
        return rest_len(n, i)


def rest_len(n, i):
    letters = letter_run_len(n, i)
    if letters > 0:
        return letters
    elif Category.is_number(cp_at(n, i)):
        return 1
    else:
        return tail_len(n, i)


def tail_len(n, i):
    symbols = symbol_run_len(n, i)
    if symbols > 0:
        return symbols
    else:
        return space_alt_len(n, i)


def space_alt_len(n, i):
    newlines = newline_alt_len(n, i)
    if newlines > 0:
        return newlines
    else:
        return bounded_space_len(n, i)


def bounded_space_len(n, i):
    """`\\s+(?!\\S)`: the whole whitespace run when it reaches the end, else all
    but its last character; a lone trailing space fails, and `\\s+` takes it."""
    run = space_run(n, i)
    if run == 0 or (run == 1 and i + run < len(n.chars)):
        return 0
    elif i + run >= len(n.chars):
        return run
    else:
        return run - 1


def cp_at(n, i):
    """Code point i, or -1 past the end: the scanners' safe read."""
    return char_at(n, i) if i < len(n.chars) and i >= 0 else -1


def is_line_break(cp):
    return cp == 13 or cp == 10


def is_letterish(cp):
    """`\\p{L}` or `\\p{M}`: a letter or a combining mark."""
    return Category.is_letter(cp) or Category.is_mark(cp)


def is_letter_or_mark_space(cp):
    """A character `\\p{L}\\p{M}` or `\\s` covers."""
    return is_letterish(cp) or is_whitespace(cp)


def is_symbol(cp):
    """A character outside `\\s`, `\\p{L}`, `\\p{M}` and `\\p{N}`."""
    return not is_letter_or_mark_space(cp) and not Category.is_number(cp)


def is_b_prefix(cp):
    """`[^\\r\\n\\p{L}\\p{N}]`: what may stand before a letter run."""
    return not is_line_break(cp) and not Category.is_letter(cp) and not Category.is_number(cp)


def same_ascii(cp, letter):
    """Whether cp is `letter` in either ASCII case."""
    return cp == letter or cp == letter - 32


def contraction_len(n, i):
    """`(?i:'s|'t|'re|'ve|'m|'ll|'d)`: only ASCII case folding applies, so the
    long s of a quoted U+017F is not a contraction."""
    if char_at(n, i) != 39:
        return 0
    elif same_ascii(cp_at(n, i + 1), 115) or same_ascii(cp_at(n, i + 1), 116) or same_ascii(cp_at(n, i + 1), 109) or same_ascii(cp_at(n, i + 1), 100):
        return 2
    elif same_ascii(cp_at(n, i + 1), 114) and same_ascii(cp_at(n, i + 2), 101):
        return 3
    elif same_ascii(cp_at(n, i + 1), 118) and same_ascii(cp_at(n, i + 2), 101):
        return 3
    elif same_ascii(cp_at(n, i + 1), 108) and same_ascii(cp_at(n, i + 2), 108):
        return 3
    else:
        return 0


def letter_run_len(n, i):
    """`[^\\r\\n\\p{L}\\p{N}]?[\\p{L}\\p{M}]+`, the optional character
    tried first because it is greedy."""
    if is_b_prefix(cp_at(n, i)) and is_letterish(cp_at(n, i + 1)):
        return 1 + letterish_run(n, i + 1)
    elif is_letterish(cp_at(n, i)):
        return letterish_run(n, i)
    else:
        return 0


def letterish_run(n, i):
    return count_while(n, i, 1)


def symbol_run_len(n, i):
    """` ?[^\\s\\p{L}\\p{M}\\p{N}]+[\\r\\n]*`."""
    start = i + 1 if cp_at(n, i) == 32 and is_symbol(cp_at(n, i + 1)) else i
    symbols = symbol_run(n, start)
    if symbols == 0:
        return 0
    else:
        return (start - i) + symbols + line_break_run(n, start + symbols)


def symbol_run(n, i):
    return count_while(n, i, 2)


def line_break_run(n, i):
    return count_while(n, i, 3)


def space_run(n, i):
    return count_while(n, i, 0)


def in_class(cp, kind):
    """Whether cp belongs to one of the scanners' character classes."""
    if kind == 0:
        return is_whitespace(cp)
    elif kind == 1:
        return is_letterish(cp)
    elif kind == 2:
        return is_symbol(cp)
    else:
        return is_line_break(cp)


class Run:
    """A greedy run count: how many characters matched, and whether the run ended."""
    count: int
    done: bool


def count_while(n, i, kind):
    """The length of the maximal run from i of characters in `kind`."""
    return fold(range(i, len(n.chars)), Run(count=0, done=False), lambda acc, k: count_step(n, acc, k, kind)).count


def count_step(n, acc, k, kind):
    if acc.done:
        return acc
    elif in_class(char_at(n, k), kind):
        return Run(count=acc.count + 1, done=False)
    else:
        return Run(count=acc.count, done=True)


def newline_alt_len(n, i):
    """`\\s*[\\r\\n]+`: the whitespace run up to its last line break, then the
    break run from there."""
    run = space_run(n, i)
    last = last_line_break(n, i, run)
    if last < 0:
        return 0
    else:
        return (last - i) + line_break_run(n, last)


def last_line_break(n, i, run):
    """The last line break inside the run that starts at i, or -1."""
    return fold(range(i, i + run), -1, lambda acc, k: k if is_line_break(char_at(n, k)) else acc)


def qwen_pretokenize(n):
    """The Qwen pre-tokenizer's Sequence: Split, then ByteLevel on each piece."""
    return mapList(qwen_split(n), bytelevel_piece)


def bytelevel_piece(n):
    """ByteLevel without its regex: every byte of the piece becomes one symbol,
    each keeping the original character span it came from, so a token that covers
    part of a character still reports that whole character."""
    return fold(range(len(n.chars)), empty_norm(), lambda acc, i: append_char_bytes(acc, n, i))


def append_char_bytes(acc, n, i):
    cp = char_at(n, i)
    start = start_at(n, i)
    stop = end_at(n, i)
    octets = utf8_of_cp(cp, [])
    return fold(range(len(octets)), acc, lambda a, k: append_norm(a, byte_char(unwrap_or(listGet(octets, k), 0)), start, stop))


def byte_char(byte):
    """The code point that spells a byte."""
    return unwrap_or(listGet(BYTE_CHARS, byte), -1)


def byte_of_char(cp):
    """The byte a byte-level code point spells, or -1 when it spells none."""
    return unwrap_or(listGet(CHAR_BYTES, cp), -1) if cp >= 0 else -1


class Octets:
    """The bytes read so far and the index of the first character outside the
    byte-level alphabet (-1 while every character is one)."""
    octets: list[int]
    bad: int


def bytelevel_raw(text) -> Result[list[int], str]:
    """The bytes a byte-level spelling stands for; a character outside the
    alphabet means the text is not a byte-level spelling at all."""
    cps = text_cps(text)
    st = fold(range(len(cps)), Octets(octets=[], bad=-1), lambda acc, i: octet_step(acc, cps, i))
    if st.bad >= 0:
        return Error(message="character " + toString(unwrap_or(listGet(cps, st.bad), -1)) + " is not a byte-level symbol")
    else:
        return Success(value=st.octets)


def octet_step(acc, cps, i):
    if acc.bad >= 0:
        return acc
    else:
        byte = byte_of_char(unwrap_or(listGet(cps, i), -1))
        return Octets(octets=acc.octets, bad=i) if byte < 0 else Octets(octets=listAppend(acc.octets, byte), bad=-1)


def bytelevel_text(octets):
    """The byte-level spelling of a byte list."""
    return join(toList(map(range(len(octets)), lambda i: unwrap_or(fromCodePoint(byte_char(unwrap_or(listGet(octets, i), 0))), "?"))), "")
