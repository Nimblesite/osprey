# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=sklearn
# osprey: module=Stats
"""Small pure list/float helpers shared by the port's modules.

scikit-learn's public API is expressed over array-likes (lists), so every
module needs a handful of one-pass reductions and a stable argsort. These live
here, named `*_float` to keep the imported names unambiguous, and are part of
the port's support surface (not scikit-learn names): they are exported because
`Metrics`, `Preprocessing` and `ModelSelection` import them, and each is tested
by the suite.
"""

__all__ = [
    "sum_float", "mean_float", "min_float", "max_float", "argsort_float",
    "unique_ints", "index_of",
]


def sum_float(xs):
    """The sum of a non-empty float list, accumulated left to right."""
    return foldList(xs, 0.0, lambda acc, x: acc + x)


def mean_float(xs):
    """The arithmetic mean of a non-empty float list."""
    return sum_float(xs) / toFloat(len(xs))


def min_float(xs):
    """The smallest element of a non-empty float list."""
    first = unwrap_or(listGet(xs, 0), 0.0)
    return foldList(xs, first, lambda acc, x: acc if acc <= x else x)


def max_float(xs):
    """The largest element of a non-empty float list."""
    first = unwrap_or(listGet(xs, 0), 0.0)
    return foldList(xs, first, lambda acc, x: acc if acc >= x else x)


def cmp_index(xs, a, b):
    """Compares `xs[a]` with `xs[b]`, breaking ties by index order so the
    argsort below is stable."""
    va = unwrap_or(listGet(xs, a), 0.0)
    vb = unwrap_or(listGet(xs, b), 0.0)
    if va < vb:
        return -1
    elif va > vb:
        return 1
    elif a < b:
        return -1
    else:
        return 1 if a > b else 0


def argsort_float(xs):
    """Indices that sort `xs` ascending, ties broken by original index order
    (a stable sort): `argsort_float([3.0, 1.0, 3.0]) == [1, 0, 2]`."""
    indices = toList(map(range(len(xs)), lambda i: i))
    return listSortBy(indices, lambda a, b: cmp_index(xs, a, b))


def dedup(sorted_xs):
    """A non-decreasing int list with consecutive duplicates removed."""
    return foldList(
        sorted_xs,
        [],
        lambda acc, x: listAppend(acc, x)
        if len(acc) == 0 or unwrap_or(listGet(acc, len(acc) - 1), x) != x
        else acc,
    )


def cmp_int(a, b):
    """Total integer comparison: negative/zero/positive."""
    return -1 if a < b else (1 if a > b else 0)


def unique_ints(xs):
    """The distinct integers of `xs` in ascending order."""
    return dedup(listSortBy(xs, cmp_int))


def index_of(xs, v):
    """The first index of `v` in an int list, or -1 when absent."""
    return fold(range(len(xs)), -1, lambda acc, i: i if unwrap_or(listGet(xs, i), v) == v and acc == -1 else acc)
