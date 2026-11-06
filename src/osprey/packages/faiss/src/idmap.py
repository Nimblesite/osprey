# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=IdMap
"""faiss's ID-mapped indexes over a flat sub-index.

`IndexIDMap` wraps a sub-index and stores an explicit label per vector;
`search` returns those labels. `IndexIDMap` does not implement `reconstruct`
in faiss (it raises "reconstruct not implemented"), while `IndexIDMap2` maps a
label back to its vector through a reverse map, so `reconstruct(label)` works
and an unknown label is an Error. `add_with_ids` appends vectors with their
labels; `remove_ids` drops the selected labels and their vectors, keeping the
remaining labels unchanged (faiss does not compact ID-mapped labels).
"""

from faiss.Flat import IndexFlat, SearchResult, add as flat_add, ntotal as flat_ntotal, search as flat_search
from faiss.Selectors import selected

__all__ = ["IndexIDMap", "IndexIDMap2", "add_with_ids", "search", "ntotal", "remove_ids", "reconstruct"]


class IdMap:
    """The record spelling of faiss's `IndexIDMap`/`IndexIDMap2`: `kind` is
    "idmap" (no reconstruct) or "idmap2" (reconstruct by label), `sub` the flat
    sub-index, `labels` one label per stored vector in order."""
    kind: str
    sub: IndexFlat
    labels: list[int]


def IndexIDMap(index):
    """faiss's `IndexIDMap(index)`: wrap `index`, labels assigned via
    `add_with_ids`."""
    return IdMap(kind="idmap", sub=index, labels=[])


def IndexIDMap2(index):
    """faiss's `IndexIDMap2(index)`: like `IndexIDMap` but `reconstruct` maps a
    label back to its vector."""
    return IdMap(kind="idmap2", sub=index, labels=[])


def ntotal(index):
    """faiss's `index.ntotal`: the number of stored (labelled) vectors."""
    return flat_ntotal(index.sub)


def add_with_ids(index, xb, ids):
    """faiss's `index.add_with_ids(xb, ids)`: append vectors with their labels."""
    return IdMap(kind=index.kind, sub=flat_add(index.sub, xb), labels=listConcat(index.labels, ids))


def label_row(labels, ordinals):
    """Map sub-index labels to their ID-mapped labels; -1 (padding) stays -1."""
    return mapList(ordinals, lambda o: unwrap_or(listGet(labels, o), -1))


def search(index, xq, k):
    """faiss's `index.search(xq, k)`: sub-index search with labels mapped."""
    res = flat_search(index.sub, xq, k)
    return SearchResult(distances=res.distances, labels=toList(map(range(len(res.labels)), lambda q: label_row(index.labels, unwrap_or(listGet(res.labels, q), [])))))


def remove_ids(index, selector):
    """faiss's `index.remove_ids(selector)`: drop selected labels and their
    vectors, preserving the remaining labels."""
    kept = filterList(toList(map(range(len(index.labels)), lambda j: j)), lambda j: not selected(selector, unwrap_or(listGet(index.labels, j), -1)))
    return IdMap(kind=index.kind, sub=IndexFlat(metric=index.sub.metric, d=index.sub.d, vectors=mapList(kept, lambda j: unwrap_or(listGet(index.sub.vectors, j), []))), labels=mapList(kept, lambda j: unwrap_or(listGet(index.labels, j), -1)))


def reconstruct(index, key):
    """faiss's `index.reconstruct(key)`: for "idmap2", the vector whose label is
    `key`; for "idmap", Error (faiss does not implement reconstruct there)."""
    match index.kind:
        case "idmap2":
            j = label_position(index.labels, key)
            if j < 0:
                return Error(message=f"key {key} not found")
            else:
                return Success(value=unwrap_or(listGet(index.sub.vectors, j), []))
        case _:
            return Error(message="reconstruct not implemented for this type of index")


def label_position(labels, key):
    """The index of `key` in `labels`, or -1 when absent (faiss's rev-map
    lookup)."""
    return fold(range(len(labels)), -1, lambda found, i: found if found >= 0 else (i if unwrap_or(listGet(labels, i), -1) == key else -1))
