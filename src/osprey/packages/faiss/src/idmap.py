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
from faiss.Metrics import map_indexed
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
    `add_with_ids`.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, ntotal
    from faiss import Flat
    index = IndexIDMap(Flat.IndexFlatL2(2))
    print(f"[{index.kind}] [{index.sub.metric}] [{ntotal(index)}]")
    ```
    ```output
    [idmap] [L2] [0]
    ```
    """
    return IdMap(kind="idmap", sub=index, labels=[])


def IndexIDMap2(index):
    """faiss's `IndexIDMap2(index)`: like `IndexIDMap` but `reconstruct` maps a
    label back to its vector.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap2, ntotal
    from faiss import Flat
    index = IndexIDMap2(Flat.IndexFlatL2(2))
    print(f"[{index.kind}] [{index.sub.metric}] [{ntotal(index)}]")
    ```
    ```output
    [idmap2] [L2] [0]
    ```
    """
    return IdMap(kind="idmap2", sub=index, labels=[])


def ntotal(index):
    """faiss's `index.ntotal`: the number of stored (labelled) vectors.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, add_with_ids, ntotal
    from faiss import Flat
    empty = IndexIDMap(Flat.IndexFlatL2(2))
    loaded = add_with_ids(empty, [[1.0, 0.0], [0.0, 1.0]], [7, 9])
    print(f"[{ntotal(empty)}] [{ntotal(loaded)}]")
    ```
    ```output
    [0] [2]
    ```
    """
    return flat_ntotal(index.sub)


def add_with_ids(index, xb, ids):
    """faiss's `index.add_with_ids(xb, ids)`: append vectors with their labels.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, add_with_ids, ntotal
    from faiss import Flat
    index = IndexIDMap(Flat.IndexFlatL2(2))
    loaded = add_with_ids(index, [[0.0, 0.0], [3.0, 4.0]], [7, 9])
    print(f"{toString(loaded.labels)} [{ntotal(loaded)}] [{ntotal(index)}]")
    ```
    ```output
    [7, 9] [2] [0]
    ```
    """
    return IdMap(kind=index.kind, sub=flat_add(index.sub, xb), labels=listConcat(index.labels, ids))


def label_row(labels, ordinals):
    """Map sub-index labels to their ID-mapped labels; -1 (padding) stays -1."""
    return mapList(ordinals, lambda o: unwrap_or(listGet(labels, o), -1))


def search(index, xq, k):
    """faiss's `index.search(xq, k)`: sub-index search with labels mapped.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, add_with_ids, search
    from faiss import Flat

    def show():
        index = IndexIDMap(Flat.IndexFlatL2(2))
        loaded = add_with_ids(index, [[0.0, 0.0], [3.0, 4.0]], [7, 9])
        result = search(loaded, [[0.0, 0.0]], 3)
        print(f"{toString(result.distances)} {toString(result.labels)}")

    wrappingArithmetic(show)
    ```
    ```output
    [[0.0, 25.0, 3.4028234663852886e+38]] [[7, 9, -1]]
    ```
    """
    res = flat_search(index.sub, xq, k)
    return SearchResult(distances=res.distances, labels=map_indexed(res.labels, lambda q: label_row(index.labels, unwrap_or(listGet(res.labels, q), []))))


def remove_ids(index, selector):
    """faiss's `index.remove_ids(selector)`: drop selected labels and their
    vectors, preserving the remaining labels.

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, add_with_ids, remove_ids
    from faiss import Flat, Selectors
    index = IndexIDMap(Flat.IndexFlatL2(2))
    loaded = add_with_ids(index, [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]], [5, 7, 9])
    kept = remove_ids(loaded, Selectors.IDSelectorBatch([7]))
    print(f"{toString(kept.labels)} {toString(kept.sub.vectors)}")
    ```
    ```output
    [5, 9] [[0.0, 0.0], [2.0, 2.0]]
    ```
    """
    kept = filterList(map_indexed(index.labels, lambda j: j), lambda j: not selected(selector, unwrap_or(listGet(index.labels, j), -1)))
    return IdMap(kind=index.kind, sub=IndexFlat(metric=index.sub.metric, d=index.sub.d, vectors=mapList(kept, lambda j: unwrap_or(listGet(index.sub.vectors, j), []))), labels=mapList(kept, lambda j: unwrap_or(listGet(index.labels, j), -1)))


def reconstruct(index, key):
    """faiss's `index.reconstruct(key)`: for "idmap2", the vector whose label is
    `key`; for "idmap", Error (faiss does not implement reconstruct there).

    # Examples
    ```osprey
    from faiss.IdMap import IndexIDMap, IndexIDMap2, add_with_ids, reconstruct
    from faiss import Flat

    def text(result):
        match result:
            case Success(value=row):
                return toString(row)
            case Error(message=m):
                return m

    def show():
        two = add_with_ids(IndexIDMap2(Flat.IndexFlatL2(2)), [[1.0, 0.0]], [42])
        one = IndexIDMap(Flat.IndexFlatL2(2))
        print(f"{text(reconstruct(two, 42))} {text(reconstruct(two, 43))} {text(reconstruct(one, 42))}")

    show()
    ```
    ```output
    [1.0, 0.0] key 43 not found reconstruct not implemented for this type of index
    ```
    """
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
