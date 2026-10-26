# osprey: flavor=python
# osprey: namespace=sklearn
# osprey: module=ModelSelection
"""scikit-learn dataset splitters over array-likes (lists).

`train_test_split` takes an explicit permutation (the shuffled index order)
and a test size; `kfold_splits` and `stratified_kfold_splits` reproduce
`KFold` and `StratifiedKFold` without shuffling, so every split is
deterministic and matches sklearn's index order exactly. Implements
[PORT-TEST-ORACLE].
"""

from sklearn.Stats import index_of, unique_ints

__all__ = [
    "Split", "train_test_split", "kfold_splits", "stratified_kfold_splits",
]


class Split:
    """One train/test split: the indices of each side, in ascending order."""
    train: list[int]
    test: list[int]


def cmp_int(a, b):
    """Total integer comparison: negative/zero/positive."""
    return -1 if a < b else (1 if a > b else 0)


def train_test_split(permutation, test_size):
    """`sklearn.model_selection.train_test_split` with the shuffle replaced by
    an explicit permutation: the last `test_size` indices are the test set."""
    n = len(permutation)
    train = toList(map(range(n - test_size), lambda i: unwrap_or(listGet(permutation, i), 0)))
    test = toList(map(range(test_size), lambda i: unwrap_or(listGet(permutation, n - test_size + i), 0)))
    return Split(train=train, test=test)


def fold_start(base, extra, k):
    """The first index of fold `k`, given the base size and the number of
    larger folds."""
    return fold(range(k), 0, lambda acc, i: acc + base + (1 if i < extra else 0))


def fold_split(n_samples, k, base, extra):
    """The `k`-th KFold split: a contiguous test block, the rest training."""
    start = fold_start(base, extra, k)
    size = base + (1 if k < extra else 0)
    test = toList(map(range(size), lambda i: start + i))
    train = listConcat(toList(map(range(start), lambda i: i)), toList(map(range(n_samples - start - size), lambda i: start + size + i)))
    return Split(train=train, test=test)


def kfold_splits(n_samples, n_splits):
    """`sklearn.model_selection.KFold(n_splits).split` without shuffling: the
    first `n_samples % n_splits` folds hold one extra sample."""
    base = intDiv(n_samples, n_splits)
    extra = n_samples % n_splits
    return toList(map(range(n_splits), lambda k: fold_split(n_samples, k, base, extra)))


def first_pos(y, label):
    """The first index where `y` equals `label`."""
    return fold(range(len(y)), len(y), lambda acc, i: i if unwrap_or(listGet(y, i), label) == label and acc == len(y) else acc)


def rank_of(fps, v):
    """How many entries of `fps` are smaller than `v`."""
    return foldList(fps, 0, lambda acc, x: acc + 1 if x < v else acc)


def appearance_encode(y):
    """Labels encoded by order of first appearance, as StratifiedKFold does."""
    u = unique_ints(y)
    fps = toList(map(range(len(u)), lambda i: first_pos(y, unwrap_or(listGet(u, i), 0))))
    perm = toList(map(range(len(u)), lambda i: rank_of(fps, unwrap_or(listGet(fps, i), 0))))
    return toList(map(range(len(y)), lambda j: unwrap_or(listGet(perm, index_of(u, unwrap_or(listGet(y, j), 0))), 0)))


def count_of(xs, v):
    """Number of elements of an int list equal to `v`."""
    return foldList(xs, 0, lambda acc, x: acc + 1 if x == v else acc)


def count_stride(sorted_encoded, n_splits, i, k):
    """How many of class `k` fall at stride positions `i, i + n_splits, ...`."""
    return fold(range(len(sorted_encoded)), 0, lambda acc, m: acc + 1 if m % n_splits == i and unwrap_or(listGet(sorted_encoded, m), -1) == k else acc)


def allocation(sorted_encoded, n_splits, n_classes):
    """Per-fold per-class counts from sklearn's round-robin over sorted labels."""
    return toList(map(range(n_splits), lambda i: toList(map(range(n_classes), lambda k: count_stride(sorted_encoded, n_splits, i, k)))))


def folds_for_class(col, n_splits):
    """The fold index list for one class: `allocation[i]` copies of fold `i`."""
    return fold(range(n_splits), [], lambda acc, i: listConcat(acc, toList(map(range(unwrap_or(listGet(col, i), 0)), lambda _: i))))


def pos_in_class(y_encoded, j, k):
    """How many samples before `j` belong to class `k`."""
    return fold(range(j), 0, lambda acc, m: acc + 1 if unwrap_or(listGet(y_encoded, m), -1) == k else acc)


def test_fold_at(y_encoded, allocation, n_splits, j):
    """The fold assigned to sample `j`."""
    k = unwrap_or(listGet(y_encoded, j), 0)
    col = toList(map(range(n_splits), lambda i: unwrap_or(listGet(unwrap_or(listGet(allocation, i), []), k), 0)))
    return unwrap_or(listGet(folds_for_class(col, n_splits), pos_in_class(y_encoded, j, k)), 0)


def test_folds_list(y_encoded, allocation, n_splits):
    """Per-sample fold indices."""
    return toList(map(range(len(y_encoded)), lambda j: test_fold_at(y_encoded, allocation, n_splits, j)))


def indices_where(test_folds, i, keep):
    """Sample indices whose fold index equals (or differs from) `i`."""
    return fold(range(len(test_folds)), [], lambda acc, j: listAppend(acc, j) if (unwrap_or(listGet(test_folds, j), -1) == i) == keep else acc)


def stratified_kfold_splits(y, n_splits):
    """`sklearn.model_selection.StratifiedKFold(n_splits).split` without
    shuffling."""
    y_encoded = appearance_encode(y)
    n_classes = len(unique_ints(y))
    sorted_encoded = listSortBy(y_encoded, cmp_int)
    alloc = allocation(sorted_encoded, n_splits, n_classes)
    test_folds = test_folds_list(y_encoded, alloc, n_splits)
    return toList(map(range(n_splits), lambda i: Split(train=indices_where(test_folds, i, False), test=indices_where(test_folds, i, True))))
