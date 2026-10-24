# osprey: flavor=python
# osprey: namespace=sacrebleu
# osprey: module=Tokenizer
"""The `none`, `13a` and `char` tokenizers of sacrebleu.

`tokenize_13a` reproduces mteval-v13a: `<skipped>` and a dash before a newline
are removed, newlines become spaces, the four HTML entities are decoded, then
the TokenizerRegexp sequence is applied as explicit codepoint scans and
whitespace is collapsed. `tokenize_char` separates every codepoint with a
space. `tokenize_none` is the identity.
"""

from sacrebleu.Text import collapse_whitespace, codepoints, from_codepoints, is_ascii_digit

__all__ = ["tokenize", "tokenize_13a", "tokenize_char", "tokenize_none"]


def in_range(cp, lo, hi):
    """Whether codepoint `cp` lies in the inclusive range `lo`..`hi`."""
    return cp >= lo and cp <= hi


def symbol_class(cp):
    """mteval-v13a's symbol class: the ASCII punctuation surrounded with spaces."""
    return in_range(cp, 32, 38) or in_range(cp, 40, 43) or cp == 47 or in_range(cp, 58, 64) or in_range(cp, 91, 96) or in_range(cp, 123, 126)


def is_period_comma(cp):
    """Whether codepoint `cp` is `.` or `,`."""
    return cp == 46 or cp == 44


def rule2_from(cps, i, acc):
    """Period or comma after a non-digit becomes its own token."""
    if i >= len(cps):
        return acc
    else:
        a = unwrap_or(listGet(cps, i), 0)
        b = unwrap_or(listGet(cps, i + 1), 0)
        matchable = i + 1 < len(cps) and not is_ascii_digit(a) and is_period_comma(b)
        next_acc = listConcat(acc, [a, 32, b, 32]) if matchable else listAppend(acc, a)
        return rule2_from(cps, i + 2 if matchable else i + 1, next_acc)


def rule3_from(cps, i, acc):
    """Period or comma before a non-digit becomes its own token."""
    if i >= len(cps):
        return acc
    else:
        a = unwrap_or(listGet(cps, i), 0)
        b = unwrap_or(listGet(cps, i + 1), 0)
        matchable = i + 1 < len(cps) and is_period_comma(a) and not is_ascii_digit(b)
        next_acc = listConcat(acc, [32, a, 32, b]) if matchable else listAppend(acc, a)
        return rule3_from(cps, i + 2 if matchable else i + 1, next_acc)


def rule4_from(cps, i, acc):
    """A dash after a digit becomes its own token."""
    if i >= len(cps):
        return acc
    else:
        a = unwrap_or(listGet(cps, i), 0)
        b = unwrap_or(listGet(cps, i + 1), 0)
        matchable = i + 1 < len(cps) and is_ascii_digit(a) and b == 45
        next_acc = listConcat(acc, [a, 32, b, 32]) if matchable else listAppend(acc, a)
        return rule4_from(cps, i + 2 if matchable else i + 1, next_acc)


def rule1(cps):
    """Surround every symbol-class codepoint with spaces."""
    return foldList(cps, [], lambda acc, cp: listConcat(acc, [32, cp, 32]) if symbol_class(cp) else listAppend(acc, cp))


def rule2(cps):
    """The period/comma-after-non-digit rule applied to the whole codepoint list."""
    return rule2_from(cps, 0, [])


def rule3(cps):
    """The period/comma-before-non-digit rule applied to the whole codepoint list."""
    return rule3_from(cps, 0, [])


def rule4(cps):
    """The dash-after-digit rule applied to the whole codepoint list."""
    return rule4_from(cps, 0, [])


def tokenize_none(line):
    """`NoneTokenizer`: the identity tokenizer."""
    return line


def tokenize_char(line):
    """`TokenizerChar`: every codepoint separated by one space, nothing else."""
    return join(mapList(codepoints(line), lambda cp: unwrap_or(fromCodePoint(cp), "")), " ")


def replace13a(line):
    """mteval-v13a's language-independent edits before the regex sequence."""
    s1 = unwrap_or(replace(line, "<skipped>", ""), line)
    s2 = unwrap_or(replace(s1, "-\n", ""), s1)
    s3 = unwrap_or(replace(s2, "\n", " "), s2)
    return s3


def decode_entities(line):
    """The four HTML entities mteval-v13a decodes, in order."""
    s1 = unwrap_or(replace(line, "&quot;", "\""), line)
    s2 = unwrap_or(replace(s1, "&amp;", "&"), s1)
    s3 = unwrap_or(replace(s2, "&lt;", "<"), s2)
    return unwrap_or(replace(s3, "&gt;", ">"), s3)


def tokenize_13a(line):
    """`Tokenizer13a`: mteval-v13a tokenization of one line."""
    cleaned = decode_entities(replace13a(line))
    cps = codepoints(" " + cleaned + " ")
    return collapse_whitespace(from_codepoints(rule4(rule3(rule2(rule1(cps))))))


def tokenize(text, kind):
    """Tokenize `text` with the named tokenizer: `none`, `13a` or `char`."""
    if kind == "13a":
        return tokenize_13a(text)
    elif kind == "char":
        return tokenize_char(text)
    else:
        return tokenize_none(text)
