# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=Kmeans
"""faiss's `Kmeans` clustering with explicit initial centroids.

`Kmeans(d, k, niter, seed)` builds the configuration faiss's constructor
takes; `train(km, x, init_centroids)` runs deterministic Lloyd iterations and
returns the state with `centroids` and the per-iteration objective `obj`, both
exactly as faiss computes them (dyadic cluster means stay dyadic, so the suite
compares bit-for-bit). `assign` is `km.index.search(x, 1)`'s labels: the
nearest centroid id per row. Early stopping matches faiss 1.15.1: the loop
breaks when the objective is unchanged from the previous iteration.
"""

from faiss.Flat import IndexFlatL2, add, search

__all__ = ["Kmeans", "train", "assign"]


class KmeansState:
    """faiss's `Kmeans` object: configuration plus the trained `centroids`
    (k x d) and `obj` (one objective per iteration run)."""
    d: int
    k: int
    niter: int
    seed: int
    centroids: list[list[float]]
    obj: list[float]


def Kmeans(d, k, niter, seed):
    """faiss's `Kmeans(d, k, niter=niter, seed=seed)` constructor."""
    return KmeansState(d=d, k=k, niter=niter, seed=seed, centroids=[], obj=[])


def train(km, x, init_centroids):
    """faiss's `km.train(x, init_centroids=init_centroids)`: Lloyd iterations
    from the explicit centroids. Error when there are fewer points than
    clusters; with exactly k points faiss copies the training set and runs no
    iterations (obj is `[0.0]`)."""
    if len(x) < km.k:
        return Error(message=f"Number of training points ({len(x)}) should be at least as large as number of clusters ({km.k})")
    elif len(x) == km.k:
        return Success(value=KmeansState(d=km.d, k=km.k, niter=km.niter, seed=km.seed, centroids=x, obj=[0.0]))
    else:
        return Success(value=iterate(km, x, init_centroids, 0, 0.0, []))


def iterate(km, x, centroids, i, prev_obj, acc):
    """One Lloyd iteration: assign, accumulate the objective, recompute means,
    and stop when the objective stops changing (faiss's early stop)."""
    if i >= km.niter:
        return KmeansState(d=km.d, k=km.k, niter=km.niter, seed=km.seed, centroids=centroids, obj=acc)
    else:
        res = search(add(IndexFlatL2(km.d), centroids), x, 1)
        assignments = flatten(res.labels)
        obj = fold(range(len(res.distances)), 0.0, lambda total, q: total + unwrap_or(listGet(unwrap_or(listGet(res.distances, q), []), 0), 0.0))
        new_centroids = recompute(x, assignments, km.k, km.d)
        acc2 = listAppend(acc, obj)
        if i > 0 and prev_obj == obj:
            return KmeansState(d=km.d, k=km.k, niter=km.niter, seed=km.seed, centroids=new_centroids, obj=acc2)
        else:
            return iterate(km, x, new_centroids, i + 1, obj, acc2)


def assign(km, x):
    """faiss's `km.index.search(x, 1)`: the nearest centroid id of every row."""
    idx = add(IndexFlatL2(km.d), km.centroids)
    return flatten(search(idx, x, 1).labels)


def flatten(labels):
    """A k=1 label matrix flattened to one id per row."""
    return toList(map(range(len(labels)), lambda q: unwrap_or(listGet(unwrap_or(listGet(labels, q), []), 0), 0)))


def recompute(x, assignments, k, d):
    """faiss's `compute_centroids`: each centroid is the mean of its assigned
    rows; an empty cluster keeps the zero vector (the suite's data never
    empties a cluster, so faiss's largest-cluster split is not exercised)."""
    return toList(map(range(k), lambda c: centroid_of(x, assignments, c, d)))


def centroid_of(x, assignments, c, d):
    """The mean of the rows assigned to cluster `c`, or the zero vector when
    none are."""
    members = filterList(toList(map(range(len(x)), lambda j: j)), lambda j: unwrap_or(listGet(assignments, j), -1) == c)
    n = len(members)
    if n == 0:
        return toList(map(range(d), lambda _t: 0.0))
    else:
        return toList(map(range(d), lambda t: fold(range(n), 0.0, lambda acc, m: acc + unwrap_or(listGet(unwrap_or(listGet(x, unwrap_or(listGet(members, m), 0)), []), t), 0.0)) / toFloat(n)))
