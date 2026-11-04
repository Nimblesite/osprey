# osprey: flavor=python
# osprey: namespace=sklearn
# osprey: module=Preprocessing
"""scikit-learn scalers, normalizers and encoders over array-likes (lists).

Each estimator is a record of its hyperparameters; `*_fit` returns a fitted
record holding the learned attributes, and `*_transform` applies it, so
`StandardScaler().fit(X).transform(X)` becomes
`standard_scaler_transform(standard_scaler_fit(scaler, X), X)`. Formulas,
zero-variance handling and quantile interpolation match sklearn 1.9.1.
"""

from sklearn.Stats import argsort_float, index_of, max_float, mean_float, min_float, sum_float, unique_ints

__all__ = [
    "StandardScaler", "StandardScalerFitted", "standard_scaler_fit",
    "standard_scaler_transform", "MinMaxScaler", "MinMaxScalerFitted",
    "minmax_scaler_fit", "minmax_scaler_transform", "RobustScaler",
    "RobustScalerFitted", "robust_scaler_fit", "robust_scaler_transform",
    "normalize", "label_encoder_fit", "label_encoder_transform",
    "one_hot_encoder_fit", "one_hot_encoder_transform", "PolynomialFeatures",
    "polynomial_powers", "polynomial_features_transform",
]


class StandardScaler:
    """`sklearn.preprocessing.StandardScaler` hyperparameters: whether to
    center by the mean and scale by the standard deviation (ddof 0)."""
    with_mean: bool
    with_std: bool


class StandardScalerFitted:
    """A fitted StandardScaler: per-feature mean and scale (1.0 for constant
    features), plus the flags the transform applies."""
    with_mean: bool
    with_std: bool
    mean: list[float]
    scale: list[float]


class MinMaxScaler:
    """`sklearn.preprocessing.MinMaxScaler` hyperparameters: the target feature
    range, as two floats (sklearn's tuple)."""
    feature_min: float
    feature_max: float


class MinMaxScalerFitted:
    """A fitted MinMaxScaler: per-feature data min/max, scale and offset."""
    data_min: list[float]
    data_max: list[float]
    scale: list[float]
    min_: list[float]


class RobustScaler:
    """`sklearn.preprocessing.RobustScaler` hyperparameters: the quantile range
    whose spread is the scale (default 25.0 and 75.0)."""
    quantile_min: float
    quantile_max: float


class RobustScalerFitted:
    """A fitted RobustScaler: per-feature median and inter-quantile scale."""
    center: list[float]
    scale: list[float]


class PolynomialFeatures:
    """`sklearn.preprocessing.PolynomialFeatures` hyperparameters: the degree,
    whether to include the bias column, and interaction-only terms."""
    degree: int
    include_bias: bool
    interaction_only: bool


EPS = 2.220446049250313e-16


def sq(x):
    """`x * x`."""
    return x * x


def col_float(X, j):
    """Column `j` of a float matrix as a flat list."""
    return toList(map(range(len(X)), lambda i: unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0.0)))


def col_int(X, j):
    """Column `j` of an int matrix as a flat list."""
    return toList(map(range(len(X)), lambda i: unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0)))


def col_var(X, j, mean):
    """Population variance (ddof 0) of column `j`."""
    return mean_float(toList(map(range(len(X)), lambda i: sq(unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0.0) - mean))))


def is_constant_feature(var, mean, n):
    """sklearn's constant-feature test on the raw variance."""
    upper = toFloat(n) * EPS * var + sq(toFloat(n) * mean * EPS)
    return var <= upper


def scale_of(var, mean, n):
    """`sqrt(var)`, or 1.0 for a constant feature."""
    return 1.0 if is_constant_feature(var, mean, n) else sqrt(var)


def standard_scaler_fit(scaler, X):
    """Fits a StandardScaler: per-feature mean and scale over `X`."""
    nf = len(unwrap_or(listGet(X, 0), []))
    means = toList(map(range(nf), lambda j: mean_float(col_float(X, j))))
    variances = toList(map(range(nf), lambda j: col_var(X, j, unwrap_or(listGet(means, j), 0.0))))
    scales = toList(map(range(nf), lambda j: scale_of(unwrap_or(listGet(variances, j), 0.0), unwrap_or(listGet(means, j), 0.0), len(X))))
    return StandardScalerFitted(with_mean=scaler.with_mean, with_std=scaler.with_std, mean=means, scale=scales)


def scaled_value(fitted, x, j):
    """One (optionally) centered and scaled element."""
    centered = x - unwrap_or(listGet(fitted.mean, j), 0.0) if fitted.with_mean else x
    return centered / unwrap_or(listGet(fitted.scale, j), 1.0) if fitted.with_std else centered


def standard_scaler_transform(fitted, X):
    """Applies a fitted StandardScaler to `X`."""
    nf = len(fitted.mean)
    return toList(map(range(len(X)), lambda i: toList(map(range(nf), lambda j: scaled_value(fitted, unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0.0), j)))))


def handle_zero_range(dr):
    """sklearn's `_handle_zeros_in_scale`: a (near) zero range becomes 1.0."""
    return 1.0 if dr < 10.0 * EPS else dr


def minmax_scaler_fit(scaler, X):
    """Fits a MinMaxScaler: per-feature data min/max, scale and offset."""
    nf = len(unwrap_or(listGet(X, 0), []))
    data_min = toList(map(range(nf), lambda j: min_float(col_float(X, j))))
    data_max = toList(map(range(nf), lambda j: max_float(col_float(X, j))))
    feature_range = scaler.feature_max - scaler.feature_min
    scale = toList(map(range(nf), lambda j: feature_range / handle_zero_range(unwrap_or(listGet(data_max, j), 0.0) - unwrap_or(listGet(data_min, j), 0.0))))
    min_ = toList(map(range(nf), lambda j: scaler.feature_min - unwrap_or(listGet(data_min, j), 0.0) * unwrap_or(listGet(scale, j), 1.0)))
    return MinMaxScalerFitted(data_min=data_min, data_max=data_max, scale=scale, min_=min_)


def minmax_scaler_transform(fitted, X):
    """Applies a fitted MinMaxScaler: `X * scale + min_`."""
    nf = len(fitted.scale)
    return toList(map(range(len(X)), lambda i: toList(map(range(nf), lambda j: unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0.0) * unwrap_or(listGet(fitted.scale, j), 1.0) + unwrap_or(listGet(fitted.min_, j), 0.0)))))


def sorted_float(xs):
    """A float list sorted ascending."""
    idx = argsort_float(xs)
    return toList(map(range(len(idx)), lambda i: unwrap_or(listGet(xs, unwrap_or(listGet(idx, i), 0)), 0.0)))


def trunc_int(x):
    """Truncation toward zero of a non-negative float via its repr."""
    parts = unwrap_or(split(toString(x), "."), [toString(x), ""])
    return unwrap_or(parseInt(unwrap_or(listGet(parts, 0), "0")), 0)


def percentile_float(xs, q):
    """numpy's `percentile` with linear interpolation: `q` in [0, 100]."""
    s = sorted_float(xs)
    n = len(s)
    lower = intDiv(trunc_int(q) * (n - 1), 100)
    upper = lower + 1 if lower < n - 1 else lower
    frac = q / 100.0 * toFloat(n - 1) - toFloat(lower)
    lo = unwrap_or(listGet(s, lower), 0.0)
    hi = unwrap_or(listGet(s, upper), 0.0)
    return lo * (1.0 - frac) + hi * frac


def robust_scaler_fit(scaler, X):
    """Fits a RobustScaler: per-feature median and inter-quantile scale."""
    nf = len(unwrap_or(listGet(X, 0), []))
    center = toList(map(range(nf), lambda j: percentile_float(col_float(X, j), 50.0)))
    scale = toList(map(range(nf), lambda j: handle_zero_range(percentile_float(col_float(X, j), scaler.quantile_max) - percentile_float(col_float(X, j), scaler.quantile_min))))
    return RobustScalerFitted(center=center, scale=scale)


def robust_scaler_transform(fitted, X):
    """Applies a fitted RobustScaler: `(X - center) / scale`."""
    nf = len(fitted.center)
    return toList(map(range(len(X)), lambda i: toList(map(range(nf), lambda j: (unwrap_or(listGet(unwrap_or(listGet(X, i), []), j), 0.0) - unwrap_or(listGet(fitted.center, j), 0.0)) / unwrap_or(listGet(fitted.scale, j), 1.0)))))


def row_norm(row, norm):
    """The l1, l2 or max norm of a float row."""
    if norm == "l1":
        return sum_float(toList(map(range(len(row)), lambda i: abs(unwrap_or(listGet(row, i), 0.0)))))
    elif norm == "max":
        return max_float(toList(map(range(len(row)), lambda i: abs(unwrap_or(listGet(row, i), 0.0)))))
    else:
        return sqrt(sum_float(toList(map(range(len(row)), lambda i: sq(unwrap_or(listGet(row, i), 0.0))))))


def normalize(X, norm):
    """`sklearn.preprocessing.normalize` with norm in l1/l2/max: each row
    divided by its norm, a zero norm kept as 1.0."""
    rows = toList(map(range(len(X)), lambda i: unwrap_or(listGet(X, i), [])))
    return toList(map(range(len(rows)), lambda i: normalize_row(unwrap_or(listGet(rows, i), []), norm)))


def normalize_row(row, norm):
    """One normalized row."""
    n = handle_zero_range(row_norm(row, norm))
    return toList(map(range(len(row)), lambda i: unwrap_or(listGet(row, i), 0.0) / n))


def label_encoder_fit(y):
    """`LabelEncoder.fit`: the sorted distinct labels."""
    return unique_ints(y)


def label_encoder_transform(classes, y):
    """`LabelEncoder.transform`: each label mapped to its index in `classes`."""
    return toList(map(range(len(y)), lambda i: index_of(classes, unwrap_or(listGet(y, i), 0))))


def one_hot_encoder_fit(X):
    """`OneHotEncoder.fit`: the sorted distinct category list per feature."""
    nf = len(unwrap_or(listGet(X, 0), []))
    return toList(map(range(nf), lambda j: unique_ints(col_int(X, j))))


def one_hot_segment(cats, v):
    """One-hot vector for one feature's value."""
    idx = index_of(cats, v)
    return toList(map(range(len(cats)), lambda k: 1.0 if k == idx else 0.0))


def one_hot_row(categories, row):
    """One-hot encoding of one sample, features concatenated in order."""
    segments = toList(map(range(len(categories)), lambda j: one_hot_segment(unwrap_or(listGet(categories, j), []), unwrap_or(listGet(row, j), 0))))
    return foldList(segments, [], lambda acc, seg: listConcat(acc, seg))


def one_hot_encoder_transform(categories, X):
    """`OneHotEncoder.transform` (dense output)."""
    return toList(map(range(len(X)), lambda i: one_hot_row(categories, unwrap_or(listGet(X, i), []))))


def offset_combo(c, d):
    """Each element of a combination shifted by `d`."""
    return toList(map(range(len(c)), lambda i: unwrap_or(listGet(c, i), 0) + d))


def combos_no_rep(n, k):
    """All `k`-subsets of `0..n-1`, in lexicographic order."""
    if k == 0:
        return [[]]
    elif n < k:
        return []
    else:
        inc = mapList(combos_no_rep(n - 1, k - 1), lambda c: listConcat([0], offset_combo(c, 1)))
        exc = mapList(combos_no_rep(n - 1, k), lambda c: offset_combo(c, 1))
        return listConcat(inc, exc)


def combos_w_rep(n, k):
    """All `k`-multisets of `0..n-1`, in lexicographic order."""
    if k == 0:
        return [[]]
    elif n == 0:
        return []
    else:
        inc = mapList(combos_w_rep(n, k - 1), lambda c: listConcat([0], c))
        exc = mapList(combos_w_rep(n - 1, k), lambda c: offset_combo(c, 1))
        return listConcat(inc, exc)


def degree_combos(pf, nf, d):
    """Combinations of degree `d` (with or without repetition)."""
    return combos_no_rep(nf, d) if pf.interaction_only else combos_w_rep(nf, d)


def collect_combos(pf, nf):
    """All polynomial combinations in sklearn's order, bias first when asked."""
    degrees = toList(map(range(pf.degree), lambda d: d + 1))
    combos = foldList(degrees, [], lambda acc, d: listConcat(acc, degree_combos(pf, nf, d)))
    return listConcat([[]], combos) if pf.include_bias else combos


def bincount(combo, nf):
    """An exponent vector from a combination of feature indices."""
    return toList(map(range(nf), lambda f: foldList(combo, 0, lambda acc, x: acc + 1 if x == f else acc)))


def polynomial_powers(pf, n_features):
    """`PolynomialFeatures.powers_`: one exponent vector per output feature."""
    return mapList(collect_combos(pf, n_features), lambda c: bincount(c, n_features))


def ipow(x, e):
    """`x` raised to a non-negative integer power."""
    return fold(range(e), 1.0, lambda acc, _: acc * x)


def monomial(row, exps):
    """One polynomial term: the product of `row[f] ** exps[f]`."""
    return fold(range(len(exps)), 1.0, lambda acc, f: acc * ipow(unwrap_or(listGet(row, f), 0.0), unwrap_or(listGet(exps, f), 0)))


def polynomial_features_transform(powers, X):
    """`PolynomialFeatures.transform` over the given exponent vectors."""
    return toList(map(range(len(X)), lambda i: toList(map(range(len(powers)), lambda k: monomial(unwrap_or(listGet(X, i), []), unwrap_or(listGet(powers, k), []))))))
