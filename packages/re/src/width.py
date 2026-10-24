# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Width
"""Fixed-width analysis for lookbehind assertions.

`sub_width` returns the number of code points a sub-pattern always matches, or
-1 when it is variable-width (then the lookbehind is rejected with CPython's
"look-behind requires fixed-width pattern"). It mirrors the parser's grammar
but tracks only width: literals, classes, `.` and category escapes are 1,
anchors and lookahead are 0, exact `{m}` repeats multiply, alternation must
have equal-width branches, and `*` `+` `?` `{m,}` `{m,n}` and backreferences
are variable (a backreference to a fixed-width group is the one CPython
case this scanner rejects; see the README).
"""

from re import Unicode

__all__ = ["sub_width"]


def at(xs, i):
    """Element `i` of an int list, or 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def is_digit(cp):
    """Whether a code point is an ASCII digit."""
    return cp >= 48 and cp <= 57


def sub_width(cps, i, n):
    """[width, next]: the fixed width of the sub-pattern at `i`, or -1."""
    return alt_width(cps, i, n)


def alt_width(cps, i, n):
    """The width of `seq (| seq)*`."""
    first = seq_width(cps, i, n)
    w = unwrap_or(listGet(first, 0), 0)
    j = unwrap_or(listGet(first, 1), n)
    if w < 0 or j >= n or at(cps, j) != 124:
        return first
    else:
        return alt_width_rest(cps, j + 1, n, w)


def alt_width_rest(cps, i, n, want):
    """Check that each `|` branch has width `want`."""
    branch = seq_width(cps, i, n)
    w = unwrap_or(listGet(branch, 0), 0)
    j = unwrap_or(listGet(branch, 1), n)
    if w != want:
        return [-1, j]
    elif j < n and at(cps, j) == 124:
        return alt_width_rest(cps, j + 1, n, want)
    else:
        return [want, j]


def seq_width(cps, i, n):
    """The sum of atom widths until `)` or `|` or the end."""
    if i >= n or at(cps, i) == 41 or at(cps, i) == 124:
        return [0, i]
    else:
        return seq_width_rest(cps, i, n)


def seq_width_rest(cps, i, n):
    """One atom's width plus the rest of the sequence."""
    aw = atom_width(cps, i, n)
    w = unwrap_or(listGet(aw, 0), 0)
    j = unwrap_or(listGet(aw, 1), n)
    if w < 0:
        return [-1, j]
    else:
        rest = seq_width(cps, j, n)
        return [w + unwrap_or(listGet(rest, 0), 0), unwrap_or(listGet(rest, 1), n)]


def atom_width(cps, i, n):
    """The width of one atom (before any quantifier)."""
    c = at(cps, i)
    if c == 40:
        return group_width(cps, i, n)
    elif c == 91:
        return quantified(cps, i, n, class_width(cps, i, n))
    elif c == 92:
        return escape_width(cps, i, n)
    elif c == 94 or c == 36:
        return [0, i + 1]
    elif c == 46:
        return quantified(cps, i, n, [1, i + 1])
    elif c == 42 or c == 43 or c == 63:
        return [1, i + 1]
    else:
        return quantified(cps, i, n, [1, i + 1])


def class_width(cps, i, n):
    """The width of `[...]`: 1, skipping to the closing `]`."""
    return [1, skip_class(cps, i + 1, n)]


def skip_class(cps, i, n):
    """The index just past the closing `]` of a class."""
    if i >= n:
        return n
    elif at(cps, i) == 92:
        return skip_class(cps, i + 2, n)
    elif at(cps, i) == 93:
        return i + 1
    else:
        return skip_class(cps, i + 1, n)


def escape_width(cps, i, n):
    """The width of a backslash escape atom: assertions are 0, the rest 1."""
    c = at(cps, i + 1)
    if c == 65 or c == 90 or c == 98 or c == 66:
        return [0, i + 2]
    elif is_digit(c) and c != 48:
        return [-1, i + 2]
    else:
        return quantified(cps, i, n, [1, i + 2])


def quantified(cps, i, n, base):
    """Apply a quantifier after the atom at `i` to the atom's width."""
    j = unwrap_or(listGet(base, 1), n)
    w = unwrap_or(listGet(base, 0), 0)
    c = at(cps, j)
    if c == 42 or c == 43 or c == 63:
        return [-1, j + 1]
    elif c == 123:
        return braces_width(cps, j, n, w)
    else:
        return [w, j]


def braces_width(cps, j, n, w):
    """The width of `{m,n}` at `j`: fixed only when m == n."""
    lo = read_digits(cps, j + 1, n)
    mn = unwrap_or(listGet(lo, 0), 0)
    k = unwrap_or(listGet(lo, 1), j + 1)
    if at(cps, k) == 125:
        return [mn * w, k + 1]
    elif at(cps, k) == 44:
        hi = read_digits(cps, k + 1, n)
        mx = unwrap_or(listGet(hi, 0), -1)
        k2 = unwrap_or(listGet(hi, 1), k + 1)
        if at(cps, k2) == 125:
            if is_digit(at(cps, k + 1)) and mx == mn:
                return [mn * w, k2 + 1]
            else:
                return [-1, k2 + 1]
        else:
            return [w, j]
    else:
        return [w, j]


def read_digits(cps, i, n):
    """[value, next] of a digit run, with -1 for an empty run."""
    return read_digits_from(cps, i, n, -1, 0)


def read_digits_from(cps, i, n, has, acc):
    """Accumulate digits from `i`."""
    if i >= n or not is_digit(at(cps, i)):
        return [acc, i]
    else:
        return read_digits_from(cps, i + 1, n, 1, acc * 10 + at(cps, i) - 48)


def group_width(cps, i, n):
    """The width of a `(` group, recursing into its content."""
    c = at(cps, i + 1)
    if c == 63:
        return ext_width(cps, i, n)
    else:
        return group_body_width(cps, i, i + 1, n)


def group_body_width(cps, i, content, n):
    """The width of a group's content, then the quantifier if any."""
    inner = sub_width(cps, content, n)
    w = unwrap_or(listGet(inner, 0), 0)
    j = unwrap_or(listGet(inner, 1), n)
    if w < 0 or j >= n:
        return [-1, j]
    else:
        return quantified(cps, i, n, [w, j + 1])


def ext_width(cps, i, n):
    """The width of a `(?...)` group: lookahead/flag groups are 0, the rest
    recurse into the content."""
    c = at(cps, i + 2)
    if c == 61 or c == 33:
        return [0, skip_group(cps, i, n)]
    elif c == 60:
        return [0, skip_group(cps, i, n)]
    elif c == 58 or c == 80:
        return named_width(cps, i, n, c)
    elif c == 35:
        return [0, skip_comment_group(cps, i, n)]
    else:
        return [0, skip_group(cps, i, n)]


def named_width(cps, i, n, c):
    """The width of a non-capturing or named group's content."""
    content = i + 3 if c == 58 else skip_name(cps, i + 3, n)
    inner = sub_width(cps, content, n)
    w = unwrap_or(listGet(inner, 0), 0)
    j = unwrap_or(listGet(inner, 1), n)
    if w < 0 or j >= n:
        return [-1, j]
    else:
        return quantified(cps, i, n, [w, j + 1])


def skip_name(cps, i, n):
    """The index after a named group's `>`."""
    if i >= n or at(cps, i) == 62:
        return i + 1
    else:
        return skip_name(cps, i + 1, n)


def skip_group(cps, i, n):
    """The index just past the `)` closing the group at `i`."""
    if i >= n:
        return n
    elif at(cps, i) == 92:
        return skip_group(cps, i + 2, n)
    elif at(cps, i) == 40:
        return skip_group(cps, skip_group(cps, i + 1, n), n)
    elif at(cps, i) == 41:
        return i + 1
    else:
        return skip_group(cps, i + 1, n)


def skip_comment_group(cps, i, n):
    """The index just past a `(?#...)` comment."""
    if i >= n or at(cps, i) == 41:
        return i + 1
    else:
        return skip_comment_group(cps, i + 1, n)
