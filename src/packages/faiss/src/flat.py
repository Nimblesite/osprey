# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=Flat
"""The exact-search flat indexes: `IndexFlatL2` and `IndexFlatIP`.

Every operation is a pure function over an immutable `IndexFlat` record, so a
mutating faiss call like `index.add(xb)` becomes `add(index, xb)` returning the
grown index (see README). `search` returns faiss's exact distances and labels,
including the float32 sentinels and -1 labels faiss pads with when k exceeds
`ntotal`; `range_search` returns `lims`/`distances`/`labels` in faiss's scan
order (ascending label); `remove_ids` keeps labels compacted exactly as faiss's
`IndexFlatCodes::remove_ids` does. Distances are sums of products of dyadic
inputs, so binary64 matches faiss's float32 bit for bit.
"""

from faiss.Metrics import l2sqr, inner_product, topk
from faiss.Selectors import selected

__all__ = ["IndexFlat", "IndexFlatL2", "IndexFlatIP", "SearchResult", "RangeSearchResult", "add", "ntotal", "search", "range_search", "reconstruct", "reconstruct_n", "remove_ids", "reset", "search_rows"]


class IndexFlat:
    """An exact-search index over stored vectors: `metric` is "L2" (squared L2)
    or "IP" (inner product), `d` the dimension, `vectors` the stored rows."""
    metric: str
    d: int
    vectors: list[list[float]]


class SearchResult:
    """faiss's `search` output: `distances[q]` and `labels[q]` hold query q's k
    results, distances first by faiss's metric then sentinel-padded."""
    distances: list[list[float]]
    labels: list[list[int]]


class RangeSearchResult:
    """faiss's `range_search` output: `lims` has nq + 1 entries delimiting each
    query's slice of `distances`/`labels` (both in ascending-label order)."""
    lims: list[int]
    distances: list[float]
    labels: list[int]


class RangeRow:
    """One query's range-search matches, in ascending-label order."""
    distances: list[float]
    labels: list[int]


def IndexFlatL2(d):
    """faiss's `IndexFlatL2(d)`: exact search by squared L2 distance."""
    return IndexFlat(metric="L2", d=d, vectors=[])


def IndexFlatIP(d):
    """faiss's `IndexFlatIP(d)`: exact search by inner product."""
    return IndexFlat(metric="IP", d=d, vectors=[])


def ntotal(index):
    """faiss's `index.ntotal`: the number of stored vectors."""
    return len(index.vectors)


def add(index, xb):
    """faiss's `index.add(xb)`: the index with `xb`'s rows appended."""
    return IndexFlat(metric=index.metric, d=index.d, vectors=listConcat(index.vectors, xb))


def distance(metric, a, b):
    return l2sqr(a, b) if metric == "L2" else inner_product(a, b)


def search_row(index, q, k):
    """One query's k results: faiss's heap selection over the stored rows."""
    values = toList(map(range(len(index.vectors)), lambda j: distance(index.metric, q, unwrap_or(listGet(index.vectors, j), []))))
    ids = toList(map(range(len(index.vectors)), lambda j: j))
    return topk(k, index.metric, values, ids)


def search(index, xq, k):
    """faiss's `index.search(xq, k)`: distances and labels for every query row."""
    rows = toList(map(range(len(xq)), lambda q: search_row(index, unwrap_or(listGet(xq, q), []), k)))
    return SearchResult(distances=mapList(rows, lambda r: r.vals), labels=mapList(rows, lambda r: r.ids))


def within(metric, dist, radius):
    """faiss's range-search membership: L2 keeps dist < radius, IP keeps
    inner product > radius (both strict, as `range_search_L2sqr` and
    `range_search_inner_product` compare)."""
    return dist < radius if metric == "L2" else dist > radius


def range_row(index, q, radius):
    """One query's range-search matches: every row within `radius`, ascending
    label (faiss's scan order)."""
    ds = toList(map(range(len(index.vectors)), lambda j: distance(index.metric, q, unwrap_or(listGet(index.vectors, j), []))))
    kept = filterList(toList(map(range(len(index.vectors)), lambda j: j)), lambda j: within(index.metric, unwrap_or(listGet(ds, j), 0.0), radius))
    return RangeRow(distances=mapList(kept, lambda j: unwrap_or(listGet(ds, j), 0.0)), labels=kept)


def prefix_sums(lens):
    """`[0, l0, l0+l1, ...]`: the cumulative lengths faiss's `lims` holds."""
    return toList(map(range(len(lens) + 1), lambda i: fold(range(i), 0, lambda acc, j: acc + unwrap_or(listGet(lens, j), 0))))


def range_search(index, xq, radius):
    """faiss's `index.range_search(xq, radius)`: lims, distances and labels."""
    rows = toList(map(range(len(xq)), lambda q: range_row(index, unwrap_or(listGet(xq, q), []), radius)))
    distances = fold(range(len(rows)), [], lambda acc, q: listConcat(acc, unwrap_or(listGet(rows, q), RangeRow(distances=[], labels=[])).distances))
    labels = fold(range(len(rows)), [], lambda acc, q: listConcat(acc, unwrap_or(listGet(rows, q), RangeRow(distances=[], labels=[])).labels))
    return RangeSearchResult(lims=prefix_sums(mapList(rows, lambda r: len(r.distances))), distances=distances, labels=labels)


def reconstruct(index, i):
    """faiss's `index.reconstruct(i)`: row i, or Error when i is out of range."""
    if i < 0 or i >= len(index.vectors):
        return Error(message=f"IndexFlat reconstruct: index {i} out of range (ntotal={len(index.vectors)})")
    else:
        return Success(value=unwrap_or(listGet(index.vectors, i), []))


def reconstruct_n(index, i0, ni0):
    """faiss's `index.reconstruct_n(i0, ni0)`: rows i0 .. i0+ni0-1, or Error."""
    if i0 < 0 or i0 + ni0 > len(index.vectors):
        return Error(message=f"IndexFlat reconstruct_n: range [{i0}, {i0 + ni0}) out of range (ntotal={len(index.vectors)})")
    else:
        return Success(value=toList(map(range(ni0), lambda i: unwrap_or(listGet(index.vectors, i0 + i), []))))


def remove_ids(index, selector):
    """faiss's `index.remove_ids(selector)`: the index without the selected rows,
    remaining labels compacted to 0..ntotal-1 in order (nremoved is the old
    ntotal minus the new one)."""
    kept = filterList(toList(map(range(len(index.vectors)), lambda j: j)), lambda j: not selected(selector, j))
    return IndexFlat(metric=index.metric, d=index.d, vectors=mapList(kept, lambda j: unwrap_or(listGet(index.vectors, j), [])))


def reset(index):
    """faiss's `index.reset()`: the empty index of the same metric and dimension."""
    return IndexFlat(metric=index.metric, d=index.d, vectors=[])


def search_rows(metric, rows, xq, k):
    """Search `xq` against already-materialized `rows` with `metric`: the shared
    step behind the quantized indexes, which reconstruct their codes and then
    search them exactly as flat vectors."""
    d = len(unwrap_or(listGet(xq, 0), []))
    idx = add(IndexFlatL2(d) if metric == "L2" else IndexFlatIP(d), rows)
    return search(idx, xq, k)
