# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=sacrebleu
# osprey: module=Text
"""Codepoint primitives shared by the sacrebleu and rouge_score tokenizers.

Osprey strings are UTF-8 byte sequences and `codePointAt` decodes one codepoint
at a byte index. These helpers iterate a string's codepoints and rebuild a
string from them, which is what every tokenizer here does. They live in
sacrebleu (not duplicated) because @osprey/rouge_score depends on
@osprey/sacrebleu.
"""

__all__ = ["codepoints", "from_codepoints", "map_codepoints", "is_ascii_digit", "is_ascii_lower_alnum", "collapse_whitespace"]


def codepoints_from(s, i, acc):
    """The codepoints of `s` from byte index `i`, appended to `acc`."""
    if i >= length(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        return codepoints_from(s, i + unwrap_or(codePointWidth(cp), 1), listAppend(acc, cp))


def codepoints(s):
    """The Unicode codepoint value of each character of `s`, in order."""
    return codepoints_from(s, 0, [])


def from_codepoints(cps):
    """The string whose codepoints are `cps` (the inverse of `codepoints`)."""
    return foldList(cps, "", lambda acc, cp: acc + unwrap_or(fromCodePoint(cp), ""))


def map_codepoints(s, f):
    """`s` with every codepoint replaced by the codepoint `f` returns."""
    return from_codepoints(mapList(codepoints(s), f))


def is_ascii_digit(cp):
    """Whether codepoint `cp` is an ASCII digit `0`-`9`."""
    return cp >= 48 and cp <= 57


def is_ascii_lower_alnum(cp):
    """Whether codepoint `cp` is an ASCII lowercase letter or digit."""
    return (cp >= 97 and cp <= 122) or (cp >= 48 and cp <= 57)


def collapse_whitespace(s):
    """`s` with runs of ASCII whitespace folded to one space, edges trimmed."""
    return join(words(s), " ")
