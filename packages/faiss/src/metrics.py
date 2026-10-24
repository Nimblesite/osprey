# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=Metrics
"""Distances and the exact top-k selection faiss uses for flat search.

`l2sqr` and `inner_product` are the two flat metrics (squared L2 distance and
dot product). `topk` replicates faiss's binary heap exactly: candidates are
fed in scan order, a size-k max-heap keeps the k smallest distances for L2
and a min-heap keeps the k largest inner products for IP, and the heap is
reordered into faiss's output order (ascending distance / descending inner
product, with faiss's tie-breaking). The heap matters because faiss's tie
order among equal keys is not a plain sort of ids. The padded result fills
unused slots with faiss's float32 sentinels (3.4028234663852886e+38 for L2,
its negation for IP) and label -1, exactly as `IndexFlat::search` does when
k exceeds the number of stored vectors. Implements [PORT-TEST-ORACLE]'s exact
dyadic-input contract: every distance here is a sum of products of dyadic
rationals, so binary64 matches faiss's float32 bit for bit.
"""

__all__ = ["Topk", "l2sqr", "inner_product", "topk"]


class Topk:
    """One query's k selected neighbours: `vals` are distances (squared L2, or
    inner products for IP) and `ids` the matching labels, both length k, in
    faiss's output order with sentinels and -1 where the index held fewer than
    k vectors."""
    vals: list[float]
    ids: list[int]


def l2sqr(a, b):
    """Squared L2 distance between two equal-length vectors: sum((a_i - b_i)^2)."""
    return fold(range(len(a)), 0.0, lambda acc, i: acc + sq(unwrap_or(listGet(a, i), 0.0) - unwrap_or(listGet(b, i), 0.0)))


def inner_product(a, b):
    """Dot product between two equal-length vectors: sum(a_i * b_i)."""
    return fold(range(len(a)), 0.0, lambda acc, i: acc + unwrap_or(listGet(a, i), 0.0) * unwrap_or(listGet(b, i), 0.0))


def sq(x):
    return x * x


class Heap:
    """One binary heap: parallel `vals` and `ids`, both length k (0-indexed here;
    the heap code below works 1-based like faiss)."""
    vals: list[float]
    ids: list[int]


def flt_max():
    """float32's largest finite value, faiss's L2 padding sentinel."""
    return 3.4028234663852886e+38


def flt_min():
    """The negation of float32's largest value, faiss's IP padding sentinel."""
    return 0.0 - 3.4028234663852886e+38


def neutral(is_max):
    return flt_max() if is_max else flt_min()


def cmp2(is_max, a1, b1, a2, b2):
    """faiss's two-key heap comparison: primary value, secondary id."""
    if is_max:
        return (a1 > b1) or ((a1 == b1) and (a2 > b2))
    else:
        return (a1 < b1) or ((a1 == b1) and (a2 < b2))


def hgetf(xs, i):
    """1-based float read of a heap array."""
    return unwrap_or(listGet(xs, i - 1), 0.0)


def hgeti(xs, i):
    """1-based int read of a heap array."""
    return unwrap_or(listGet(xs, i - 1), -1)


def hsetf(xs, i, v):
    """A copy of `xs` with 1-based position i holding `v`."""
    return toList(map(range(len(xs)), lambda j: v if j + 1 == i else unwrap_or(listGet(xs, j), 0.0)))


def hseti(xs, i, v):
    """A copy of `xs` with 1-based position i holding `v`."""
    return toList(map(range(len(xs)), lambda j: v if j + 1 == i else unwrap_or(listGet(xs, j), -1)))


def sift(is_max, size, h, i, val, id):
    """Sift `(val, id)` down from 1-based position `i` in a heap of `size`,
    returning the new heap. Mirrors faiss's `heap_replace_top`/`heap_pop`
    sift-down exactly."""
    i1 = 2 * i
    if i1 > size:
        return Heap(vals=hsetf(h.vals, i, val), ids=hseti(h.ids, i, id))
    else:
        return sift_branch(is_max, size, h, i, val, id, i1, i1 + 1)


def sift_branch(is_max, size, h, i, val, id, i1, i2):
    """Pick which child of 1-based position `i` the sift descends into."""
    v1 = hgetf(h.vals, i1)
    v2 = hgetf(h.vals, i2)
    id1 = hgeti(h.ids, i1)
    id2 = hgeti(h.ids, i2)
    if (i2 == size + 1) or cmp2(is_max, v1, v2, id1, id2):
        return sift_child(is_max, size, h, i, val, id, i1, v1, id1)
    else:
        return sift_child(is_max, size, h, i, val, id, i2, v2, id2)


def sift_child(is_max, size, h, i, val, id, child, cv, cid):
    """Place `(val, id)` at `i` when it beats the chosen child, else descend."""
    if cmp2(is_max, val, cv, id, cid):
        return Heap(vals=hsetf(h.vals, i, val), ids=hseti(h.ids, i, id))
    else:
        return sift(is_max, size, Heap(vals=hsetf(h.vals, i, cv), ids=hseti(h.ids, i, cid)), child, val, id)


def better(is_max, top, val):
    """Whether `val` beats the current heap top: smaller for L2, larger for IP."""
    return top > val if is_max else top < val


def pop_top(is_max, size, h):
    """Pop the top of a size-`size` heap: the last element replaces the root and
    sifts down (faiss's `heap_pop`)."""
    return sift(is_max, size, h, 1, hgetf(h.vals, size), hgeti(h.ids, size))


def reorder_loop(is_max, k, size, h, acc_v, acc_i):
    """Pop the whole heap; `acc_v`/`acc_i` collect real (non--1) elements in pop
    order. Returns the final Topk once `size` reaches 0."""
    if size == 0:
        rev_v = listReverse(acc_v)
        rev_i = listReverse(acc_i)
        pad = k - len(rev_v)
        return Topk(vals=listConcat(rev_v, toList(map(range(pad), lambda _i: neutral(is_max)))), ids=listConcat(rev_i, toList(map(range(pad), lambda _i: -1))))
    else:
        id = hgeti(h.ids, 1)
        val = hgetf(h.vals, 1)
        h2 = pop_top(is_max, size, h)
        if id == -1:
            return reorder_loop(is_max, k, size - 1, h2, acc_v, acc_i)
        else:
            return reorder_loop(is_max, k, size - 1, h2, listAppend(acc_v, val), listAppend(acc_i, id))


def topk(k, metric, values, ids):
    """Select the k best of the `len(values)` candidates: for metric "L2" the k
    smallest squared distances, for "IP" the k largest inner products, with
    faiss's exact tie ordering. `values` and `ids` are parallel lists in scan
    order. The result always has length k."""
    is_max = metric == "L2"
    h0 = Heap(vals=toList(map(range(k), lambda _i: neutral(is_max))), ids=toList(map(range(k), lambda _i: -1)))
    h1 = fold(range(len(values)), h0, lambda acc, j: fold_repl(is_max, k, acc, unwrap_or(listGet(values, j), 0.0), unwrap_or(listGet(ids, j), -1)))
    return reorder_loop(is_max, k, k, h1, [], [])


def fold_repl(is_max, k, acc, val, id):
    if better(is_max, hgetf(acc.vals, 1), val):
        return sift(is_max, k, acc, 1, val, id)
    else:
        return acc
