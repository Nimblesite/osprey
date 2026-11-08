# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=Nfc
"""Unicode canonical normalization: NFD and NFC over code point lists.

The tables in `NfcData` are the reference `tokenizers` package's own Unicode
data, recovered from its NFD/NFC output (see src/osprey/packages/tokenizers/oracle/tokenizers.cpython).
Combining classes are stored as ranks in that ordering, which is the whole
truth canonical ordering needs: it only ever compares two classes and asks
whether one is zero.

Hangul syllables decompose arithmetically, and their L+V and LV+T compositions
are part of NFC, so both live here rather than in the table. Implements
[LLM-TOKENIZER]'s NFC normalizer.
"""
from tokenizers import NfcData
from tokenizers.Unicode import cps_text, map_indexed, text_cps

__all__ = [
    "combining_class", "decompose_cp", "decompose", "canonical_order",
    "composite", "compose", "nfd_cps", "nfc_cps", "nfd_text", "nfc_text",
    "HANGUL_SBASE", "HANGUL_SCOUNT",
]

HANGUL_SBASE = 44032
"""The first Hangul syllable, U+AC00."""

HANGUL_SCOUNT = 11172
"""How many Hangul syllables there are, 19 x 21 x 28."""

HANGUL_LBASE = 4352
HANGUL_VBASE = 4449
HANGUL_TBASE = 4519
HANGUL_LCOUNT = 19
HANGUL_VCOUNT = 21
HANGUL_TCOUNT = 28
HANGUL_NCOUNT = 588


def at(xs, i):
    return unwrap_or(listGet(xs, i), 0)


def search(keys, target, lo, hi):
    """The index of target in a sorted list of ints, or -1."""
    if lo > hi:
        return -1
    else:
        mid = (lo + hi) // 2
        pivot = at(keys, mid)
        if target < pivot:
            return search(keys, target, lo, mid - 1)
        elif target > pivot:
            return search(keys, target, mid + 1, hi)
        else:
            return mid


def slice_list(xs, start, stop):
    """xs[start:stop]."""
    return toList(map(range(start, stop), lambda i: at(xs, i)))


def combining_class(cp):
    """The canonical combining class of cp, as its rank among the classes;
    0 for a starter."""
    index = search(NfcData.CCC_KEYS, cp, 0, len(NfcData.CCC_KEYS) - 1)
    return at(NfcData.CCC_VALUES, index) if index >= 0 else 0


def decompose_cp(cp):
    """The full canonical decomposition of one code point."""
    if cp >= HANGUL_SBASE and cp < HANGUL_SBASE + HANGUL_SCOUNT:
        return hangul_jamo(cp)
    else:
        index = search(NfcData.DECOMP_KEYS, cp, 0, len(NfcData.DECOMP_KEYS) - 1)
        if index < 0:
            return [cp]
        else:
            start = at(NfcData.DECOMP_OFFSETS, index)
            return slice_list(NfcData.DECOMP_CHARS, start, at(NfcData.DECOMP_OFFSETS, index + 1))


def hangul_jamo(cp):
    """A syllable's leading, vowel and trailing jamo, trailing omitted at 0."""
    index = cp - HANGUL_SBASE
    lead = HANGUL_LBASE + index // HANGUL_NCOUNT
    vowel = HANGUL_VBASE + (index % HANGUL_NCOUNT) // HANGUL_TCOUNT
    trail = index % HANGUL_TCOUNT
    return [lead, vowel] if trail == 0 else [lead, vowel, HANGUL_TBASE + trail]


def decompose(cps):
    """The full canonical decomposition of every code point, in order: each one
    appended, never the whole text re-copied per character."""
    return fold(range(len(cps)), [], lambda acc, i: foldList(decompose_cp(at(cps, i)), acc, lambda out, cp: listAppend(out, cp)))


class Ordering:
    """The ordered characters so far and the open run of non-starters: canonical
    ordering only ever sorts inside such a run."""
    out: list[int]
    run: list[int]


def canonical_order(cps):
    """Stable sort of every run of non-starters by combining class.

    A fold, so a long text can't exhaust the stack.
    """
    return flush_run(fold(range(len(cps)), Ordering(out=[], run=[]), lambda acc, i: order_step(acc, at(cps, i))))


def order_step(state, cp):
    if combining_class(cp) == 0:
        return Ordering(out=listAppend(flush_run(state), cp), run=[])
    else:
        return Ordering(out=state.out, run=listAppend(state.run, cp))


def flush_run(state):
    """The open run sorted by class and appended to the output."""
    return foldList(sort_run(state.run), state.out, lambda acc, cp: listAppend(acc, cp))


def sort_run(run):
    """Insertion sort that keeps equal classes in their original order."""
    return fold(range(len(run)), [], lambda acc, i: insert_run(acc, at(run, i)))


def insert_run(acc, cp):
    """Append cp after every element whose class is at most its own."""
    return insert_from(acc, cp, combining_class(cp), 0)


def insert_from(acc, cp, cc, i):
    if i >= len(acc) or combining_class(at(acc, i)) > cc:
        return listConcat(listConcat(slice_list(acc, 0, i), [cp]), slice_list(acc, i, len(acc)))
    else:
        return insert_from(acc, cp, cc, i + 1)


def composite(a, b):
    """The primary composite a + b spells, or -1."""
    if a < 0:
        return -1
    else:
        index = search(NfcData.COMP_KEYS, a * 1114112 + b, 0, len(NfcData.COMP_KEYS) - 1)
        return at(NfcData.COMP_VALUES, index) if index >= 0 else hangul_composite(a, b)


def hangul_composite(a, b):
    """The syllable leading + vowel or syllable + trailing jamo spell, or -1."""
    if a >= HANGUL_LBASE and a < HANGUL_LBASE + HANGUL_LCOUNT and b >= HANGUL_VBASE and b < HANGUL_VBASE + HANGUL_VCOUNT:
        return HANGUL_SBASE + ((a - HANGUL_LBASE) * HANGUL_VCOUNT + (b - HANGUL_VBASE)) * HANGUL_TCOUNT
    elif a >= HANGUL_SBASE and a < HANGUL_SBASE + HANGUL_SCOUNT and (a - HANGUL_SBASE) % HANGUL_TCOUNT == 0 and b > HANGUL_TBASE and b < HANGUL_TBASE + HANGUL_TCOUNT:
        return a + (b - HANGUL_TBASE)
    else:
        return -1


class State:
    """The composition scan: what is built so far, the last starter's index and
    the class of the last character appended (0 when that was a starter)."""
    built: list[int]
    starter: int
    last: int


def compose(cps):
    """The NFC composition pass: combine a starter with a following character
    when nothing between them blocks it. A fold, so long text can't exhaust the
    stack."""
    built = fold(range(len(cps)), State(built=[], starter=-1, last=0), lambda acc, i: compose_step(acc, at(cps, i)))
    return built.built


def compose_step(state, cp):
    """Compose cp with the current starter when the pair has a composite and no
    character of equal or higher class stands between them."""
    cc = combining_class(cp)
    if state.starter >= 0 and (state.last < cc or state.last == 0):
        joined = composite(at(state.built, state.starter), cp)
        if joined >= 0:
            return State(built=replace_at(state.built, state.starter, joined), starter=state.starter, last=state.last)
        else:
            return push(state, cp, cc)
    else:
        return push(state, cp, cc)


def push(state, cp, cc):
    """Append a character; a starter becomes the new composition target."""
    return State(built=listAppend(state.built, cp), starter=len(state.built) if cc == 0 else state.starter, last=cc)


def replace_at(xs, position, value):
    return map_indexed(xs, lambda i: value if i == position else at(xs, i))


def nfd_cps(cps):
    """The canonical decomposition of a code point list."""
    return canonical_order(decompose(cps))


def nfc_cps(cps):
    """The canonical composition of a code point list."""
    return compose(canonical_order(decompose(cps)))


def nfd_text(text):
    """The NFD form of a string."""
    return cps_text(nfd_cps(text_cps(text)))


def nfc_text(text):
    """The NFC form of a string."""
    return cps_text(nfc_cps(text_cps(text)))
