# osprey: flavor=python
# osprey: namespace=sklearn
# osprey: module=Metrics
"""scikit-learn classification and regression metrics over array-likes (lists).

Labels and scores are lists of ints and floats, as scikit-learn accepts; the
port reimplements sklearn 1.9.1's exact formulas and tie-breaking. Floats
compare against the oracle with numpy's `isclose` rule in the suite. `log_loss`
and `top_k_accuracy_score` take a list of per-class probability rows (the
multiclass shape); the binary one-column spelling is expressed as its two-column
equivalent. Implements [PORT-TEST-ORACLE].
"""

from sklearn.Stats import argsort_float, index_of, mean_float, unique_ints

__all__ = [
    "RocCurve", "PrecisionRecallCurve", "accuracy_score", "precision_score",
    "recall_score", "f1_score", "confusion_matrix", "roc_auc_score", "roc_curve",
    "precision_recall_curve", "average_precision_score", "log_loss",
    "mean_squared_error", "mean_absolute_error", "r2_score",
    "explained_variance_score", "matthews_corrcoef", "top_k_accuracy_score",
]


class RocCurve:
    """`sklearn.metrics.roc_curve` result: false positive rates, true positive
    rates and their decreasing thresholds (the first threshold is +inf)."""
    fpr: list[float]
    tpr: list[float]
    thresholds: list[float]


class PrecisionRecallCurve:
    """`sklearn.metrics.precision_recall_curve` result: precision, recall and
    their increasing thresholds (precision and recall carry one extra trailing
    point, 1.0 and 0.0, with no threshold)."""
    precision: list[float]
    recall: list[float]
    thresholds: list[float]


class Points:
    """Raw per-threshold counts from a score sort, before curve assembly."""
    fps: list[float]
    tps: list[float]
    thresholds: list[float]


class ClassStats:
    """Per-label sufficient statistics for precision/recall/f-score."""
    tp: list[int]
    pred: list[int]
    true: list[int]
    precision: list[float]
    recall: list[float]
    f1: list[float]


class PRFResult:
    """An averaged precision/recall/f1 triple."""
    precision: float
    recall: float
    f1: float


def nan():
    """IEEE NaN as `inf - inf`, the flavor's only way to spell it."""
    return exp(1000.0) - exp(1000.0)


def count_of(xs, v):
    """Number of elements of an int list equal to `v`."""
    return foldList(xs, 0, lambda acc, x: acc + 1 if x == v else acc)


def count_both(yt, yp, v):
    """Positions where both `yt` and `yp` equal `v`."""
    return fold(range(len(yt)), 0, lambda acc, i: acc + 1 if unwrap_or(listGet(yt, i), v) == v and unwrap_or(listGet(yp, i), v) == v else acc)


def count_pair(yt, yp, a, b):
    """Positions where `yt` equals `a` and `yp` equals `b`."""
    return fold(range(len(yt)), 0, lambda acc, i: acc + 1 if unwrap_or(listGet(yt, i), a) == a and unwrap_or(listGet(yp, i), b) == b else acc)


def sum_int(xs):
    """Sum of an int list."""
    return foldList(xs, 0, lambda acc, x: acc + x)


def divide_z(num, den, zd):
    """`num / den`, or the zero_division value when `den` is zero."""
    return toFloat(zd) if den == 0.0 else num / den


def weighted_mean(vals, weights):
    """Average of `vals` weighted by an int list."""
    num = fold(range(len(vals)), 0.0, lambda acc, i: acc + unwrap_or(listGet(vals, i), 0.0) * toFloat(unwrap_or(listGet(weights, i), 0)))
    return num / toFloat(sum_int(weights))


def class_stats(yt, yp, labels, zd):
    """Per-label tp, predicted and true counts, then precision/recall/f1."""
    tp = toList(map(range(len(labels)), lambda i: count_both(yt, yp, unwrap_or(listGet(labels, i), 0))))
    pred = toList(map(range(len(labels)), lambda i: count_of(yp, unwrap_or(listGet(labels, i), 0))))
    true = toList(map(range(len(labels)), lambda i: count_of(yt, unwrap_or(listGet(labels, i), 0))))
    precision = toList(map(range(len(labels)), lambda i: divide_z(toFloat(unwrap_or(listGet(tp, i), 0)), toFloat(unwrap_or(listGet(pred, i), 0)), zd)))
    recall = toList(map(range(len(labels)), lambda i: divide_z(toFloat(unwrap_or(listGet(tp, i), 0)), toFloat(unwrap_or(listGet(true, i), 0)), zd)))
    f1 = toList(map(range(len(labels)), lambda i: divide_z(2.0 * toFloat(unwrap_or(listGet(tp, i), 0)), toFloat(unwrap_or(listGet(true, i), 0)) + toFloat(unwrap_or(listGet(pred, i), 0)), zd)))
    return ClassStats(tp=tp, pred=pred, true=true, precision=precision, recall=recall, f1=f1)


def prf_scores(yt, yp, average, zd):
    """Averaged precision, recall and f1 over the chosen labels."""
    labels = [1] if average == "binary" else unique_ints(listConcat(yt, yp))
    s = class_stats(yt, yp, labels, zd)
    if average == "micro":
        p = divide_z(toFloat(sum_int(s.tp)), toFloat(sum_int(s.pred)), zd)
        r = divide_z(toFloat(sum_int(s.tp)), toFloat(sum_int(s.true)), zd)
        f = divide_z(2.0 * toFloat(sum_int(s.tp)), toFloat(sum_int(s.true)) + toFloat(sum_int(s.pred)), zd)
        return PRFResult(precision=p, recall=r, f1=f)
    elif average == "weighted":
        return PRFResult(precision=weighted_mean(s.precision, s.true), recall=weighted_mean(s.recall, s.true), f1=weighted_mean(s.f1, s.true))
    else:
        return PRFResult(precision=mean_float(s.precision), recall=mean_float(s.recall), f1=mean_float(s.f1))


def accuracy_score(y_true, y_pred):
    """`sklearn.metrics.accuracy_score`: fraction of equal labels."""
    n = len(y_true)
    return toFloat(fold(range(n), 0, lambda acc, i: acc + 1 if unwrap_or(listGet(y_true, i), 0) == unwrap_or(listGet(y_pred, i), 0) else acc)) / toFloat(n)


def precision_score(y_true, y_pred, average, zero_division):
    """`sklearn.metrics.precision_score` for average in binary/micro/macro/
    weighted with zero_division 0 or 1."""
    return prf_scores(y_true, y_pred, average, zero_division).precision


def recall_score(y_true, y_pred, average, zero_division):
    """`sklearn.metrics.recall_score` for average in binary/micro/macro/
    weighted with zero_division 0 or 1."""
    return prf_scores(y_true, y_pred, average, zero_division).recall


def f1_score(y_true, y_pred, average, zero_division):
    """`sklearn.metrics.f1_score` for average in binary/micro/macro/weighted
    with zero_division 0 or 1."""
    return prf_scores(y_true, y_pred, average, zero_division).f1


def confusion_matrix(y_true, y_pred, labels):
    """`sklearn.metrics.confusion_matrix` over an explicit, sorted label list:
    entry `[i][j]` counts true `labels[i]` predicted as `labels[j]`."""
    return toList(map(range(len(labels)), lambda i: toList(map(range(len(labels)), lambda j: count_pair(y_true, y_pred, unwrap_or(listGet(labels, i), 0), unwrap_or(listGet(labels, j), 0))))))


def gather_float(xs, indices):
    """`xs` gathered at the given indices, as floats."""
    return toList(map(range(len(indices)), lambda i: unwrap_or(listGet(xs, unwrap_or(listGet(indices, i), 0)), 0.0)))


def negate_scores(ys):
    """`-ys` elementwise, so an ascending sort becomes descending."""
    return toList(map(range(len(ys)), lambda i: 0.0 - unwrap_or(listGet(ys, i), 0.0)))


def threshold_indices(ss):
    """Indices where a descending score list changes, plus the final index."""
    n = len(ss)
    distinct = fold(range(n - 1), [], lambda acc, i: listAppend(acc, i) if unwrap_or(listGet(ss, i), 0.0) != unwrap_or(listGet(ss, i + 1), 0.0) else acc)
    return listAppend(distinct, n - 1)


def sum_first(vals, m):
    """The first `m` elements of a float list summed in order."""
    return fold(range(m), 0.0, lambda acc, i: acc + unwrap_or(listGet(vals, i), 0.0))


def threshold_points(yt, ys):
    """True/false-positive counts and scores at each distinct threshold,
    matching sklearn's stable descending sort and cumulative sums."""
    order = argsort_float(negate_scores(ys))
    st = gather_float(toList(map(range(len(yt)), lambda i: 1.0 if unwrap_or(listGet(yt, i), 0) == 1 else 0.0)), order)
    ss = gather_float(ys, order)
    tids = threshold_indices(ss)
    tps = toList(map(range(len(tids)), lambda k: sum_first(st, unwrap_or(listGet(tids, k), 0) + 1)))
    fps = toList(map(range(len(tids)), lambda k: 1.0 + toFloat(unwrap_or(listGet(tids, k), 0)) - unwrap_or(listGet(tps, k), 0.0)))
    thresholds = gather_float(ss, tids)
    return Points(fps=fps, tps=tps, thresholds=thresholds)


def second_diff(xs, i):
    """Discrete second difference `x[i+1] - 2 x[i] + x[i-1]`: a corner test."""
    return unwrap_or(listGet(xs, i + 1), 0.0) - 2.0 * unwrap_or(listGet(xs, i), 0.0) + unwrap_or(listGet(xs, i - 1), 0.0)


def keep_flags(fps, tps):
    """Which points survive sklearn's `drop_intermediate`: first, last, and any
    corner of the ROC polyline."""
    m = len(fps)
    return toList(map(range(m), lambda i: i == 0 or i == m - 1 or second_diff(fps, i) != 0.0 or second_diff(tps, i) != 0.0))


def filter_floats(xs, flags):
    """Elements of `xs` kept where `flags` is true."""
    return fold(range(len(xs)), [], lambda acc, i: listAppend(acc, unwrap_or(listGet(xs, i), 0.0)) if unwrap_or(listGet(flags, i), False) else acc)


def divide_by_last(xs):
    """`xs / xs[-1]`, for the last element being the total count."""
    last = unwrap_or(listGet(xs, len(xs) - 1), 0.0)
    return toList(map(range(len(xs)), lambda i: unwrap_or(listGet(xs, i), 0.0) / last))


def all_nan(n):
    """`n` NaNs, the roc_curve result when a class is absent."""
    return toList(map(range(n), lambda _: nan()))


def roc_curve(y_true, y_score):
    """`sklearn.metrics.roc_curve` (pos_label=1, drop_intermediate=True)."""
    p = threshold_points(y_true, y_score)
    flags = keep_flags(p.fps, p.tps)
    fps = filter_floats(p.fps, flags)
    tps = filter_floats(p.tps, flags)
    thresholds = filter_floats(p.thresholds, flags)
    fps0 = listConcat([0.0], fps)
    tps0 = listConcat([0.0], tps)
    fpr = all_nan(len(fps0)) if unwrap_or(listGet(fps0, len(fps0) - 1), 0.0) <= 0.0 else divide_by_last(fps0)
    tpr = all_nan(len(tps0)) if unwrap_or(listGet(tps0, len(tps0) - 1), 0.0) <= 0.0 else divide_by_last(tps0)
    return RocCurve(fpr=fpr, tpr=tpr, thresholds=listConcat([exp(1000.0)], thresholds))


def auc_trapezoid(xs, ys):
    """Trapezoidal area under a polyline, as sklearn's `auc`."""
    return fold(range(len(xs) - 1), 0.0, lambda acc, i: acc + 0.5 * (unwrap_or(listGet(ys, i + 1), 0.0) + unwrap_or(listGet(ys, i), 0.0)) * (unwrap_or(listGet(xs, i + 1), 0.0) - unwrap_or(listGet(xs, i), 0.0)))


def roc_auc_score(y_true, y_score):
    """`sklearn.metrics.roc_auc_score` for binary labels: NaN when one class is
    absent, else the trapezoidal AUC of the ROC curve."""
    return nan() if len(unique_ints(y_true)) != 2 else auc_trapezoid(roc_curve(y_true, y_score).fpr, roc_curve(y_true, y_score).tpr)


def precision_recall_curve(y_true, y_score):
    """`sklearn.metrics.precision_recall_curve` (pos_label=1, no dropping)."""
    p = threshold_points(y_true, y_score)
    last = unwrap_or(listGet(p.tps, len(p.tps) - 1), 0.0)
    precision = toList(map(range(len(p.tps)), lambda i: precision_at(p, i)))
    recall = toList(map(range(len(p.tps)), lambda i: 1.0 if last == 0.0 else unwrap_or(listGet(p.tps, i), 0.0) / last))
    return PrecisionRecallCurve(precision=listAppend(listReverse(precision), 1.0), recall=listAppend(listReverse(recall), 0.0), thresholds=listReverse(p.thresholds))


def precision_at(p, i):
    """`tps/(tps+fps)` at point `i`, zero when no predictions."""
    ps = unwrap_or(listGet(p.tps, i), 0.0) + unwrap_or(listGet(p.fps, i), 0.0)
    return 0.0 if ps == 0.0 else unwrap_or(listGet(p.tps, i), 0.0) / ps


def average_precision_score(y_true, y_score):
    """`sklearn.metrics.average_precision_score`: the step-function integral of
    the precision-recall curve, clipped at zero."""
    curve = precision_recall_curve(y_true, y_score)
    total = fold(range(len(curve.precision) - 1), 0.0, lambda acc, i: acc + (unwrap_or(listGet(curve.recall, i), 0.0) - unwrap_or(listGet(curve.recall, i + 1), 0.0)) * unwrap_or(listGet(curve.precision, i), 0.0))
    return total if total > 0.0 else 0.0


def clip_prob(p, eps):
    """A probability clipped into `[eps, 1 - eps]`, as sklearn's log_loss."""
    if p < eps:
        return eps
    elif p > 1.0 - eps:
        return 1.0 - eps
    else:
        return p


def log_loss(y_true, y_proba):
    """`sklearn.metrics.log_loss` over a list of per-class probability rows,
    clipping each to `[eps, 1 - eps]` with eps = float64 machine epsilon."""
    eps = 2.220446049250313e-16
    losses = toList(map(range(len(y_true)), lambda i: 0.0 - ln(clip_prob(unwrap_or(listGet(unwrap_or(listGet(y_proba, i), []), unwrap_or(listGet(y_true, i), 0)), 0.0), eps))))
    return mean_float(losses)


def sq_diff(yt, yp, i):
    """`(yt[i] - yp[i])^2`."""
    d = unwrap_or(listGet(yt, i), 0.0) - unwrap_or(listGet(yp, i), 0.0)
    return d * d


def mean_squared_error(y_true, y_pred):
    """`sklearn.metrics.mean_squared_error`."""
    return mean_float(toList(map(range(len(y_true)), lambda i: sq_diff(y_true, y_pred, i))))


def mean_absolute_error(y_true, y_pred):
    """`sklearn.metrics.mean_absolute_error`."""
    return mean_float(toList(map(range(len(y_true)), lambda i: abs(unwrap_or(listGet(y_true, i), 0.0) - unwrap_or(listGet(y_pred, i), 0.0)))))


def assemble(num, den):
    """sklearn's fraction of explained deviance with force_finite=True: 1.0
    when the numerator is zero (perfect), 0.0 when only the denominator is
    zero, else `1 - num/den`."""
    return 1.0 if num == 0.0 else (0.0 if den == 0.0 else 1.0 - num / den)


def sse(yt, yp):
    """Sum of squared differences between two float lists."""
    return fold(range(len(yt)), 0.0, lambda acc, i: acc + sq_diff(yt, yp, i))


def sse_around_mean(yt):
    """Sum of squared deviations from the mean."""
    m = mean_float(yt)
    return fold(range(len(yt)), 0.0, lambda acc, i: acc + (unwrap_or(listGet(yt, i), 0.0) - m) * (unwrap_or(listGet(yt, i), 0.0) - m))


def r2_score(y_true, y_pred):
    """`sklearn.metrics.r2_score` (single output, force_finite): NaN with fewer
    than two samples."""
    return nan() if len(y_true) < 2 else assemble(sse(y_true, y_pred), sse_around_mean(y_true))


def explained_variance_score(y_true, y_pred):
    """`sklearn.metrics.explained_variance_score`: 1 minus the ratio of the
    unexplained to the total variance."""
    diffs = toList(map(range(len(y_true)), lambda i: unwrap_or(listGet(y_true, i), 0.0) - unwrap_or(listGet(y_pred, i), 0.0)))
    dm = mean_float(diffs)
    num = mean_float(toList(map(range(len(diffs)), lambda i: (unwrap_or(listGet(diffs, i), 0.0) - dm) * (unwrap_or(listGet(diffs, i), 0.0) - dm))))
    ym = mean_float(y_true)
    den = mean_float(toList(map(range(len(y_true)), lambda i: (unwrap_or(listGet(y_true, i), 0.0) - ym) * (unwrap_or(listGet(y_true, i), 0.0) - ym))))
    return assemble(num, den)


def label_encode(labels, ys):
    """Maps each label to its index in `labels`."""
    return toList(map(range(len(ys)), lambda i: index_of(labels, unwrap_or(listGet(ys, i), 0))))


def row_sum(C, i):
    """Sum of one row of an int matrix."""
    return foldList(unwrap_or(listGet(C, i), []), 0, lambda acc, x: acc + x)


def col_sum(C, j):
    """Sum of one column of an int matrix."""
    return fold(range(len(C)), 0, lambda acc, i: acc + unwrap_or(listGet(unwrap_or(listGet(C, i), []), j), 0))


def dot_float(a, b):
    """Dot product of two float lists."""
    return fold(range(len(a)), 0.0, lambda acc, i: acc + unwrap_or(listGet(a, i), 0.0) * unwrap_or(listGet(b, i), 0.0))


def matthews_corrcoef(y_true, y_pred):
    """`sklearn.metrics.matthews_corrcoef` for binary and multiclass labels."""
    labels = unique_ints(listConcat(y_true, y_pred))
    ey = label_encode(labels, y_true)
    ep = label_encode(labels, y_pred)
    classes = toList(map(range(len(labels)), lambda i: i))
    C = confusion_matrix(ey, ep, classes)
    t_sum = toList(map(range(len(labels)), lambda k: toFloat(row_sum(C, k))))
    p_sum = toList(map(range(len(labels)), lambda k: toFloat(col_sum(C, k))))
    n_correct = toFloat(fold(range(len(labels)), 0, lambda acc, k: acc + unwrap_or(listGet(unwrap_or(listGet(C, k), []), k), 0)))
    n_samples = toFloat(len(y_true))
    cov_ytyp = n_correct * n_samples - dot_float(t_sum, p_sum)
    cov_ypyp = n_samples * n_samples - dot_float(p_sum, p_sum)
    cov_ytyt = n_samples * n_samples - dot_float(t_sum, t_sum)
    den = cov_ypyp * cov_ytyt
    return 0.0 if den == 0.0 else cov_ytyp / sqrt(den)


def take(xs, k):
    """The first `k` elements of a list."""
    return fold(range(k), [], lambda acc, i: listAppend(acc, unwrap_or(listGet(xs, i), 0)))


def contains(xs, v):
    """Whether an int list contains `v`."""
    return foldList(xs, False, lambda acc, x: acc or x == v)


def hit_top_k(label, scores, k):
    """1 when `label` is among the `k` highest-scoring classes, sklearn's
    tie-breaking choosing higher indices first."""
    order = listReverse(argsort_float(scores))
    return 1 if contains(take(order, k), label) else 0


def top_k_accuracy_score(y_true, y_score, k):
    """`sklearn.metrics.top_k_accuracy_score` over a list of per-class score
    rows."""
    hits = toList(map(range(len(y_true)), lambda i: hit_top_k(unwrap_or(listGet(y_true, i), 0), unwrap_or(listGet(y_score, i), []), k)))
    return toFloat(sum_int(hits)) / toFloat(len(y_true))
