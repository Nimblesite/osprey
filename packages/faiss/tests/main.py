# osprey: flavor=python
"""@osprey/faiss's suite: every flat, selector, ID-map and normalize case
against faiss-cpu 1.15.1's values (scripts/oracles/faiss.py). Search distances
and labels compare exactly (dyadic inputs keep float32 == binary64);
normalize_L2 compares at rtol 1e-6 because faiss normalizes with a float32
square root."""
from faiss.Flat import IndexFlat, IndexFlatL2, IndexFlatIP, SearchResult, RangeSearchResult, add, ntotal, search, range_search, reconstruct, reconstruct_n, remove_ids, reset
from faiss.IdMap import IndexIDMap, IndexIDMap2, add_with_ids, search as idmap_search, ntotal as idmap_ntotal, remove_ids as idmap_remove_ids, reconstruct as idmap_reconstruct
from faiss.Kmeans import Kmeans, train, assign
from faiss.Metrics import Topk, l2sqr, inner_product, topk
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


def d_of(xq):
    """The dimension of a scenario, read from its first query row."""
    return len(unwrap_or(listGet(xq, 0), []))


def close_rows(name, got, want, rtol, atol):
    """Compare two float matrices row by row, one assertion per row."""
    return toList(map(range(len(want)), lambda q: Check.close_all(f"{name}.r{q}", unwrap_or(listGet(got, q), []), unwrap_or(listGet(want, q), []), rtol, atol)))


def int_rows(name, got, want):
    """Compare two int matrices row by row, one assertion per row."""
    return toList(map(range(len(want)), lambda q: Check.ints_equal(f"{name}.r{q}", unwrap_or(listGet(got, q), []), unwrap_or(listGet(want, q), []))))


def search_scenario(metric, s):
    """All search assertions for one scenario and metric."""
    xb = unwrap_or(listGet(Expected.XB, s), [])
    xq = unwrap_or(listGet(Expected.XQ, s), [])
    k = unwrap_or(listGet(Expected.KS, s), 1)
    sd = Expected.L2_SD if metric == "L2" else Expected.IP_SD
    si = Expected.L2_SI if metric == "L2" else Expected.IP_SI
    want_d = unwrap_or(listGet(sd, s), [])
    want_i = unwrap_or(listGet(si, s), [])
    idx = add(IndexFlatL2(d_of(xq)) if metric == "L2" else IndexFlatIP(d_of(xq)), xb)
    res = search(idx, xq, k)
    return listConcat(close_rows(f"search.{metric}.{s}.D", res.distances, want_d, RTOL_EXACT, ATOL_EXACT), int_rows(f"search.{metric}.{s}.I", res.labels, want_i))


def search_cases():
    """faiss's flat search for both metrics over every scenario."""
    l2 = fold(range(len(Expected.XB)), [], lambda acc, s: listConcat(acc, search_scenario("L2", s)))
    ip = fold(range(len(Expected.XB)), [], lambda acc, s: listConcat(acc, search_scenario("IP", s)))
    return listConcat(l2, ip)


def range_scenario(metric, s):
    """All range-search assertions for one scenario and metric."""
    xb = unwrap_or(listGet(Expected.XB, s), [])
    xq = unwrap_or(listGet(Expected.XQ, s), [])
    radius = unwrap_or(listGet(Expected.RADII, s), 0.0)
    rl = Expected.L2_RLIMS if metric == "L2" else Expected.IP_RLIMS
    rd = Expected.L2_RD if metric == "L2" else Expected.IP_RD
    ri = Expected.L2_RI if metric == "L2" else Expected.IP_RI
    wl = unwrap_or(listGet(rl, s), [])
    wd = unwrap_or(listGet(rd, s), [])
    wi = unwrap_or(listGet(ri, s), [])
    idx = add(IndexFlatL2(d_of(xq)) if metric == "L2" else IndexFlatIP(d_of(xq)), xb)
    res = range_search(idx, xq, radius)
    return [
        Check.ints_equal(f"range.{metric}.{s}.lims", res.lims, wl),
        Check.close_all(f"range.{metric}.{s}.D", res.distances, wd, RTOL_EXACT, ATOL_EXACT),
        Check.ints_equal(f"range.{metric}.{s}.I", res.labels, wi),
    ]


def range_cases():
    """faiss's flat range_search for both metrics over every scenario."""
    l2 = fold(range(len(Expected.XB)), [], lambda acc, s: listConcat(acc, range_scenario("L2", s)))
    ip = fold(range(len(Expected.XB)), [], lambda acc, s: listConcat(acc, range_scenario("IP", s)))
    return listConcat(l2, ip)


def recon_cases():
    """reconstruct and reconstruct_n over the oracle's slices."""
    outs = fold(range(len(Expected.RECON_IDX)), [], lambda acc, t: listConcat(acc, recon_one(t)))
    flat = IndexFlat(metric="L2", d=2, vectors=[[0.0, 0.0]])
    return listConcat(outs, [Check.truth("recon.out_of_range", not ok_of(reconstruct(flat, 5)), "index 5 out of range"), Check.truth("recon.bad_slice", not ok_of(reconstruct_n(flat, 0, 2)), "slice out of range")])


def recon_one(t):
    """The reconstruct_n assertion for one oracle slice."""
    idx = unwrap_or(listGet(Expected.RECON_IDX, t), [])
    s = unwrap_or(listGet(idx, 0), 0)
    i0 = unwrap_or(listGet(idx, 1), 0)
    ni0 = unwrap_or(listGet(idx, 2), 0)
    want = unwrap_or(listGet(Expected.RECON_V, t), [])
    xb = unwrap_or(listGet(Expected.XB, s), [])
    xq = unwrap_or(listGet(Expected.XQ, s), [])
    built = add(IndexFlatL2(d_of(xq)), xb)
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
    s = unwrap_or(listGet(Expected.REMOVE_SCEN, t), 0)
    kind = unwrap_or(listGet(Expected.REMOVE_KIND, t), "batch")
    arg = unwrap_or(listGet(Expected.REMOVE_ARG, t), [])
    want_n = unwrap_or(listGet(Expected.REMOVE_N, t), 0)
    want_i = unwrap_or(listGet(Expected.REMOVE_SEARCH_I, t), [])
    xb = unwrap_or(listGet(Expected.XB, s), [])
    xq = unwrap_or(listGet(Expected.XQ, s), [])
    sel = IDSelectorBatch(arg) if kind == "batch" else IDSelectorRange(unwrap_or(listGet(arg, 0), 0), unwrap_or(listGet(arg, 1), 0))
    before = ntotal(add(IndexFlatL2(d_of(xq)), xb))
    after_idx = remove_ids(add(IndexFlatL2(d_of(xq)), xb), sel)
    res = search(after_idx, xq, 5)
    return listConcat(
        [Check.equal(f"remove.{t}.n", before - ntotal(after_idx), want_n)],
        int_rows(f"remove.{t}.I", res.labels, want_i))


def remove_cases():
    """remove_ids for IDSelectorBatch and IDSelectorRange."""
    return fold(range(len(Expected.REMOVE_SCEN)), [], lambda acc, t: listConcat(acc, remove_one(t)))


def idmap_one(t):
    """One ID-map case: add_with_ids, search labels, IDMap2 reconstruct, removal."""
    s = unwrap_or(listGet(Expected.ID_SCEN, t), 0)
    ids = unwrap_or(listGet(Expected.ID_IDS, t), [])
    xb = unwrap_or(listGet(Expected.XB, s), [])
    xq = unwrap_or(listGet(Expected.XQ, s), [])
    base = IndexFlatL2(d_of(xq))
    m = add_with_ids(IndexIDMap(base), xb, ids)
    m2 = add_with_ids(IndexIDMap2(base), xb, ids)
    removed = m if len(ids) == 0 else idmap_remove_ids(m, IDSelectorBatch([unwrap_or(listGet(ids, 0), -1)]))
    return listConcat(idmap_searches(t, m, m2, removed, xq), idmap_recons(t, m, m2, ids))


def idmap_searches(t, m, m2, removed, xq):
    """Search and removal label checks for one ID-map case."""
    want_map = unwrap_or(listGet(Expected.IDMAP_SEARCH_I, t), [])
    want_map2 = unwrap_or(listGet(Expected.IDMAP2_SEARCH_I, t), [])
    want_remove = unwrap_or(listGet(Expected.IDMAP_REMOVE_I, t), [])
    return listConcat(listConcat(int_rows(f"idmap.{t}.search", idmap_search(m, xq, 5).labels, want_map), int_rows(f"idmap.{t}.search2", idmap_search(m2, xq, 5).labels, want_map2)), int_rows(f"idmap.{t}.remove", idmap_search(removed, xq, 5).labels, want_remove))


def idmap_recons(t, m, m2, ids):
    """Reconstruct-by-label and ntotal checks for one ID-map case."""
    want_recon = unwrap_or(listGet(Expected.IDMAP_RECON, t), [])
    recon_outs = toList(map(range(len(want_recon)), lambda j: Check.close_all(f"idmap.{t}.recon{j}", value_of(idmap_reconstruct(m2, unwrap_or(listGet(ids, j), -1))), unwrap_or(listGet(want_recon, j), []), RTOL_EXACT, ATOL_EXACT)))
    return listConcat(recon_outs, [Check.equal(f"idmap.{t}.ntotal", idmap_ntotal(m), len(ids)), Check.truth(f"idmap.{t}.no_recon", not ok_of(idmap_reconstruct(m, unwrap_or(listGet(ids, 0), -1))), "IndexIDMap has no reconstruct")])


def idmap_cases():
    """IndexIDMap and IndexIDMap2: add_with_ids, search labels, reconstruct."""
    return fold(range(len(Expected.ID_SCEN)), [], lambda acc, t: listConcat(acc, idmap_one(t)))


def norm_one(t):
    """One normalize_L2 case, at sqrt tolerance."""
    x = unwrap_or(listGet(Expected.NORM_X, t), [])
    want = unwrap_or(listGet(Expected.NORM_Y, t), [])
    return close_rows(f"norm.{t}", normalize_L2(x), want, RTOL_SQRT, ATOL_SQRT)


def norm_cases():
    """normalize_L2 over the oracle's matrices."""
    return fold(range(len(Expected.NORM_X)), [], lambda acc, t: listConcat(acc, norm_one(t)))


def kmeans_one(t):
    """One Kmeans case: centroids, per-iteration objective and assignments."""
    d = unwrap_or(listGet(Expected.KM_D, t), 0)
    k = unwrap_or(listGet(Expected.KM_K, t), 1)
    niter = unwrap_or(listGet(Expected.KM_NITER, t), 1)
    seed = unwrap_or(listGet(Expected.KM_SEED, t), 0)
    x = unwrap_or(listGet(Expected.KM_X, t), [])
    init = unwrap_or(listGet(Expected.KM_INIT, t), [])
    want_c = unwrap_or(listGet(Expected.KM_CENTROIDS, t), [])
    want_o = unwrap_or(listGet(Expected.KM_OBJ, t), [])
    want_a = unwrap_or(listGet(Expected.KM_ASSIGN, t), [])
    match train(Kmeans(d, k, niter, seed), x, init):
        case Success(value=value):
            return listConcat(
                listConcat(
                    close_rows(f"kmeans.{t}.centroids", value.centroids, want_c, RTOL_EXACT, ATOL_EXACT),
                    [Check.close_all(f"kmeans.{t}.obj", value.obj, want_o, RTOL_EXACT, ATOL_EXACT)]),
                [Check.ints_equal(f"kmeans.{t}.assign", assign(value, x), want_a)])
        case Error(message=message):
            return [Check.truth(f"kmeans.{t}.ok", False, message)]


def kmeans_cases():
    """Kmeans over the oracle's explicit-init cases, plus the too-few-points error."""
    outs = fold(range(len(Expected.KM_D)), [], lambda acc, t: listConcat(acc, kmeans_one(t)))
    err = train(Kmeans(2, 3, 5, 0), [[0.0, 0.0], [1.0, 0.0]], [[0.0, 0.0], [1.0, 0.0], [2.0, 2.0]])
    return listConcat(outs, [Check.truth("kmeans.too_few", not ok_of(err), "fewer points than clusters")])


def scalar_one(t):
    """One ScalarQuantizer case: exact codes and rtol-1e-6 reconstruction."""
    qtype = unwrap_or(listGet(Expected.SQ_QTYPE, t), 0)
    x = unwrap_or(listGet(Expected.SQ_X, t), [])
    want_codes = unwrap_or(listGet(Expected.SQ_CODES, t), [])
    want_decode = unwrap_or(listGet(Expected.SQ_DECODE, t), [])
    sq = sq_train(ScalarQuantizer(len(unwrap_or(listGet(x, 0), [])), qtype), x)
    codes = compute_codes(sq, x)
    return listConcat(int_rows(f"sq.{t}.codes", codes, want_codes), close_rows(f"sq.{t}.decode", decode(sq, codes), want_decode, RTOL_SQRT, ATOL_SQRT))


def scalar_cases():
    """ScalarQuantizer train/compute_codes/decode over the oracle's cases."""
    return fold(range(len(Expected.SQ_QTYPE)), [], lambda acc, t: listConcat(acc, scalar_one(t)))


def sqidx_one(t):
    """One IndexScalarQuantizer search case: rtol distances, exact labels."""
    qtype = unwrap_or(listGet(Expected.SQIDX_QTYPE, t), 0)
    metric = unwrap_or(listGet(Expected.SQIDX_METRIC, t), "L2")
    x = unwrap_or(listGet(Expected.SQIDX_X, t), [])
    q = unwrap_or(listGet(Expected.SQIDX_Q, t), [])
    k = unwrap_or(listGet(Expected.SQIDX_K, t), 1)
    want_d = unwrap_or(listGet(Expected.SQIDX_D, t), [])
    want_i = unwrap_or(listGet(Expected.SQIDX_I, t), [])
    d = len(unwrap_or(listGet(x, 0), []))
    sq = sq_train(ScalarQuantizer(d, qtype), x)
    idx = sq_add(IndexScalarQuantizer(d, qtype, metric), sq, x)
    res = sq_search(idx, q, k)
    return listConcat(close_rows(f"sqidx.{t}.D", res.distances, want_d, RTOL_SQRT, ATOL_SQRT), int_rows(f"sqidx.{t}.I", res.labels, want_i))


def sqidx_cases():
    """IndexScalarQuantizer search over the oracle's cases."""
    return fold(range(len(Expected.SQIDX_QTYPE)), [], lambda acc, t: listConcat(acc, sqidx_one(t)))


def pq_one(t):
    """One ProductQuantizer case: exact codes, decode and sdc table."""
    d = unwrap_or(listGet(Expected.PQ_D, t), 0)
    M = unwrap_or(listGet(Expected.PQ_M, t), 1)
    nbits = unwrap_or(listGet(Expected.PQ_NBITS, t), 4)
    x = unwrap_or(listGet(Expected.PQ_X, t), [])
    centroids = unwrap_or(listGet(Expected.PQ_CENTROIDS, t), [])
    want_codes = unwrap_or(listGet(Expected.PQ_CODES, t), [])
    want_decode = unwrap_or(listGet(Expected.PQ_DECODE, t), [])
    want_sdc = unwrap_or(listGet(Expected.PQ_SDC, t), [])
    pq = set_centroids(ProductQuantizer(d, M, nbits), centroids)
    codes = pq_compute_codes(pq, x)
    sdc = sdc_table(pq) if len(want_sdc) > 0 else []
    return listConcat(
        listConcat(int_rows(f"pq.{t}.codes", codes, want_codes), close_rows(f"pq.{t}.decode", pq_decode(pq, codes), want_decode, RTOL_EXACT, ATOL_EXACT)),
        [Check.close_all(f"pq.{t}.sdc", sdc, want_sdc, RTOL_EXACT, ATOL_EXACT)])


def pq_cases():
    """ProductQuantizer train path over the oracle's centroid-installed cases."""
    return fold(range(len(Expected.PQ_D)), [], lambda acc, t: listConcat(acc, pq_one(t)))


def pqidx_one(t):
    """One IndexPQ search case (L2 then IP per PQ case)."""
    c = intDiv(t, 2)
    d = unwrap_or(listGet(Expected.PQ_D, c), 0)
    M = unwrap_or(listGet(Expected.PQ_M, c), 1)
    nbits = unwrap_or(listGet(Expected.PQ_NBITS, c), 4)
    x = unwrap_or(listGet(Expected.PQ_X, c), [])
    metric = unwrap_or(listGet(Expected.PQIDX_METRIC, t), "L2")
    centroids = unwrap_or(listGet(Expected.PQ_CENTROIDS, c), [])
    want_d = unwrap_or(listGet(Expected.PQIDX_D, t), [])
    want_i = unwrap_or(listGet(Expected.PQIDX_I, t), [])
    idx = pq_add(IndexPQ(set_centroids(ProductQuantizer(d, M, nbits), centroids), metric), x)
    res = pq_search(idx, x, 3)
    return listConcat(close_rows(f"pqidx.{t}.D", res.distances, want_d, RTOL_EXACT, ATOL_EXACT), int_rows(f"pqidx.{t}.I", res.labels, want_i))


def pqidx_cases():
    """IndexPQ search over the oracle's L2 and IP cases."""
    return fold(range(len(Expected.PQIDX_METRIC)), [], lambda acc, t: listConcat(acc, pqidx_one(t)))


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
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
