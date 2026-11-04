# osprey: flavor=python
# osprey: namespace=tiktoken
# osprey: module=Pattern
"""tiktoken's pre-tokenizers, as hand-written scanners over code points.

There is no regex engine in Osprey, so each pattern's alternation is a
scanner: `piece_len` returns the length of the leftmost match at a position,
with the alternatives tried in the pattern's order and greedy runs, which is
the leftmost-first semantics of the `regex` module and the Rust core. The
GPT-2 pattern is
`'s|'t|'re|'ve|'m|'ll|'d| ?\\p{L}+| ?\\p{N}+| ?[^\\s\\p{L}\\p{N}]+|\\s+(?!\\S)|\\s+`
and cl100k_base's is
`(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\\r\\n\\p{L}\\p{N}]?\\p{L}+|\\p{N}{1,3}|
 ?[^\\s\\p{L}\\p{N}]+[\\r\\n]*|\\s*[\\r\\n]+|\\s+(?!\\S)|\\s+`
(written there with possessive quantifiers; the suite pins the equivalence).
Each piece leaves as its UTF-8 bytes, ready for byte-pair encoding.
"""

from tiktoken import Unicode

__all__ = ["Pattern", "Gpt2", "Cl100kBase", "split", "split_cps"]


class Pattern:
    """Which pre-tokenizer regex an encoding splits its input with, as a
    `kind` tag: "gpt2" or "cl100k_base" (a record until `@union` lands)."""
    kind: str


def Gpt2():
    """The GPT-2 / r50k_base / p50k_base pattern."""
    return Pattern(kind="gpt2")


def Cl100kBase():
    """The cl100k_base pattern, with its case-insensitive contractions,
    3-digit number chunks and CR/LF tails."""
    return Pattern(kind="cl100k_base")


def split(pattern, text):
    """The pieces the pattern splits text into, each as its UTF-8 bytes."""
    cps = Unicode.code_points(text)
    return split_cps(pattern, cps, 0, len(cps))


def split_cps(pattern, cps, start, stop):
    """The pieces of code points cps[start:stop], each as its UTF-8 bytes."""
    return split_from(pattern, cps, start, stop, [])


def split_from(pattern, cps, i, stop, acc):
    if i >= stop:
        return acc
    else:
        n = piece_len(pattern, cps, i, stop)
        return split_from(pattern, cps, i + n, stop, listAppend(acc, Unicode.utf8_range(cps, i, i + n)))


def piece_len(pattern, cps, i, stop):
    match pattern.kind:
        case "gpt2":
            return gpt2_piece(cps, i, stop)
        case _:
            return cl100k_piece(cps, i, stop)


def run_len(cps, i, stop, pred):
    """How many code points from i satisfy pred, before stop."""
    if i >= stop or not pred(Unicode.at(cps, i)):
        return 0
    else:
        return 1 + run_len(cps, i + 1, stop, pred)


def gpt2_piece(cps, i, stop):
    """The GPT-2 pattern's match length at i (0 at or past stop)."""
    if i >= stop:
        return 0
    else:
        cp = Unicode.at(cps, i)
        c = gpt2_contraction(cps, i, stop)
        if c > 0:
            return c
        elif cp == 32:
            return gpt2_spaced(cps, i, stop)
        elif Unicode.is_letter(cp):
            return run_len(cps, i, stop, Unicode.is_letter)
        elif Unicode.is_number(cp):
            return run_len(cps, i, stop, Unicode.is_number)
        elif Unicode.is_other(cp):
            return run_len(cps, i, stop, Unicode.is_other)
        else:
            return space_piece(cps, i, stop)


def gpt2_contraction(cps, i, stop):
    """'s|'t|'re|'ve|'m|'ll|'d: the case-sensitive contraction alternatives."""
    return contraction(cps, i, stop, lambda cp: cp)


def contraction(cps, i, stop, normalize):
    if Unicode.at(cps, i) != 39 or i + 1 >= stop:
        return 0
    else:
        second = normalize(Unicode.at(cps, i + 1))
        third = normalize(Unicode.at(cps, i + 2))
        if second == 115 or second == 116 or second == 109 or second == 100:
            return 2
        elif (second == 114 or second == 118) and third == 101:
            return 3
        elif second == 108 and third == 108:
            return 3
        else:
            return 0


def gpt2_spaced(cps, i, stop):
    """At a space: the ` ?`-prefixed alternatives (space + one run), else the
    whitespace alternatives."""
    nxt = Unicode.at(cps, i + 1)
    if i + 1 < stop and Unicode.is_letter(nxt):
        return 1 + run_len(cps, i + 1, stop, Unicode.is_letter)
    elif i + 1 < stop and Unicode.is_number(nxt):
        return 1 + run_len(cps, i + 1, stop, Unicode.is_number)
    elif i + 1 < stop and Unicode.is_other(nxt):
        return 1 + run_len(cps, i + 1, stop, Unicode.is_other)
    else:
        return space_piece(cps, i, stop)


def space_piece(cps, i, stop):
    """`\\s+(?!\\S)|\\s+` at i: a run followed by non-space keeps its last
    space for the next word, a lone whitespace char is the last resort, and a
    run to the end is one piece."""
    run = run_len(cps, i, stop, Unicode.is_space)
    if i + run >= stop:
        return run
    elif run > 1:
        return run - 1
    else:
        return 1


def cl100k_piece(cps, i, stop):
    """The cl100k_base pattern's match length at i (0 at or past stop)."""
    if i >= stop:
        return 0
    else:
        cp = Unicode.at(cps, i)
        c = cl_contraction(cps, i, stop)
        if c > 0:
            return c
        elif Unicode.is_letter(cp):
            return run_len(cps, i, stop, Unicode.is_letter)
        elif prefixable(cp) and i + 1 < stop and Unicode.is_letter(Unicode.at(cps, i + 1)):
            return 1 + run_len(cps, i + 1, stop, Unicode.is_letter)
        elif Unicode.is_number(cp):
            return number_chunk(cps, i, stop)
        elif cp == 32 and i + 1 < stop and Unicode.is_other(Unicode.at(cps, i + 1)):
            return 1 + other_crlf(cps, i + 1, stop)
        elif Unicode.is_other(cp):
            return other_crlf(cps, i, stop)
        elif Unicode.is_space(cp):
            return cl_space_piece(cps, i, stop)
        else:
            return 0


def cl_contraction(cps, i, stop):
    """(?i:'s|'t|'re|'ve|'m|'ll|'d): case-insensitive, with U+017F folding to
    s as simple case folding does."""
    return contraction(cps, i, stop, fold_cp)


def fold_cp(cp):
    """Simple case folding for the contraction letters: ASCII upper case to
    lower, U+017F (long s) to s."""
    if cp >= 65 and cp <= 90:
        return cp + 32
    elif cp == 383:
        return 115
    else:
        return cp


def prefixable(cp):
    """[^\\r\\n\\p{L}\\p{N}]: any code point but CR, LF, letters and numbers may
    open a letter run (a space, a tab, an emoji)."""
    return cp != 13 and cp != 10 and not Unicode.is_letter(cp) and not Unicode.is_number(cp)


def number_chunk(cps, i, stop):
    """\\p{N}{1,3}: at most three digits per piece."""
    run = run_len(cps, i, stop, Unicode.is_number)
    return 3 if run > 3 else run


def other_crlf(cps, i, stop):
    """`[^\\s\\p{L}\\p{N}]+[\\r\\n]*`: a punctuation run plus its CR/LF tail."""
    run = run_len(cps, i, stop, Unicode.is_other)
    return run + run_len(cps, i + run, stop, is_crlf)


def is_crlf(cp):
    """CR or LF."""
    return cp == 13 or cp == 10


def cl_space_piece(cps, i, stop):
    """`\\s*[\\r\\n]+|\\s+(?!\\S)|\\s+` at whitespace: when the run holds a CR/LF
    the match ends at the last of them, else the trailing-space rule."""
    run = run_len(cps, i, stop, Unicode.is_space)
    k = last_crlf(cps, i, run)
    if k >= 0:
        return k + 1
    elif i + run >= stop:
        return run
    elif run > 1:
        return run - 1
    else:
        return 1


def last_crlf(cps, i, run):
    """The offset of the last CR/LF inside the whitespace run at i, or -1."""
    return fold(range(0, run), -1, lambda best, off: off if is_crlf(Unicode.at(cps, i + off)) else best)
