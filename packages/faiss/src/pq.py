# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=ProductQuant
"""faiss's `ProductQuantizer` (nbits = 8) and `IndexPQ`.

The vector is split into `M` sub-vectors of `dsub = d / M`; each sub-vector is
assigned to the nearest of `ksub = 2^nbits` centroids, and the vector is coded
by the M chosen centroid ids. `set_centroids` installs the centroids (dyadic in
the suite, from the oracle via `faiss.copy_array_to_vector`), `compute_codes`
and `decode` are then exact, `sdc_table` holds the symmetric centroid-to-
centroid squared-L2 distances, and `pq_search` reconstructs each code and heap-
selects exactly as faiss's distance-table search sums the same terms. nbits=8
stores one byte per sub-quantizer; other bit widths are not ported yet.
"""

from faiss.Flat import search_rows
from faiss.Metrics import l2sqr, inner_product

__all__ = ["ProductQuantizer", "set_centroids", "compute_codes", "decode", "sdc_table", "IndexPQ", "pq_add", "pq_search"]


class ProductQuantizerState:
    """A product quantizer: `d`, `M` sub-quantizers, `nbits` bits each, `dsub`
    and `ksub` derived, and `centroids` as M*ksub sub-vectors of dsub floats."""
    d: int
    M: int
    nbits: int
    dsub: int
    ksub: int
    centroids: list[list[float]]


class IndexPQState:
    """An index over product-quantized codes: the trained `pq`, a `metric`
    ("L2" or "IP") and the stored `codes` (M bytes per vector)."""
    pq: ProductQuantizerState
    metric: str
    codes: list[list[int]]


def pow2(n):
    return fold(range(n), 1, lambda acc, _i: acc * 2)


def ProductQuantizer(d, M, nbits):
    """faiss's `ProductQuantizer(d, M, nbits)` constructor (centroids empty
    until `set_centroids`)."""
    return ProductQuantizerState(d=d, M=M, nbits=nbits, dsub=intDiv(d, M), ksub=pow2(nbits), centroids=[])


def set_centroids(pq, centroids):
    """Install `centroids` (M*ksub sub-vectors) into the quantizer."""
    return ProductQuantizerState(d=pq.d, M=pq.M, nbits=pq.nbits, dsub=pq.dsub, ksub=pq.ksub, centroids=centroids)


def centroid_at(pq, m, k):
    """Centroid k of sub-quantizer m (a dsub-vector)."""
    return unwrap_or(listGet(pq.centroids, m * pq.ksub + k), [])


def sub_of(row, m, dsub):
    """Sub-vector m of `row`."""
    return toList(map(range(dsub), lambda i: unwrap_or(listGet(row, m * dsub + i), 0.0)))


def best_centroid(xsub, pq, m):
    """The nearest centroid id for sub-vector m; ties keep the lowest id."""
    return fold(range(pq.ksub), 0, lambda best, k: k if l2sqr(xsub, centroid_at(pq, m, k)) < l2sqr(xsub, centroid_at(pq, m, best)) else best)


def sub_codes(pq, row):
    """The M chosen centroid ids for one row."""
    return toList(map(range(pq.M), lambda m: best_centroid(sub_of(row, m, pq.dsub), pq, m)))


def compute_codes(pq, x):
    """faiss's `pq.compute_codes(x)`: the packed centroid ids per row
    (one byte per sub-quantizer at nbits = 8, two ids per byte at nbits = 4)."""
    return mapList(x, lambda row: pack_ids(pq, sub_codes(pq, row)))


def decode(pq, codes):
    """faiss's `pq.decode(codes)`: reconstruct each row from its centroid ids."""
    return mapList(codes, lambda code: decode_code(pq, code))


def id_at(ids, j):
    if j >= len(ids):
        return 0
    else:
        return unwrap_or(listGet(ids, j), 0)


def pack_ids(pq, ids):
    """Pack centroid ids into bytes: identity at nbits = 8, nibble pairs at 4."""
    match pq.nbits:
        case 8:
            return ids
        case _:
            return toList(map(range(intDiv(pq.M + 1, 2)), lambda t: id_at(ids, 2 * t) + 16 * id_at(ids, 2 * t + 1)))


def unpack_ids(pq, code):
    """The M centroid ids hidden in `code`."""
    match pq.nbits:
        case 8:
            return code
        case _:
            return toList(map(range(pq.M), lambda m: id_at(code, intDiv(m, 2)) % 16 if m % 2 == 0 else intDiv(id_at(code, intDiv(m, 2)), 16)))


def decode_code(pq, code):
    """Reconstruct one code by concatenating the chosen centroids."""
    return fold(range(pq.M), [], lambda acc, m: listConcat(acc, centroid_at(pq, m, unwrap_or(listGet(unpack_ids(pq, code), m), 0))))


def sdc_table(pq):
    """faiss's `pq.sdc_table` (after `compute_sdc_table`): for each
    sub-quantizer m, the ksub x ksub squared-L2 distances between centroids,
    flattened m-major, row-major."""
    return toList(map(range(pq.M * pq.ksub * pq.ksub), lambda idx: sdc_entry(pq, idx)))


def sdc_entry(pq, idx):
    """One entry of `sdc_table` from its flat index."""
    i = intDiv(idx, pq.ksub) % pq.ksub
    j = idx % pq.ksub
    m = intDiv(idx, pq.ksub * pq.ksub)
    return l2sqr(centroid_at(pq, m, i), centroid_at(pq, m, j))


def IndexPQ(pq, metric):
    """faiss's `IndexPQ(pq, metric)` constructor: an empty index over `pq`."""
    return IndexPQState(pq=pq, metric=metric, codes=[])


def pq_add(index, xb):
    """faiss's `index.add(xb)`: append the codes of `xb`."""
    return IndexPQState(pq=index.pq, metric=index.metric, codes=listConcat(index.codes, compute_codes(index.pq, xb)))


def pq_search(index, xq, k):
    """faiss's `IndexPQ.search`: reconstruct codes and heap-select over the
    metric (the same sum faiss's distance tables form, so it is exact)."""
    rows = mapList(index.codes, lambda code: decode_code(index.pq, code))
    return search_rows(index.metric, rows, xq, k)
