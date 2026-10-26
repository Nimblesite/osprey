# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=Normalizers
"""The BertNormalizer: clean_text, handle_chinese_chars, strip_accents and
lowercase, operating on a NormalizedString that tracks byte alignments.

A `Norm` keeps the normalized text as a list of codepoints, and for every
codepoint the `[start, end)` byte range it came from in the original string.
Normalization changes codepoints but never loses the alignment, so the final
Encoding can report offsets into the untouched input, exactly as the real
`tokenizers` BertNormalizer does. Implements [PORT-SCOPE] normalizer behaviour
for @osprey/tokenizers.
"""
from tokenizers.Unicode import is_chinese_char, is_control, is_whitespace, lowercase, strip_base

__all__ = ["Norm", "norm_of", "norm_text", "bert_normalize", "replace_space", "prepend_cp"]


class Norm:
    """A normalized string with per-codepoint original byte ranges."""
    chars: list[int]
    starts: list[int]
    ends: list[int]


def norm_of(s):
    """A Norm over an unmodified string: each codepoint covers its own bytes."""
    return norm_build(s, 0, len(s), [], [], [])


def norm_build(s, i, n, chars, starts, ends):
    if i >= n:
        return Norm(chars=chars, starts=starts, ends=ends)
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        w = unwrap_or(codePointWidth(cp), 1)
        return norm_build(s, i + w, n, listAppend(chars, cp), listAppend(starts, i), listAppend(ends, i + w))


def norm_text(n):
    """The normalized string, rebuilt from its codepoints."""
    return cps_text(n.chars)


def cps_text(cps):
    return join(toList(map(range(len(cps)), lambda i: unwrap_or(fromCodePoint(unwrap_or(listGet(cps, i), 63)), "?"))), "")


def norm_expand(n, f):
    """Apply f (codepoint -> list of codepoints) to every char; each output
    codepoint inherits the input char's original byte range."""
    return fold(range(len(n.chars)), Norm(chars=[], starts=[], ends=[]), lambda acc, i: expand_one(acc, n, i, f))


def expand_one(acc, n, i, f):
    cp = unwrap_or(listGet(n.chars, i), 0)
    s = unwrap_or(listGet(n.starts, i), 0)
    e = unwrap_or(listGet(n.ends, i), 0)
    outs = f(cp)
    return fold(range(len(outs)), acc, lambda a2, k: Norm(chars=listAppend(a2.chars, unwrap_or(listGet(outs, k), 0)), starts=listAppend(a2.starts, s), ends=listAppend(a2.ends, e)))


def clean_text(n):
    """Drop NUL, U+FFFD and control characters; map every whitespace to space."""
    return fold(range(len(n.chars)), Norm(chars=[], starts=[], ends=[]), lambda acc, i: clean_one(acc, n, i))


def clean_one(acc, n, i):
    cp = unwrap_or(listGet(n.chars, i), 0)
    s = unwrap_or(listGet(n.starts, i), 0)
    e = unwrap_or(listGet(n.ends, i), 0)
    if cp == 0 or cp == 65533 or is_control(cp):
        return acc
    elif is_whitespace(cp):
        return Norm(chars=listAppend(acc.chars, 32), starts=listAppend(acc.starts, s), ends=listAppend(acc.ends, e))
    else:
        return Norm(chars=listAppend(acc.chars, cp), starts=listAppend(acc.starts, s), ends=listAppend(acc.ends, e))


def handle_chinese(n):
    """Put a space on each side of every CJK character, all three chars sharing
    the original char's byte range."""
    return fold(range(len(n.chars)), Norm(chars=[], starts=[], ends=[]), lambda acc, i: chinese_one(acc, n, i))


def chinese_one(acc, n, i):
    cp = unwrap_or(listGet(n.chars, i), 0)
    s = unwrap_or(listGet(n.starts, i), 0)
    e = unwrap_or(listGet(n.ends, i), 0)
    if is_chinese_char(cp):
        return Norm(
            chars=listAppend(listAppend(listAppend(acc.chars, 32), cp), 32),
            starts=listAppend(listAppend(listAppend(acc.starts, s), s), s),
            ends=listAppend(listAppend(listAppend(acc.ends, e), e), e),
        )
    else:
        return Norm(chars=listAppend(acc.chars, cp), starts=listAppend(acc.starts, s), ends=listAppend(acc.ends, e))


def strip_accents_norm(n):
    """NFD minus nonspacing marks, keeping each base char's byte range."""
    return norm_expand(n, strip_base)


def lowercase_norm(n):
    """to_lowercase per char; every output char shares the input char's range."""
    return norm_expand(n, lowercase)


def replace_space(n, replacement):
    """Replace each space with a single codepoint (Metaspace's step)."""
    return norm_expand(n, lambda cp: [replacement] if cp == 32 else [cp])


def prepend_cp(n, cp):
    """Prepend a codepoint that shares the first char's byte range (Metaspace's
    add_prefix_space step; no-op on an empty string)."""
    if len(n.chars) == 0:
        return n
    else:
        s0 = unwrap_or(listGet(n.starts, 0), 0)
        e0 = unwrap_or(listGet(n.ends, 0), 0)
        return Norm(chars=listConcat([cp], n.chars), starts=listConcat([s0], n.starts), ends=listConcat([e0], n.ends))


def bert_normalize(text, clean, handle, strip, lower):
    """BertNormalizer's normalize: clean, space out CJK, strip accents, lower."""
    n = norm_of(text)
    a = clean_text(n) if clean else n
    b = handle_chinese(a) if handle else a
    c = strip_accents_norm(b) if strip else b
    d = lowercase_norm(c) if lower else c
    return d
