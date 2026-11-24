# osprey: flavor=python
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
    "wrapping", "sum_float", "mean_float", "min_float", "max_float", "argsort_float",
    "unique_ints", "index_of", "encode_labels", "cmp_int", "map_indexed",
]


@handler(Arith)
class wrapping:
    """The arithmetic policy the examples run under: an overflow answers the
    two's-complement wrapped value, a division or remainder by zero answers zero.

    # Examples
    ```osprey
    from sklearn import Stats

    def show(top, zero):
        with Stats.wrapping:
            print(f"[{top + 1}] [{3.0 / toFloat(zero)}] [{7 % zero}]")

    show(9223372036854775807, 0)
    ```
    ```output
    [-9223372036854775808] [0.0] [0]
    ```
    """
    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, _l):
        return 0.0

    def remainderByZero(_l):
        return 0


def map_indexed(xs, read):
    """`xs` rebuilt with its indices: element `i` of the result is `read(i)`.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            scaled = Stats.map_indexed([4, 5], lambda i: i * 10)
            lone = Stats.map_indexed([1], lambda i: i)
            print(f"[{toString(scaled)}] [{toString(lone)}]")

    show()
    ```
    ```output
    [[0, 10]] [[0]]
    ```
    """
    return toList(map(range(len(xs)), read))


def sum_float(xs):
    """The sum of a non-empty float list, accumulated left to right.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            total = Stats.sum_float([1.0, 2.5, 3.0])
            lone = Stats.sum_float([0.5])
            print(f"[{total}] [{lone}]")

    show()
    ```
    ```output
    [6.5] [0.5]
    ```
    """
    return foldList(xs, 0.0, lambda acc, x: acc + x)


def mean_float(xs):
    """The arithmetic mean of a non-empty float list.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            even = Stats.mean_float([1.0, 2.0, 3.0])
            lone = Stats.mean_float([4.0])
            print(f"[{even}] [{lone}]")

    show()
    ```
    ```output
    [2.0] [4.0]
    ```
    """
    return sum_float(xs) / toFloat(len(xs))


def min_float(xs):
    """The smallest element of a non-empty float list.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            smallest = Stats.min_float([3.0, 1.0, 2.0])
            lone = Stats.min_float([7.0])
            print(f"[{smallest}] [{lone}]")

    show()
    ```
    ```output
    [1.0] [7.0]
    ```
    """
    first = unwrap_or(listGet(xs, 0), 0.0)
    return foldList(xs, first, lambda acc, x: acc if acc <= x else x)


def max_float(xs):
    """The largest element of a non-empty float list.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            largest = Stats.max_float([3.0, 1.0, 2.0])
            lone = Stats.max_float([7.0])
            print(f"[{largest}] [{lone}]")

    show()
    ```
    ```output
    [3.0] [7.0]
    ```
    """
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
    (a stable sort): `argsort_float([3.0, 1.0, 3.0]) == [1, 0, 2]`.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            tied = Stats.argsort_float([3.0, 1.0, 3.0])
            spread = Stats.argsort_float([3.0, 1.0, 3.0, 0.5])
            print(f"[{toString(tied)}] [{toString(spread)}]")

    show()
    ```
    ```output
    [[1, 0, 2]] [[3, 1, 0, 2]]
    ```
    """
    indices = map_indexed(xs, lambda i: i)
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
    """Total integer comparison: negative/zero/positive.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            print(f"[{Stats.cmp_int(2, 5)}] [{Stats.cmp_int(5, 2)}] [{Stats.cmp_int(3, 3)}]")

    show()
    ```
    ```output
    [-1] [1] [0]
    ```
    """
    return -1 if a < b else (1 if a > b else 0)


def unique_ints(xs):
    """The distinct integers of `xs` in ascending order.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            repeated = Stats.unique_ints([3, 1, 3, 2, 1])
            lone = Stats.unique_ints([5])
            print(f"[{toString(repeated)}] [{toString(lone)}]")

    show()
    ```
    ```output
    [[1, 2, 3]] [[5]]
    ```
    """
    return dedup(listSortBy(xs, cmp_int))


def index_of(xs, v):
    """The first index of `v` in an int list, or -1 when absent.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            found = Stats.index_of([10, 20, 30], 20)
            missing = Stats.index_of([10, 20, 30], 99)
            print(f"[{found}] [{missing}]")

    show()
    ```
    ```output
    [1] [-1]
    ```
    """
    return fold(range(len(xs)), -1, lambda acc, i: i if unwrap_or(listGet(xs, i), v) == v and acc == -1 else acc)


def encode_labels(labels, ys):
    """Each label of `ys` as its index in `labels`: LabelEncoder.transform's
    mapping, which the metrics share.

    # Examples
    ```osprey
    from sklearn import Stats

    def show():
        with Stats.wrapping:
            encoded = Stats.encode_labels([3, 7, 9], [9, 3, 7, 9])
            unseen = Stats.encode_labels([3, 7], [3, 99])
            print(f"[{toString(encoded)}] [{toString(unseen)}]")

    show()
    ```
    ```output
    [[2, 0, 1, 2]] [[0, -1]]
    ```
    """
    return map_indexed(ys, lambda i: index_of(labels, unwrap_or(listGet(ys, i), 0)))
