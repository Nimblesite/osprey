# osprey: flavor=python
"""@osprey/faiss's suite: every flat, selector, ID-map and normalize case
against faiss-cpu 1.15.1's values (src/osprey/packages/faiss/oracle/faiss.cpython). Search distances
and labels compare exactly (dyadic inputs keep float32 == binary64);
normalize_L2 compares at rtol 1e-6 because faiss normalizes with a float32
square root."""
from faiss.Flat import IndexFlat, IndexFlatL2, IndexFlatIP, SearchResult, RangeSearchResult, add, ntotal, search, search_rows, range_search, reconstruct, reconstruct_n, remove_ids, reset
from faiss.IdMap import IndexIDMap, IndexIDMap2, add_with_ids, search as idmap_search, ntotal as idmap_ntotal, remove_ids as idmap_remove_ids, reconstruct as idmap_reconstruct
from faiss.Kmeans import Kmeans, train, assign
from faiss.Metrics import Topk, l2sqr, inner_product, topk
from faiss import Metrics
from faiss.ScalarQuant import ScalarQuantizer, QT_8bit, QT_4bit, QT_8bit_uniform, train as sq_train, compute_codes, decode, IndexScalarQuantizer, sq_add, sq_search
from faiss.Normalize import normalize_L2
from faiss.ProductQuant import ProductQuantizer, set_centroids, compute_codes as pq_compute_codes, decode as pq_decode, sdc_table, IndexPQ, pq_add, pq_search
from faiss.Selectors import IDSelector, IDSelectorBatch, IDSelectorRange, selected
from faiss_expected import Expected
from testing import Check

RTOL_EXACT = 0.0
ATOL_EXACT = 0.0
RTOL_SQRT = 0.000001
ATOL_SQRT = 0.000001


class Vectors:
    """One oracle scenario's base and query matrices."""
    base: list[list[float]]
    query: list[list[float]]


def cell(table, i, default):
    """Case `i` of an oracle table, or `default` when the table is shorter."""
    return unwrap_or(listGet(table, i), default)


def vectors(s):
    """The base and query matrices of one oracle scenario."""
    return Vectors(base=cell(Expected.XB, s, []), query=cell(Expected.XQ, s, []))


def d_of(xq):
    """The dimension of a scenario, read from its first query row."""
    return len(cell(xq, 0, []))


def built(metric, base, d):
    """An empty flat index for the metric, loaded with `base`."""
    return add(IndexFlatL2(d) if metric == "L2" else IndexFlatIP(d), base)


def by_metric(metric, l2, ip):
    """The oracle table the metric selects."""
    return l2 if metric == "L2" else ip


def matrix_rows(name, got, want, compare):
    """One `compare` assertion per row of `want`, numbered."""
    return toList(map(range(len(want)), lambda q: compare(f"{name}.r{q}", cell(got, q, []), cell(want, q, []))))


def close_rows(name, got, want, rtol, atol):
    """Compare two float matrices row by row, one assertion per row."""
    return matrix_rows(name, got, want, lambda tag, row, expected: Check.close_all(tag, row, expected, rtol, atol))


def int_rows(name, got, want):
    """Compare two int matrices row by row, one assertion per row."""
    return matrix_rows(name, got, want, lambda tag, row, expected: Check.ints_equal(tag, row, expected))


def result_rows(name, result, table_d, table_i, t, rtol, atol):
    """The distance and label assertions for case `t` of one search result."""
    return listConcat(close_rows(f"{name}.D", result.distances, cell(table_d, t, []), rtol, atol),
                      int_rows(f"{name}.I", result.labels, cell(table_i, t, [])))


def code_rows(name, codes, want_codes, decoded, want_decode, rtol, atol):
    """The exact-code and decoded-value assertions of one quantizer case."""
    return listConcat(int_rows(f"{name}.codes", codes, want_codes),
                      close_rows(f"{name}.decode", decoded, want_decode, rtol, atol))


def both_metrics(one):
    """One scenario's cases under both flat-search metrics, L2 then IP."""
    return listConcat(each(Expected.XB, lambda s: one("L2", s)), each(Expected.XB, lambda s: one("IP", s)))


def each(table, one):
    """Every case of an oracle table, each checked by `one`."""
    return fold(range(len(table)), [], lambda acc, i: listConcat(acc, one(i)))


def search_scenario(metric, s):
    """All search assertions for one scenario and metric."""
    v = vectors(s)
    k = cell(Expected.KS, s, 1)
    sd = by_metric(metric, Expected.L2_SD, Expected.IP_SD)
    si = by_metric(metric, Expected.L2_SI, Expected.IP_SI)
    res = search(built(metric, v.base, d_of(v.query)), v.query, k)
    rows = search_rows(metric, v.base, v.query, k)
    direct = result_rows(f"search.{metric}.{s}", res, sd, si, s, RTOL_EXACT, ATOL_EXACT)
    materialized = result_rows(f"search_rows.{metric}.{s}", rows, sd, si, s, RTOL_EXACT, ATOL_EXACT)
    return listConcat(direct, materialized)


def search_cases():
    """faiss's flat search for both metrics over every scenario."""
    return both_metrics(search_scenario)


def range_scenario(metric, s):
    """All range-search assertions for one scenario and metric."""
    v = vectors(s)
    radius = cell(Expected.RADII, s, 0.0)
    wl = cell(by_metric(metric, Expected.L2_RLIMS, Expected.IP_RLIMS), s, [])
    wd = cell(by_metric(metric, Expected.L2_RD, Expected.IP_RD), s, [])
    wi = cell(by_metric(metric, Expected.L2_RI, Expected.IP_RI), s, [])
    res = range_search(built(metric, v.base, d_of(v.query)), v.query, radius)
    return [
        Check.ints_equal(f"range.{metric}.{s}.lims", res.lims, wl),
        Check.close_all(f"range.{metric}.{s}.D", res.distances, wd, RTOL_EXACT, ATOL_EXACT),
        Check.ints_equal(f"range.{metric}.{s}.I", res.labels, wi),
    ]


def range_cases():
    """faiss's flat range_search for both metrics over every scenario."""
    return both_metrics(range_scenario)


def recon_cases():
    """reconstruct and reconstruct_n over the oracle's slices."""
    outs = each(Expected.RECON_IDX, recon_one)
    flat = IndexFlat(metric="L2", d=2, vectors=[[0.0, 0.0]])
    return listConcat(outs, [Check.truth("recon.out_of_range", not ok_of(reconstruct(flat, 5)), "index 5 out of range"), Check.truth("recon.bad_slice", not ok_of(reconstruct_n(flat, 0, 2)), "slice out of range")])


def recon_one(t):
    """The reconstruct_n assertion for one oracle slice."""
    idx = cell(Expected.RECON_IDX, t, [])
    s = cell(idx, 0, 0)
    i0 = cell(idx, 1, 0)
    ni0 = cell(idx, 2, 0)
    want = cell(Expected.RECON_V, t, [])
    v = vectors(s)
    built = add(IndexFlatL2(d_of(v.query)), v.base)
    got = reconstruct_n(built, i0, ni0)
    return close_rows(f"recon.{t}", value_of(got), want, RTOL_EXACT, ATOL_EXACT)


def ok_of(result):
    match result:
        case Success(value=_value):
            return True
        case Error(message=_message):
            return False


def value_of(result):
    match result:
        case Success(value=value):
            return value
        case Error(message=_message):
            return []


def remove_one(t):
    """One remove_ids case: nremoved via ntotal delta and the surviving labels."""
    s = cell(Expected.REMOVE_SCEN, t, 0)
    kind = cell(Expected.REMOVE_KIND, t, "batch")
    arg = cell(Expected.REMOVE_ARG, t, [])
    want_n = cell(Expected.REMOVE_N, t, 0)
    want_i = cell(Expected.REMOVE_SEARCH_I, t, [])
    v = vectors(s)
    sel = IDSelectorBatch(arg) if kind == "batch" else IDSelectorRange(cell(arg, 0, 0), cell(arg, 1, 0))
    before = ntotal(add(IndexFlatL2(d_of(v.query)), v.base))
    after_idx = remove_ids(add(IndexFlatL2(d_of(v.query)), v.base), sel)
    res = search(after_idx, v.query, 5)
    return listConcat(
        [Check.equal(f"remove.{t}.n", before - ntotal(after_idx), want_n)],
        int_rows(f"remove.{t}.I", res.labels, want_i))


def remove_cases():
    """remove_ids for IDSelectorBatch and IDSelectorRange."""
    return each(Expected.REMOVE_SCEN, remove_one)


def idmap_one(t):
    """One ID-map case: add_with_ids, search labels, IDMap2 reconstruct, removal."""
    s = cell(Expected.ID_SCEN, t, 0)
    ids = cell(Expected.ID_IDS, t, [])
    v = vectors(s)
    base = IndexFlatL2(d_of(v.query))
    m = add_with_ids(IndexIDMap(base), v.base, ids)
    m2 = add_with_ids(IndexIDMap2(base), v.base, ids)
    removed = m if len(ids) == 0 else idmap_remove_ids(m, IDSelectorBatch([cell(ids, 0, -1)]))
    return listConcat(idmap_searches(t, m, m2, removed, v.query), idmap_recons(t, m, m2, ids))


def idmap_searches(t, m, m2, removed, xq):
    """Search and removal label checks for one ID-map case."""
    want_map = cell(Expected.IDMAP_SEARCH_I, t, [])
    want_map2 = cell(Expected.IDMAP2_SEARCH_I, t, [])
    want_remove = cell(Expected.IDMAP_REMOVE_I, t, [])
    return listConcat(listConcat(int_rows(f"idmap.{t}.search", idmap_search(m, xq, 5).labels, want_map), int_rows(f"idmap.{t}.search2", idmap_search(m2, xq, 5).labels, want_map2)), int_rows(f"idmap.{t}.remove", idmap_search(removed, xq, 5).labels, want_remove))


def idmap_recons(t, m, m2, ids):
    """Reconstruct-by-label and ntotal checks for one ID-map case."""
    want_recon = cell(Expected.IDMAP_RECON, t, [])
    recon_outs = toList(map(range(len(want_recon)), lambda j: Check.close_all(f"idmap.{t}.recon{j}", value_of(idmap_reconstruct(m2, cell(ids, j, -1))), cell(want_recon, j, []), RTOL_EXACT, ATOL_EXACT)))
    return listConcat(recon_outs, [Check.equal(f"idmap.{t}.ntotal", idmap_ntotal(m), len(ids)), Check.truth(f"idmap.{t}.no_recon", not ok_of(idmap_reconstruct(m, cell(ids, 0, -1))), "IndexIDMap has no reconstruct")])


def idmap_cases():
    """IndexIDMap and IndexIDMap2: add_with_ids, search labels, reconstruct."""
    return each(Expected.ID_SCEN, idmap_one)


def norm_one(t):
    """One normalize_L2 case, at sqrt tolerance."""
    x = cell(Expected.NORM_X, t, [])
    want = cell(Expected.NORM_Y, t, [])
    return close_rows(f"norm.{t}", normalize_L2(x), want, RTOL_SQRT, ATOL_SQRT)


def norm_cases():
    """normalize_L2 over the oracle's matrices."""
    return each(Expected.NORM_X, norm_one)


def kmeans_one(t):
    """One Kmeans case: centroids, per-iteration objective and assignments."""
    d = cell(Expected.KM_D, t, 0)
    k = cell(Expected.KM_K, t, 1)
    niter = cell(Expected.KM_NITER, t, 1)
    seed = cell(Expected.KM_SEED, t, 0)
    x = cell(Expected.KM_X, t, [])
    init = cell(Expected.KM_INIT, t, [])
    want_c = cell(Expected.KM_CENTROIDS, t, [])
    want_o = cell(Expected.KM_OBJ, t, [])
    want_a = cell(Expected.KM_ASSIGN, t, [])
    match train(Kmeans(d, k, niter, seed), x, init):
        case Success(value=value):
            return listConcat(listConcat(
                    close_rows(f"kmeans.{t}.centroids", value.centroids, want_c, RTOL_EXACT, ATOL_EXACT),
                    [Check.close_all(f"kmeans.{t}.obj", value.obj, want_o, RTOL_EXACT, ATOL_EXACT)]),
                [Check.ints_equal(f"kmeans.{t}.assign", assign(value, x), want_a)])
        case Error(message=message):
            return [Check.truth(f"kmeans.{t}.ok", False, message)]


def kmeans_cases():
    """Kmeans over the oracle's explicit-init cases, plus the too-few-points error."""
    outs = each(Expected.KM_D, kmeans_one)
    err = train(Kmeans(2, 3, 5, 0), [[0.0, 0.0], [1.0, 0.0]], [[0.0, 0.0], [1.0, 0.0], [2.0, 2.0]])
    return listConcat(outs, [Check.truth("kmeans.too_few", not ok_of(err), "fewer points than clusters")])


def scalar_one(t):
    """One ScalarQuantizer case: exact codes and rtol-1e-6 reconstruction."""
    qtype = cell(Expected.SQ_QTYPE, t, 0)
    x = cell(Expected.SQ_X, t, [])
    want_codes = cell(Expected.SQ_CODES, t, [])
    want_decode = cell(Expected.SQ_DECODE, t, [])
    sq = sq_train(ScalarQuantizer(len(cell(x, 0, [])), qtype), x)
    codes = compute_codes(sq, x)
    return code_rows(f"sq.{t}", codes, want_codes, decode(sq, codes), want_decode, RTOL_SQRT, ATOL_SQRT)


def scalar_cases():
    """ScalarQuantizer train/compute_codes/decode over the oracle's cases."""
    return each(Expected.SQ_QTYPE, scalar_one)


def sqidx_one(t):
    """One IndexScalarQuantizer search case: rtol distances, exact labels."""
    qtype = cell(Expected.SQIDX_QTYPE, t, 0)
    metric = cell(Expected.SQIDX_METRIC, t, "L2")
    x = cell(Expected.SQIDX_X, t, [])
    q = cell(Expected.SQIDX_Q, t, [])
    k = cell(Expected.SQIDX_K, t, 1)
    d = len(cell(x, 0, []))
    sq = sq_train(ScalarQuantizer(d, qtype), x)
    idx = sq_add(IndexScalarQuantizer(d, qtype, metric), sq, x)
    return result_rows(f"sqidx.{t}", sq_search(idx, q, k), Expected.SQIDX_D, Expected.SQIDX_I, t, RTOL_SQRT, ATOL_SQRT)


def sqidx_cases():
    """IndexScalarQuantizer search over the oracle's cases."""
    return each(Expected.SQIDX_QTYPE, sqidx_one)


def pq_one(t):
    """One ProductQuantizer case: exact codes, decode and sdc table."""
    d = cell(Expected.PQ_D, t, 0)
    M = cell(Expected.PQ_M, t, 1)
    nbits = cell(Expected.PQ_NBITS, t, 4)
    x = cell(Expected.PQ_X, t, [])
    centroids = cell(Expected.PQ_CENTROIDS, t, [])
    want_codes = cell(Expected.PQ_CODES, t, [])
    want_decode = cell(Expected.PQ_DECODE, t, [])
    want_sdc = cell(Expected.PQ_SDC, t, [])
    pq = set_centroids(ProductQuantizer(d, M, nbits), centroids)
    codes = pq_compute_codes(pq, x)
    sdc = sdc_table(pq) if len(want_sdc) > 0 else []
    return listConcat(code_rows(f"pq.{t}", codes, want_codes, pq_decode(pq, codes), want_decode, RTOL_EXACT, ATOL_EXACT),
                      [Check.close_all(f"pq.{t}.sdc", sdc, want_sdc, RTOL_EXACT, ATOL_EXACT)])


def pq_cases():
    """ProductQuantizer train path over the oracle's centroid-installed cases."""
    return each(Expected.PQ_D, pq_one)


def pqidx_one(t):
    """One IndexPQ search case (L2 then IP per PQ case)."""
    c = intDiv(t, 2)
    d = cell(Expected.PQ_D, c, 0)
    M = cell(Expected.PQ_M, c, 1)
    nbits = cell(Expected.PQ_NBITS, c, 4)
    x = cell(Expected.PQ_X, c, [])
    metric = cell(Expected.PQIDX_METRIC, t, "L2")
    centroids = cell(Expected.PQ_CENTROIDS, c, [])
    idx = pq_add(IndexPQ(set_centroids(ProductQuantizer(d, M, nbits), centroids), metric), x)
    return result_rows(f"pqidx.{t}", pq_search(idx, x, 3), Expected.PQIDX_D, Expected.PQIDX_I, t, RTOL_EXACT, ATOL_EXACT)


def pqidx_cases():
    """IndexPQ search over the oracle's L2 and IP cases."""
    return each(Expected.PQIDX_METRIC, pqidx_one)


def metric_cases():
    """Direct checks of the shared metrics and selectors (the exported helpers)."""
    return [
        Check.close("metrics.l2sqr", l2sqr([0.0, 1.0], [0.0, 3.0]), 4.0, RTOL_EXACT, ATOL_EXACT),
        Check.close("metrics.ip", inner_product([1.0, 2.0], [3.0, 4.0]), 11.0, RTOL_EXACT, ATOL_EXACT),
        Check.ints_equal("metrics.topk.l2", topk(2, "L2", [3.0, 1.0, 2.0], [0, 1, 2]).ids, [1, 2]),
        Check.ints_equal("metrics.topk.ip", topk(2, "IP", [3.0, 1.0, 2.0], [0, 1, 2]).ids, [0, 2]),
        Check.ints_equal("metrics.topk.pad", topk(4, "L2", [1.0], [7]).ids, [7, -1, -1, -1]),
        Check.truth("select.batch", selected(IDSelectorBatch([1, 3]), 3), "3 is selected"),
        Check.truth("select.batch.not", not selected(IDSelectorBatch([1, 3]), 2), "2 is not selected"),
        Check.truth("select.range", selected(IDSelectorRange(1, 4), 3), "3 in [1,4)"),
        Check.truth("select.range.not", not selected(IDSelectorRange(1, 4), 4), "4 not in [1,4)"),
    ]


def record_cases():
    """The exported result and selector records, and the quantizer type constants."""
    return [
        Check.equal("select.record", IDSelector(kind="range", ids=[], imin=2, imax=5).imax, 5),
        Check.ints_equal("indexed.rebuild", Metrics.map_indexed([1.5, 2.5, 3.5], lambda i: i * i), [0, 1, 4]),
        Check.equal("search.result.record", len(SearchResult(distances=[[1.0]], labels=[[2]]).distances), 1),
        Check.ints_equal("range.result.record", RangeSearchResult(lims=[0, 1], distances=[1.0], labels=[3]).labels, [3]),
        Check.close_all("topk.record", Topk(vals=[1.0], ids=[4]).vals, [1.0], RTOL_EXACT, ATOL_EXACT),
        Check.equal("flat.reset", ntotal(reset(add(IndexFlatL2(2), [[1.0, 2.0]]))), 0),
        Check.equal("sq.qt8", QT_8bit, 0),
        Check.equal("sq.qt4", QT_4bit, 1),
        Check.equal("sq.qt8u", QT_8bit_uniform, 2),
    ]


def helper_cases():
    return listConcat(metric_cases(), record_cases())


def all_cases():
    return listConcat(listConcat(listConcat(listConcat(listConcat(listConcat(listConcat(search_cases(), range_cases()), listConcat(recon_cases(), remove_cases())), listConcat(idmap_cases(), norm_cases())), kmeans_cases()), listConcat(scalar_cases(), sqidx_cases())), listConcat(pq_cases(), pqidx_cases())), helper_cases())


def main():
    return Check.run(all_cases)
