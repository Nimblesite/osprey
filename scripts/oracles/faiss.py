"""CPython oracle for @osprey/faiss: flat indexes, selectors, ID maps, L2 norm.

Computes every case with faiss-cpu 1.15.1 and writes
packages/faiss/tests/expected.py ([PORT-TEST-ORACLE], spec 0043). Run with the
oracle virtualenv: `python scripts/oracles/faiss.py`.

Inputs are dyadic rationals (multiples of 1/16 in [-8, 8], d <= 64, n <= 200)
so every squared L2 distance and inner product is an integer/256 below 2^24,
and faiss's float32 result is bit-identical to the binary64 the port computes.
The one exception is normalize_L2 (a square root), which the suite compares at
rtol 1e-6.
"""
import os
import sys

from expected_module import write

import numpy as np
import faiss

D = {}  # gathered constants, name -> value


def vecs(rows):
    return np.array(rows, dtype=np.float32)


def empty(n, d):
    return np.empty((n, d), dtype=np.float32)


def dyadic(rng, n, d, lo=-8, hi=8):
    """n x d matrix of multiples of 1/16 in [lo, hi]."""
    steps = 16
    lo_i, hi_i = lo * steps, hi * steps
    return (rng.randint(lo_i, hi_i + 1, size=(n, d))).astype(np.float32) / np.float32(steps)


def flat_search(metric, xb, xq, k):
    if metric == "L2":
        idx = faiss.IndexFlatL2(xb.shape[1])
    else:
        idx = faiss.IndexFlatIP(xb.shape[1])
    idx.add(xb)
    dd, ii = idx.search(xq, k)
    return [list(map(float, row)) for row in dd], [list(map(int, row)) for row in ii]


def flat_range(metric, xb, xq, radius):
    if metric == "L2":
        idx = faiss.IndexFlatL2(xb.shape[1])
    else:
        idx = faiss.IndexFlatIP(xb.shape[1])
    idx.add(xb)
    lims, dd, ii = idx.range_search(xq, radius)
    return list(map(int, lims)), list(map(float, dd)), list(map(int, ii))


rng = np.random.RandomState(20241105)

# --- scenario list: (name, d, xb, xq) ---
SCEN = []
SCEN.append(("empty", 2, empty(0, 2), vecs([[0.0, 0.0], [1.0, -1.0]])))
SCEN.append(("single", 2, vecs([[1.0, 2.0]]), vecs([[1.0, 2.0], [0.0, 0.0], [-1.0, -2.0]])))
SCEN.append(("tie3", 2, vecs([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]), vecs([[0.0, 0.0], [1.0, 1.0]])))
SCEN.append(("five", 2, vecs([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [2.0, 2.0], [-1.0, -1.0]]), vecs([[0.0, 0.0], [1.0, 1.0], [10.0, 10.0]])))
SCEN.append(("six", 2, vecs([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [2.0, 2.0], [-1.0, -1.0], [0.5, 0.5]]), vecs([[0.0, 0.0], [1.0, 1.0]])))
SCEN.append(("d1", 1, vecs([[-2.0], [-1.0], [0.0], [1.0], [2.0]]), vecs([[0.0], [1.5], [-2.0]])))
SCEN.append(("dup", 3, vecs([[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [2.0, 0.0, -1.0]]), vecs([[1.0, 1.0, 1.0], [0.0, 0.0, 0.0]])))
SCEN.append(("neg4", 4, vecs([[0.0, 0.0, 0.0, 0.0], [-1.0, -2.0, -3.0, -4.0], [1.0, 2.0, 3.0, 4.0]]), vecs([[0.0, 0.0, 0.0, 0.0], [-1.0, -1.0, -1.0, -1.0]])))
SCEN.append(("d8", 8, dyadic(rng, 30, 8), dyadic(rng, 4, 8)))
SCEN.append(("d16", 16, dyadic(rng, 50, 16), dyadic(rng, 5, 16)))
SCEN.append(("d32", 32, dyadic(rng, 100, 32), dyadic(rng, 3, 32)))
SCEN.append(("d64", 64, dyadic(rng, 200, 64), dyadic(rng, 2, 64)))
SCEN.append(("alleq", 2, vecs([[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.5, 0.5]]), vecs([[0.5, 0.5], [0.0, 0.0]])))
SCEN.append(("ipeq", 2, vecs([[1.0, 0.0], [0.0, 1.0], [0.5, 0.5], [-1.0, 0.0], [0.0, -1.0], [2.0, 0.0]]), vecs([[1.0, 1.0], [0.0, 0.0]])))
SCEN.append(("randtie", 2, dyadic(rng, 40, 2), dyadic(rng, 6, 2)))

KS = [3, 4, 2, 3, 10, 3, 3, 3, 5, 7, 9, 11, 6, 6, 8]
RADII = [1.0, 5.0, 1.0, 1.5, 1.5, 2.0, 3.0, 8.0, 12.0, 20.0, 30.0, 40.0, 0.25, 1.0, 0.5]

XB = [xb for (_, _, xb, _) in SCEN]
XQ = [xq for (_, _, _, xq) in SCEN]
D["XB"] = [r.tolist() for r in XB]
D["XQ"] = [r.tolist() for r in XQ]
D["KS"] = KS
D["RADII"] = RADII

for metric in ("L2", "IP"):
    sd = []
    si = []
    for (_, _, xb, xq), k in zip(SCEN, KS):
        dd, ii = flat_search(metric, xb, xq, k)
        sd.append(dd)
        si.append(ii)
    D[f"{metric}_SD"] = sd
    D[f"{metric}_SI"] = si

    rlims, rd, ri = [], [], []
    for (_, _, xb, xq), radius in zip(SCEN, RADII):
        lims, dd, ii = flat_range(metric, xb, xq, radius)
        rlims.append(lims)
        rd.append(dd)
        ri.append(ii)
    D[f"{metric}_RLIMS"] = rlims
    D[f"{metric}_RD"] = rd
    D[f"{metric}_RI"] = ri

# --- reconstruct cases: (scenario, i0, ni0) ---
RECON = []
RECON.append((4, 0, 5))   # five, all
RECON.append((4, 1, 2))   # five, middle two
RECON.append((4, 0, 0))   # empty
RECON.append((1, 0, 1))   # single, one
RECON.append((9, 7, 12))  # d8, slice
RECON.append((13, 3, 1))  # ipeq, one
D["RECON_IDX"] = [list(t) for t in RECON]
RECON_V = []
for (s, i0, ni0) in RECON:
    idx = faiss.IndexFlatL2(XB[s].shape[1])
    idx.add(vecs(XB[s]))
    RECON_V.append(idx.reconstruct_n(i0, ni0).tolist())
D["RECON_V"] = RECON_V

# --- remove_ids cases: (scenario, kind, arg) ---
REMOVE = []
REMOVE.append((3, "batch", [1, 3]))
REMOVE.append((3, "range", [1, 3]))
REMOVE.append((4, "batch", [5]))
REMOVE.append((4, "range", [0, 2]))
REMOVE.append((9, "batch", [0, 1, 2]))
REMOVE.append((14, "range", [0, 20]))
D["REMOVE_KIND"] = [k for (_, k, _) in REMOVE]
D["REMOVE_ARG"] = [a for (_, _, a) in REMOVE]
D["REMOVE_SCEN"] = [s for (s, _, _) in REMOVE]
REMOVE_N = []
REMOVE_SEARCH_I = []
for (s, kind, arg) in REMOVE:
    idx = faiss.IndexFlatL2(XB[s].shape[1])
    idx.add(vecs(XB[s]))
    sel = faiss.IDSelectorBatch(np.array(arg, dtype=np.int64)) if kind == "batch" else faiss.IDSelectorRange(arg[0], arg[1])
    nrem = int(idx.remove_ids(sel))
    REMOVE_N.append(nrem)
    dd, ii = idx.search(vecs(XQ[s]), 5)
    REMOVE_SEARCH_I.append([list(map(int, row)) for row in ii])
D["REMOVE_N"] = REMOVE_N
D["REMOVE_SEARCH_I"] = REMOVE_SEARCH_I

# --- IDMap cases: add_with_ids + search labels ---
ID_IDS = [[10, 20, 30, 40, 50], [7, -3, 100], [5], []]
ID_SCEN = [3, 2, 1, 0]
D["ID_SCEN"] = ID_SCEN
D["ID_IDS"] = ID_IDS
IDMAP_SEARCH_I = []
IDMAP2_SEARCH_I = []
IDMAP_RECON = []
IDMAP_REMOVE_I = []
for s, ids in zip(ID_SCEN, ID_IDS):
    xb = vecs(XB[s])
    for cls in (faiss.IndexIDMap, faiss.IndexIDMap2):
        idx = cls(faiss.IndexFlatL2(xb.shape[1]))
        idx.add_with_ids(xb, np.array(ids, dtype=np.int64))
        dd, ii = idx.search(vecs(XQ[s]), 5)
        (IDMAP_SEARCH_I if cls is faiss.IndexIDMap else IDMAP2_SEARCH_I).append([list(map(int, row)) for row in ii])
    # reconstruct via IDMap2 by label
    idx2 = faiss.IndexIDMap2(faiss.IndexFlatL2(xb.shape[1]))
    idx2.add_with_ids(xb, np.array(ids, dtype=np.int64))
    recs = []
    for lbl in ids:
        recs.append(idx2.reconstruct(lbl).tolist())
    IDMAP_RECON.append(recs)
    # remove one label via IDMap, then search labels
    idxr = faiss.IndexIDMap(faiss.IndexFlatL2(xb.shape[1]))
    idxr.add_with_ids(xb, np.array(ids, dtype=np.int64))
    if ids:
        idxr.remove_ids(faiss.IDSelectorBatch(np.array([ids[0]], dtype=np.int64)))
    dd, ii = idxr.search(vecs(XQ[s]), 5)
    IDMAP_REMOVE_I.append([list(map(int, row)) for row in ii])
D["IDMAP_SEARCH_I"] = IDMAP_SEARCH_I
D["IDMAP2_SEARCH_I"] = IDMAP2_SEARCH_I
D["IDMAP_RECON"] = IDMAP_RECON
D["IDMAP_REMOVE_I"] = IDMAP_REMOVE_I

# --- normalize_L2 (sqrt: compare at rtol 1e-6) ---
NORM_X = [
    [[3.0, 4.0], [0.0, 0.0], [1.0, 0.0]],
    [[1.0, 1.0, 1.0, 1.0], [-2.0, 2.0, -2.0, 2.0]],
    [[1.0], [-1.0], [8.0]],
    [[0.0, 0.0, 0.0]],
]
D["NORM_X"] = NORM_X
NORM_Y = []
for m in NORM_X:
    y = vecs(m)
    faiss.normalize_L2(y)
    NORM_Y.append(y.tolist())
D["NORM_Y"] = NORM_Y

# --- Kmeans with explicit init: power-of-2 cluster sizes keep means dyadic ---
KM_CASES = [
    (2, 2, 5, 1234, [[0.0, 0.0], [1.0, 0.0], [10.0, 10.0], [11.0, 10.0]], [[0.0, 0.0], [10.0, 10.0]]),
    (2, 2, 5, 1234, [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0], [8.0, 8.0], [8.0, 9.0], [9.0, 8.0], [9.0, 9.0]], [[0.0, 0.0], [8.0, 8.0]]),
    (1, 2, 3, 1234, [[0.0], [2.0]], [[0.0], [2.0]]),
    (1, 1, 5, 1234, [[0.0], [2.0], [4.0], [6.0]], [[0.0]]),
    (3, 4, 8, 1234, [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [10.0, 10.0, 10.0], [11.0, 10.0, 10.0], [-10.0, -10.0, -10.0], [-11.0, -10.0, -10.0], [0.0, 10.0, 0.0], [0.0, 11.0, 0.0]], [[0.0, 0.0, 0.0], [10.0, 10.0, 10.0], [-10.0, -10.0, -10.0], [0.0, 10.0, 0.0]]),
    (2, 2, 4, 7, [[-4.0, 0.0], [-3.0, 0.0], [4.0, 0.0], [5.0, 0.0]], [[-1.0, 0.0], [1.0, 0.0]]),
]
D["KM_D"] = [c[0] for c in KM_CASES]
D["KM_K"] = [c[1] for c in KM_CASES]
D["KM_NITER"] = [c[2] for c in KM_CASES]
D["KM_SEED"] = [c[3] for c in KM_CASES]
D["KM_X"] = [c[4] for c in KM_CASES]
D["KM_INIT"] = [c[5] for c in KM_CASES]
KM_CENTROIDS = []
KM_OBJ = []
KM_ASSIGN = []
for (d, k, niter, seed, x, init) in KM_CASES:
    km = faiss.Kmeans(d, k, niter=niter, seed=seed)
    km.train(vecs(x), init_centroids=vecs(init))
    KM_CENTROIDS.append(km.centroids.tolist())
    KM_OBJ.append(km.obj.tolist())
    KM_ASSIGN.append(km.index.search(vecs(x), 1)[1].ravel().tolist())
D["KM_CENTROIDS"] = KM_CENTROIDS
D["KM_OBJ"] = KM_OBJ
D["KM_ASSIGN"] = KM_ASSIGN

# --- ScalarQuantizer: powers-of-two ranges keep codes exact ---
SQ_CASES = [
    (faiss.ScalarQuantizer.QT_8bit, [[0.0, -4.0, 0.0], [2.0, 4.0, 8.0], [1.0, 0.0, 4.0]]),
    (faiss.ScalarQuantizer.QT_4bit, [[0.0, 0.0, 0.0, -2.0, 0.0], [2.0, 4.0, 8.0, 2.0, 1.0], [1.0, 2.0, 4.0, 0.0, 0.0]]),
    (faiss.ScalarQuantizer.QT_8bit_uniform, [[0.0, 1.0, 2.0, 3.0], [4.0, 3.0, 2.0, 1.0], [2.0, 2.0, 2.0, 2.0]]),
]
SQ_QTYPE = [c[0] for c in SQ_CASES]
SQ_X = [c[1] for c in SQ_CASES]
D["SQ_QTYPE"] = SQ_QTYPE
D["SQ_X"] = SQ_X
SQ_CODES = []
SQ_DECODE = []
for (qtype, x) in SQ_CASES:
    d = len(x[0])
    sq = faiss.ScalarQuantizer(d, qtype)
    sq.train(vecs(x))
    codes = sq.compute_codes(vecs(x))
    SQ_CODES.append([list(map(int, row)) for row in codes])
    SQ_DECODE.append(sq.decode(codes).tolist())
D["SQ_CODES"] = SQ_CODES
D["SQ_DECODE"] = SQ_DECODE

# --- IndexScalarQuantizer search (decode is float32: rtol 1e-6) ---
SQIDX = [
    (faiss.ScalarQuantizer.QT_8bit, "L2", [[0.0, -4.0, 0.0], [2.0, 4.0, 8.0], [1.0, 0.0, 4.0]], [[0.0, -4.0, 0.0], [2.0, 4.0, 8.0]], 3),
    (faiss.ScalarQuantizer.QT_8bit_uniform, "IP", [[0.0, 1.0, 2.0, 3.0], [4.0, 3.0, 2.0, 1.0], [2.0, 2.0, 2.0, 2.0]], [[0.0, 1.0, 2.0, 3.0], [4.0, 3.0, 2.0, 1.0]], 2),
]
D["SQIDX_QTYPE"] = [c[0] for c in SQIDX]
D["SQIDX_METRIC"] = [c[1] for c in SQIDX]
D["SQIDX_X"] = [c[2] for c in SQIDX]
D["SQIDX_Q"] = [c[3] for c in SQIDX]
D["SQIDX_K"] = [c[4] for c in SQIDX]
SQIDX_D = []
SQIDX_I = []
for (qtype, metric, x, q, k) in SQIDX:
    d = len(x[0])
    idx = faiss.IndexScalarQuantizer(d, qtype, faiss.METRIC_L2 if metric == "L2" else faiss.METRIC_INNER_PRODUCT)
    idx.train(vecs(x))
    idx.add(vecs(x))
    dd, ii = idx.search(vecs(q), k)
    SQIDX_D.append([list(map(float, row)) for row in dd])
    SQIDX_I.append([list(map(int, row)) for row in ii])
D["SQIDX_D"] = SQIDX_D
D["SQIDX_I"] = SQIDX_I

# --- ProductQuantizer: dyadic centroids keep codes/sdc/search exact ---
PQ_CASES = [
    (4, 4, 4, 101, [[0.0, 0.0, 0.0, 0.0], [1.0, 2.0, 3.0, 4.0], [-2.0, 1.0, -1.0, 0.5]]),
    (6, 6, 4, 202, [[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], [2.0, -1.0, 0.5, 3.0, -2.0, 1.0]]),
    (4, 2, 8, 303, [[0.0, 0.0, 0.0, 0.0], [1.0, 2.0, 3.0, 4.0]]),
]
PQ_D = [c[0] for c in PQ_CASES]
PQ_M = [c[1] for c in PQ_CASES]
PQ_NBITS = [c[2] for c in PQ_CASES]
PQ_X = [c[4] for c in PQ_CASES]
D["PQ_D"] = PQ_D
D["PQ_M"] = PQ_M
D["PQ_NBITS"] = PQ_NBITS
D["PQ_X"] = PQ_X
PQ_CENTROIDS = []
PQ_CODES = []
PQ_DECODE = []
PQ_SDC = []
PQIDX_D = []
PQIDX_I = []
for (d, M, nbits, seed, x) in PQ_CASES:
    dsub = d // M
    ksub = 1 << nbits
    crng = np.random.RandomState(seed)
    centroids = (crng.randint(-64, 65, size=(M * ksub, dsub))).astype(np.float32) / np.float32(16)
    PQ_CENTROIDS.append(centroids.tolist())
    pq = faiss.ProductQuantizer(d, M, nbits)
    faiss.copy_array_to_vector(centroids.ravel(), pq.centroids)
    codes = pq.compute_codes(vecs(x))
    PQ_CODES.append([list(map(int, row)) for row in codes])
    PQ_DECODE.append(pq.decode(codes).tolist())
    if nbits == 4:
        pq.compute_sdc_table()
        PQ_SDC.append([float(v) for v in faiss.vector_float_to_array(pq.sdc_table)])
    else:
        PQ_SDC.append([])
    for metric in (faiss.METRIC_L2, faiss.METRIC_INNER_PRODUCT):
        idx = faiss.IndexPQ(d, M, nbits, metric)
        faiss.copy_array_to_vector(centroids.ravel(), idx.pq.centroids)
        idx.is_trained = True
        idx.add(vecs(x))
        dd, ii = idx.search(vecs(x), 3)
        PQIDX_D.append([list(map(float, row)) for row in dd])
        PQIDX_I.append([list(map(int, row)) for row in ii])
D["PQ_CENTROIDS"] = PQ_CENTROIDS
D["PQ_CODES"] = PQ_CODES
D["PQ_DECODE"] = PQ_DECODE
D["PQ_SDC"] = PQ_SDC
D["PQIDX_D"] = PQIDX_D
D["PQIDX_I"] = PQIDX_I
# metric per PQ index case: L2, IP for each PQ case, in order
D["PQIDX_METRIC"] = ["L2", "IP"] * len(PQ_CASES)

def main():
    """Compute every case with the real package and write tests/expected.py."""
    write(
        "faiss",
        "faiss-cpu 1.15.1 flat L2/IP search, range search, reconstruct, remove_ids, ID maps and normalize_L2 on dyadic inputs.",
        {"numpy": np.__version__, "faiss": faiss.__version__},
        D,
    )
    print(f"wrote packages/faiss/tests/expected.py with {len(D)} constants")


if __name__ == "__main__":
    main()
