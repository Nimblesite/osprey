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
from tokenizers.Unicode import is_numeric, is_punctuation, is_whitespace, is_word_char
from tokenizers.Normalizers import Norm, prepend_cp, replace_space

__all__ = [
    "bert_pretokenize", "whitespace_pretokenize", "whitespace_split",
    "punctuation_pretokenize", "digits_pretokenize", "char_delimiter_split",
    "metaspace_pretokenize", "META_CP",
]

META_CP = 9601
"""The codepoint of `▁` (U+2581), the metaspace marker SentencePiece-style pre-tokenizers insert for spaces."""


def empty_norm():
    return Norm(chars=[], starts=[], ends=[])


def append_norm(cur, cp, s, e):
    return Norm(chars=listAppend(cur.chars, cp), starts=listAppend(cur.starts, s), ends=listAppend(cur.ends, e))


def close(cur, acc):
    return listAppend(acc, cur) if len(cur.chars) > 0 else acc


def split_removed(n, is_delim, i, cur, acc):
    """Split on a delimiter that is dropped."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = unwrap_or(listGet(n.chars, i), 0)
        s = unwrap_or(listGet(n.starts, i), 0)
        e = unwrap_or(listGet(n.ends, i), 0)
        if is_delim(cp):
            return split_removed(n, is_delim, i + 1, empty_norm(), close(cur, acc))
        else:
            return split_removed(n, is_delim, i + 1, append_norm(cur, cp, s, e), acc)


def split_isolated(n, is_delim, i, cur, acc):
    """Split on a delimiter that becomes its own single-char piece."""
    if i >= len(n.chars):
        return close(cur, acc)
    else:
        cp = unwrap_or(listGet(n.chars, i), 0)
        s = unwrap_or(listGet(n.starts, i), 0)
        e = unwrap_or(listGet(n.ends, i), 0)
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
        cp = unwrap_or(listGet(n.chars, i), 0)
        s = unwrap_or(listGet(n.starts, i), 0)
        e = unwrap_or(listGet(n.ends, i), 0)
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
        cp = unwrap_or(listGet(n.chars, i), 0)
        s = unwrap_or(listGet(n.starts, i), 0)
        e = unwrap_or(listGet(n.ends, i), 0)
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
        cp = unwrap_or(listGet(n.chars, i), 0)
        s = unwrap_or(listGet(n.starts, i), 0)
        e = unwrap_or(listGet(n.ends, i), 0)
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
