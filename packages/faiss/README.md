# @osprey/faiss

A port of [faiss](https://github.com/facebookresearch/faiss) (faiss-cpu 1.15.1) vector search to
the Osprey Python flavor: the exact flat indexes, ID-mapped indexes and selectors, `normalize_L2`,
k-means with explicit initial centroids, and the product and scalar quantizers. Every value the
suite checks was computed by the real faiss (`scripts/oracles/faiss.py`, run as
`python -I scripts/oracles/oracle.py faiss`).

faiss computes in float32 and Osprey in binary64. The oracle feeds dyadic rationals with few bits
(multiples of 1/16, small dimensions), so every squared L2 distance and inner product is exact in
both and search distances and labels compare exactly; `normalize_L2` compares at `rtol 1e-6`
because faiss normalizes with a float32 reciprocal square root.

## API

| Osprey | faiss | Notes |
| --- | --- | --- |
| `Flat.IndexFlatL2(d)`, `Flat.IndexFlatIP(d)`, `Flat.add`, `Flat.ntotal`, `Flat.search(index, xq, k)` | `IndexFlatL2`, `IndexFlatIP`, `.add`, `.ntotal`, `.search` | Exact tie ordering through a port of faiss's result heap; `k > ntotal` pads labels with `-1` |
| `Flat.range_search`, `Flat.reconstruct`, `Flat.reconstruct_n`, `Flat.remove_ids`, `Flat.reset` | `.range_search`, `.reconstruct`, `.reconstruct_n`, `.remove_ids`, `.reset` | `lims`/`distances`/`labels` in faiss's order |
| `Flat.SearchResult`, `Flat.RangeSearchResult` | the `(D, I)` and `(lims, D, I)` tuples | Records |
| `IdMap.IndexIDMap`, `IdMap.IndexIDMap2`, `add_with_ids`, `search`, `remove_ids`, `reconstruct` | `IndexIDMap`, `IndexIDMap2` | Custom labels; `IndexIDMap2` reconstructs by label |
| `Selectors.IDSelectorBatch`, `IDSelectorRange`, `selected` | `IDSelectorBatch`, `IDSelectorRange` | |
| `Normalize.normalize_L2(x)` | `faiss.normalize_L2` | Returns the normalized matrix |
| `Kmeans.Kmeans(d, k, niter)`, `Kmeans.train(kmeans, x, init_centroids)`, `Kmeans.assign` | `faiss.Kmeans(...).train(x, init_centroids=...)`, `.index.search(x, 1)` | Deterministic Lloyd iterations from explicit centroids |
| `ProductQuant.ProductQuantizer(d, M)`, `set_centroids`, `compute_codes`, `decode`, `sdc_table`, `IndexPQ`, `pq_add`, `pq_search` | `ProductQuantizer` (nbits 8), `IndexPQ` | Centroids are set from the oracle, as faiss's `copy_array_to_vector` does |
| `ScalarQuant.ScalarQuantizer(d, qtype)`, `QT_8bit`, `QT_4bit`, `QT_8bit_uniform`, `train`, `compute_codes`, `decode`, `IndexScalarQuantizer`, `sq_add`, `sq_search` | `ScalarQuantizer` with `RS_minmax`, `IndexScalarQuantizer` | |
| `Metrics.l2sqr`, `inner_product`, `topk` | `fvec_L2sqr`, `fvec_inner_product`, the k-selection | |

## Divergences

- `IndexHNSW`, `IndexIVFFlat` and every index with random level or seed state are not ported.
- `QT_fp16` is not ported: it needs float32 bit access, pending the `floatFromBits` builtins.
- k-means training only from explicit initial centroids; faiss's random initialization is not
  reproduced.
- Matrices are `list[list[float]]`; indexes are immutable records, so `add` returns the grown index.

## Status

The suite (`tests/main.py`, 482 oracle assertions) passes on the default and GC backends. Under ARC
it exits with live objects because of a compiler bug in closures that capture records (tracked by
the ARC work in the port programme); the package is not published until that run is leak-free.
