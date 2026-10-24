# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=tiktoken
# osprey: module=Core
"""`tiktoken.Encoding`: the byte-level BPE codec with special tokens.

The record holds what the Python constructor takes (name, mergeable ranks
keyed canonically, special tokens, the pattern) plus what the decoders derive
(the rank-sorted entries, the max token value and n_vocab). Encoding splits
its input with the pattern, byte-pair encodes each piece, and scans allowed
special tokens before the pattern like the Rust core does — earliest match
wins, and the alphabetically first token wins a same-position tie, which is
the sorted alternation the Rust core builds. Special tokens the caller did
not allow make `encode` fail instead of raising ValueError; unknown token
ids make the decoders fail instead of raising KeyError.
"""

from tiktoken import Bpe, Pattern, Unicode

__all__ = [
    "Encoding", "build", "encode_ordinary", "encode", "encode_range",
    "encode_single_token", "decode", "decode_errors", "decode_bytes",
    "decode_single_token_bytes", "decode_tokens_bytes", "token_byte_values",
    "is_special_token", "special_tokens_set",
]


class Encoding:
    """A byte-level BPE codec: name, mergeable ranks (canonical keys),
    special tokens, pattern, and the derived decoders' index."""
    name: str
    ranks: dict[str, int]
    special_tokens: dict[str, int]
    pattern: Pattern.Pattern
    max_token_value: int
    n_vocab: int
    entries: list[Bpe.Ranked]


def build(name, mergeable_ranks, special_tokens, pattern):
    """The `tiktoken.Encoding` constructor: derives max_token_value, n_vocab
    and the rank-sorted entries. Unlike CPython's, an empty mergeable_ranks
    does not raise; n_vocab is simply 1 plus the largest id."""
    mx = Unicode.max_int(max_of_values(mergeable_ranks), max_of_values(special_tokens))
    return Encoding(
        name=name,
        ranks=mergeable_ranks,
        special_tokens=special_tokens,
        pattern=pattern,
        max_token_value=mx,
        n_vocab=mx + 1,
        entries=Bpe.entries_by_rank(mergeable_ranks),
    )


def max_of_values(m):
    """The largest value in a string-keyed int map, 0 when empty."""
    return foldList(mapValues(m), 0, Unicode.max_int)


def encode_ordinary(enc, text) -> Result[list[int], str]:
    """Encode text ignoring special tokens: `encode(text, disallowed_special=())`."""
    cps = Unicode.code_points(text)
    return encode_range(enc.ranks, enc.pattern, cps, 0, len(cps))


def encode_range(ranks, pattern, cps, start, stop) -> Result[list[int], str]:
    """Split cps[start:stop] by the pattern and byte-pair encode each piece."""
    pieces = Pattern.split_cps(pattern, cps, start, stop)
    return encode_pieces(ranks, pieces, 0, [])


def encode_pieces(ranks, pieces, i, acc) -> Result[list[int], str]:
    """Byte-pair encode each piece and concatenate the token id lists."""
    if i >= len(pieces):
        return Success(value=acc)
    else:
        match Bpe.bpe_encode(ranks, Unicode.at_list(pieces, i)):
            case Success(value=value):
                return encode_pieces(ranks, pieces, i + 1, listConcat(acc, value))
            case Error(message=message):
                return Error(message=message)


def encode(enc, text, allowed_special):
    """Encode text with special tokens. tiktoken's default policy applies:
    a special token the caller did not allow makes this fail (ValueError in
    CPython, Error here), the allowed ones are matched before the pattern."""
    cps = Unicode.code_points(text)
    disallowed = specials_except(enc.special_tokens, allowed_special)
    hit = Bpe.earliest_special(disallowed, cps, 0)
    if hit.start >= 0:
        return Error(message=f"Encountered text corresponding to disallowed special token '{hit.token}'.")
    else:
        return encode_specials(enc, cps, allowed_special, 0, [])


def encode_specials(enc, cps, allowed_special, i, acc) -> Result[list[int], str]:
    """The Rust core's loop: scan for the earliest allowed special, encode the
    text before it ordinarily, emit its id, continue after it."""
    allowed = specials_within(enc.special_tokens, allowed_special)
    hit = Bpe.earliest_special(allowed, cps, i)
    if hit.start < 0:
        match encode_range(enc.ranks, enc.pattern, cps, i, len(cps)):
            case Success(value=value):
                return Success(value=listConcat(acc, value))
            case Error(message=message):
                return Error(message=message)
    else:
        id = unwrap_or(mapGet(enc.special_tokens, hit.token), -1)
        match encode_range(enc.ranks, enc.pattern, cps, i, hit.start):
            case Success(value=value):
                return encode_specials(enc, cps, allowed_special, hit.next, listAppend(listConcat(acc, value), id))
            case Error(message=message):
                return Error(message=message)


def specials_except(specials, allowed):
    """The special tokens not in allowed, sorted: the order that breaks
    same-position ties when reporting the disallowed one."""
    keys = filterList(mapKeys(specials), lambda k: not listContains(allowed, k))
    return listSortBy(keys, Unicode.cmp_text)


def specials_within(specials, allowed):
    """The special tokens in allowed, sorted like the Rust core sorts its
    special-token alternation."""
    keys = filterList(mapKeys(specials), lambda k: listContains(allowed, k))
    return listSortBy(keys, Unicode.cmp_text)


def encode_single_token(enc, text):
    """The id of the token whose bytes text spells: mergeable first, then
    special, else Error (KeyError in CPython)."""
    match mapGet(enc.ranks, Bpe.key(Unicode.utf8_text(text))):
        case Success(value=value):
            return Success(value=value)
        case Error():
            return single_special(enc, text)


def single_special(enc, text):
    match mapGet(enc.special_tokens, text):
        case Success(value=value):
            return Success(value=value)
        case Error():
            return Error(message=f"token {text} not in vocabulary")


def decode(enc, tokens):
    """Decode tokens to text with errors='replace', tiktoken's default."""
    return decode_errors(enc, tokens, "replace")


def decode_errors(enc, tokens, errors):
    """Decode tokens to text under a Python `errors` policy."""
    match decode_bytes(enc, tokens):
        case Success(value=value):
            return Unicode.decode_utf8(value, errors)
        case Error(message=message):
            return Error(message=message)


def decode_bytes(enc, tokens):
    """The bytes a token list spells; an unknown token id fails (KeyError)."""
    return bytes_of_tokens(enc, tokens, 0, [])


def bytes_of_tokens(enc, tokens, i, acc) -> Result[list[int], str]:
    if i >= len(tokens):
        return Success(value=acc)
    else:
        match token_bytes(enc, Unicode.at(tokens, i)):
            case Success(value=value):
                return bytes_of_tokens(enc, tokens, i + 1, listConcat(acc, value))
            case Error(message=message):
                return Error(message=message)


def token_bytes(enc, token):
    """One token's bytes: mergeable by rank, else the special token with that id."""
    match Bpe.key_of_rank(enc.entries, token, 0, len(enc.entries) - 1):
        case Success(value=value):
            return Success(value=Bpe.bytes_of_key(value))
        case Error():
            return special_bytes(enc, token)


def special_bytes(enc, token):
    """The special token's bytes behind an id, or Error."""
    keys = mapKeys(enc.special_tokens)
    hit = fold(range(len(keys)), "", lambda acc, i: special_at(enc, token, acc, Unicode.at_str(keys, i)))
    if hit == "":
        return Error(message=f"Invalid token for decoding: {token}")
    else:
        return Success(value=Unicode.utf8_text(hit))


def special_at(enc, token, acc, k):
    return k if acc == "" and unwrap_or(mapGet(enc.special_tokens, k), -1) == token else acc


def decode_single_token_bytes(enc, token):
    """The bytes one token spells; unknown ids fail (KeyError)."""
    return token_bytes(enc, token)


def decode_tokens_bytes(enc, tokens):
    """Each token's bytes, in order; unknown ids fail (KeyError)."""
    return tokens_bytes_of(enc, tokens, 0, [])


def tokens_bytes_of(enc, tokens, i, acc) -> Result[list[list[int]], str]:
    if i >= len(tokens):
        return Success(value=acc)
    else:
        match token_bytes(enc, Unicode.at(tokens, i)):
            case Success(value=value):
                return tokens_bytes_of(enc, tokens, i + 1, listAppend(acc, value))
            case Error(message=message):
                return Error(message=message)


def token_byte_values(enc):
    """Every mergeable token's bytes in lexicographic byte order, like the
    Rust core; special tokens are not included."""
    entries = listSortBy(enc.entries, lambda a, b: Unicode.cmp_text(a.key, b.key))
    return mapList(entries, lambda e: Bpe.bytes_of_key(e.key))


def is_special_token(enc, token):
    """Whether an id belongs to a special token."""
    return listContains(mapValues(enc.special_tokens), token)


def special_tokens_set(enc):
    """The special token strings, sorted (CPython's is a set)."""
    return listSortBy(mapKeys(enc.special_tokens), Unicode.cmp_text)
