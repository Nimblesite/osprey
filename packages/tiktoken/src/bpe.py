# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=tiktoken
# osprey: module=Bpe
"""The byte-pair merge core, the canonical token key encoding and the
special-token scan.

tiktoken's vocabulary maps token bytes to ranks. An Osprey map keys on
strings, so a byte sequence gets one canonical key: each byte b becomes the
code point b + 1 (code point 0 cannot live in an Osprey string, so the shift
keeps every byte representable). The encoding is injective and order
preserving, and concatenating two keys is the key of the concatenated bytes —
which is what makes a merge a string concatenation. Training counts pairs by
`first + SEP + second`, with SEP the code point 257 that no key contains.

`bpe_encode` merges the lowest-ranked adjacent pair until none remains,
exactly the loop in `tiktoken._educational.bpe_encode` and the Rust core:
any pair a merge creates ranks higher than the merge itself, so merging every
occurrence of the best pair at once is the same sequence of merges. A piece
byte with no rank means a broken vocabulary, which errors like the Rust
core's panic.
"""

from tiktoken import Unicode

__all__ = [
    "SEP", "NO_RANK", "Ranked", "Best", "Hit", "Pair",
    "key_char", "key", "bytes_of_key", "pair_key", "base_ranks",
    "ranks_with_merges", "entries_by_rank", "key_of_rank", "merges_of",
    "bpe_encode", "merge_all", "earliest_special",
]

# The pair separator: code point 257, above every key character (1..256).
SEP = "Ā"

# A rank no pair has: smaller than every real rank's comparison sentinel.
NO_RANK = 2147483647


def key_char(b):
    """A byte's key character: the code point b + 1."""
    return unwrap_or(fromCodePoint(b + 1), "?")


def key(bs):
    """The canonical key of a byte sequence."""
    return fold(range(len(bs)), "", lambda acc, i: acc + key_char(Unicode.at(bs, i)))


def bytes_of_key(k):
    """The byte sequence a canonical key stands for."""
    return mapList(Unicode.code_points(k), lambda cp: cp - 1)


def pair_key(a, b):
    """The training-count key of the adjacent pair (a, b)."""
    return a + SEP + b


def base_ranks():
    """The 256 single-byte ranks every tiktoken vocabulary starts from."""
    return fold(range(256), {}, lambda acc, b: mapSet(acc, key_char(b), b))


def ranks_with_merges(merges):
    """The base 256 ranks plus one merge per entry, ranked 256 + i in order."""
    return fold(range(len(merges)), base_ranks(), lambda acc, i: mapSet(acc, key(Unicode.at_list(merges, i)), 256 + i))


class Ranked:
    """One vocabulary entry: a token's key and its rank."""
    key: str
    rank: int


def entries_by_rank(ranks):
    """The entries sorted by rank, for rank-keyed binary search."""
    entries = mapList(mapKeys(ranks), lambda k: Ranked(key=k, rank=unwrap_or(mapGet(ranks, k), -1)))
    return listSortBy(entries, lambda a, b: a.rank - b.rank)


def key_of_rank(entries, rank, lo, hi) -> Result[str, str]:
    """The key of the entry with this rank, by binary search over the sorted
    entries; Error when no token has the rank."""
    if lo > hi:
        return Error(message=f"no token with rank {rank}")
    else:
        mid = (lo + hi) // 2
        entry = unwrap_or(listGet(entries, mid), Ranked(key="", rank=-1))
        if rank < entry.rank:
            return key_of_rank(entries, rank, lo, mid - 1)
        elif rank > entry.rank:
            return key_of_rank(entries, rank, mid + 1, hi)
        else:
            return Success(value=entry.key)


def merges_of(ranks):
    """The merges a vocabulary holds, in rank order, as byte lists (the base
    256 single bytes excluded)."""
    entries = filterList(entries_by_rank(ranks), lambda e: e.rank >= 256)
    return mapList(entries, lambda e: bytes_of_key(e.key))


def bpe_encode(ranks, piece) -> Result[list[int], str]:
    """Byte-pair encode one piece's bytes into token ids."""
    syms = toList(map(range(len(piece)), lambda i: key_char(Unicode.at(piece, i))))
    return encode_syms(ranks, syms)


def encode_syms(ranks, syms):
    """Merge the best-ranked adjacent pair until none remains, then read the
    ids off the symbols."""
    best = best_pair(ranks, syms)
    if best.index < 0:
        return ids_of(ranks, syms, 0, [])
    else:
        return encode_syms(ranks, merge_all(syms, Unicode.at_str(syms, best.index), Unicode.at_str(syms, best.index + 1), 0, []))


class Best:
    """The best pair found: its rank and where; index -1 when none merges."""
    rank: int
    index: int


def best_pair(ranks, syms):
    """The leftmost adjacent pair with the lowest merge rank."""
    return fold(range(0, len(syms) - 1), Best(rank=NO_RANK, index=-1), lambda best, i: better_pair(ranks, syms, best, i))


def better_pair(ranks, syms, best, i):
    """Whether the pair at i beats the best so far: strictly lower rank wins,
    so the leftmost occurrence of the minimum is kept."""
    rank = unwrap_or(mapGet(ranks, Unicode.at_str(syms, i) + Unicode.at_str(syms, i + 1)), NO_RANK)
    return Best(rank=rank, index=i) if rank < best.rank else best


def merge_all(syms, first, second, i, acc) -> list[str]:
    """Merge every non-overlapping occurrence of (first, second), left to
    right; the merged symbol's key is the pair's concatenated key."""
    if i >= len(syms):
        return acc
    else:
        s = Unicode.at_str(syms, i)
        if s == first and i + 1 < len(syms) and Unicode.at_str(syms, i + 1) == second:
            return merge_all(syms, first, second, i + 2, listAppend(acc, first + second))
        else:
            return merge_all(syms, first, second, i + 1, listAppend(acc, s))


def ids_of(ranks, syms, i, acc) -> Result[list[int], str]:
    """The id of each symbol; a symbol with no rank means the vocabulary is
    missing a byte, which errors like the Rust core's panic."""
    if i >= len(syms):
        return Success(value=acc)
    else:
        match mapGet(ranks, Unicode.at_str(syms, i)):
            case Success(value=value):
                return ids_of(ranks, syms, i + 1, listAppend(acc, value))
            case Error():
                return Error(message=f"no token for byte sequence {Unicode.ints_text(bytes_of_key(Unicode.at_str(syms, i)))}")


class Hit:
    """The earliest special-token match: where it starts, its text and where
    it ends; start -1 when nothing matched."""
    start: int
    token: str
    next: int


def earliest_special(tokens, cps, start):
    """The earliest match of any of the tokens (a list already in the order
    ties break in) at or after `start`, or start -1."""
    tcps = mapList(tokens, Unicode.code_points)
    return scan_from(tokens, tcps, cps, start)


def scan_from(tokens, tcps, cps, i) -> Hit:
    if i >= len(cps):
        return Hit(start=-1, token="", next=-1)
    else:
        hit = scan_at(tokens, tcps, cps, i)
        if hit.start >= 0:
            return hit
        else:
            return scan_from(tokens, tcps, cps, i + 1)


def scan_at(tokens, tcps, cps, i):
    """The first token in list order that matches at exactly position i."""
    return fold(range(len(tokens)), Hit(start=-1, token="", next=-1), lambda acc, t: try_token(tokens, tcps, cps, i, acc, t))


def try_token(tokens, tcps, cps, i, acc, t):
    if acc.start >= 0:
        return acc
    elif match_at(cps, i, Unicode.at_list(tcps, t), 0):
        return Hit(start=i, token=Unicode.at_str(tokens, t), next=i + len(Unicode.at_list(tcps, t)))
    else:
        return acc


def match_at(cps, p, tcps, k):
    """Whether cps[p:] starts with the token's code points from k on."""
    if k >= len(tcps):
        return True
    else:
        return p + k < len(cps) and Unicode.at(cps, p + k) == Unicode.at(tcps, k) and match_at(cps, p, tcps, k + 1)


class Pair:
    """An adjacent token pair during training."""
    first: str
    second: str
