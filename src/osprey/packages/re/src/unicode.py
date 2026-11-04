# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Unicode
"""Code point decoding and Unicode classification for the regex matcher.

Strings are UTF-8; the matcher works over code points. `code_points` decodes
a string once, `text_of` rebuilds a span's text, and the `is_*`/`fold_*`
functions classify a code point against the range tables in `re.Tables`
(generated from CPython 3.12 by scripts/oracles/re.py). Implements the
Unicode-awareness the brief requires: `\\d` is any Unicode decimal digit,
`\\w` any alphanumeric plus underscore, `\\s` any Unicode whitespace.
"""

from re import Tables

__all__ = [
    "at", "at_str", "code_points", "text_of", "in_ranges", "fold_lower",
    "fold_upper", "is_digit", "is_word", "is_space", "is_newline", "utf8_of",
]


def at(xs, i):
    """Element `i` of an int list, or 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def at_str(xs, i):
    """Element `i` of a string list, or "" past the end."""
    return unwrap_or(listGet(xs, i), "")


def step_cps(s, i, acc):
    """Decode the code points of `s` starting at byte `i`."""
    if i >= len(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 65533)
        w = unwrap_or(codePointWidth(cp), 1)
        return step_cps(s, i + w, listAppend(acc, cp))


def code_points(s):
    """The code points of a string, in order."""
    return step_cps(s, 0, [])


def text_of(cps, start, end):
    """The string made of code points cps[start:end]."""
    return text_of_from(cps, start, end, "")


def text_of_from(cps, i, end, acc):
    """Append cps[i:end] to `acc` as a string."""
    if i >= end:
        return acc
    else:
        return text_of_from(cps, i + 1, end, acc + utf8_of(at(cps, i)))


def utf8_of(cp):
    """The one-code-point string for `cp`, or U+FFFD for an invalid value."""
    return unwrap_or(fromCodePoint(cp), "\uFFFD")


def in_ranges(ranges, cp):
    """Whether code point `cp` falls in one of the inclusive [lo, hi] ranges."""
    return ranges_hit(ranges, cp, 0)


def ranges_hit(ranges, cp, i):
    """Scan ranges[i:] for a range containing `cp`."""
    if i >= len(ranges):
        return False
    else:
        r = unwrap_or(listGet(ranges, i), [0, 0])
        return cp >= at(r, 0) and cp <= at(r, 1) or ranges_hit(ranges, cp, i + 1)


def fold_bsearch(pairs, cp, lo, hi):
    """Binary search the sorted [cp, folded] pairs; `cp` itself when absent."""
    if lo >= hi:
        return cp
    else:
        mid = intDiv(lo + hi, 2)
        p = unwrap_or(listGet(pairs, mid), [0, 0])
        key = at(p, 0)
        if cp == key:
            return at(p, 1)
        elif cp < key:
            return fold_bsearch(pairs, cp, lo, mid)
        else:
            return fold_bsearch(pairs, cp, mid + 1, hi)


def fold_lower(cp):
    """The simple lowercase of `cp`, itself when it has none."""
    return fold_bsearch(Tables.LOWER, cp, 0, len(Tables.LOWER))


def fold_upper(cp):
    """The simple uppercase of `cp`, itself when it has none."""
    return fold_bsearch(Tables.UPPER, cp, 0, len(Tables.UPPER))


def is_digit(cp):
    """Whether `cp` is a Unicode decimal digit (`\\d`)."""
    return in_ranges(Tables.DIGITS, cp)


def is_word(cp):
    """Whether `cp` is a Unicode word character (`\\w`)."""
    return in_ranges(Tables.WORDS, cp)


def is_space(cp):
    """Whether `cp` is Unicode whitespace (`\\s`)."""
    return in_ranges(Tables.SPACES, cp)


def is_newline(cp):
    """Whether `cp` is U+000A, the only codepoint anchors treat as a line end."""
    return cp == 10
