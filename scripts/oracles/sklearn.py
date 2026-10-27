"""CPython oracle for @osprey/sklearn: metrics, preprocessing and model
selection over array-likes (lists), compared against scikit-learn.

Writes packages/sklearn/tests/expected.py, which the suite compares the port
against ([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python scripts/oracles/sklearn.py`.
"""
import os
import sys

from expected_module import write

import numpy as np
import sklearn
from sklearn import metrics, model_selection, preprocessing

VERSION = {"sklearn": sklearn.__version__, "numpy": np.__version__}

# ---------------------------------------------------------------------------
# Classification metrics
# ---------------------------------------------------------------------------
YT_A = [0, 1, 2, 0, 1, 2]
YP_A = [0, 2, 1, 0, 0, 1]
YT_B = [1, 1, 1, 0, 0]
YP_B = [1, 0, 1, 0, 1]
YT_PERF = [0, 1, 2, 3]
YP_PERF = [0, 1, 2, 3]
YT_CONST = [1, 1, 1]
YP_CONST = [1, 1, 1]
YT_ZERO = [0, 0, 0, 0]
YP_ZERO = [1, 1, 1, 1]
YT_IMB = [0, 0, 0, 0, 0, 1]
YP_IMB = [0, 0, 0, 1, 1, 1]


def prf(y_true, y_pred, average, zero_division):
    return [
        metrics.precision_score(y_true, y_pred, average=average, zero_division=zero_division),
        metrics.recall_score(y_true, y_pred, average=average, zero_division=zero_division),
        metrics.f1_score(y_true, y_pred, average=average, zero_division=zero_division),
    ]


def classification_cases():
    out = {}
    # accuracy_score
    for name, yt, yp in [
        ("a", YT_A, YP_A), ("perfect", YT_PERF, YP_PERF), ("const", YT_CONST, YP_CONST),
        ("imb", YT_IMB, YP_IMB),
    ]:
        out[f"ACC_{name}"] = float(metrics.accuracy_score(yt, yp))
    # precision / recall / f1 across averages and zero_division
    for name, yt, yp, binary in [("a", YT_A, YP_A, False), ("b", YT_B, YP_B, True),
                                  ("zero", YT_ZERO, YP_ZERO, True), ("imb", YT_IMB, YP_IMB, True),
                                  ("const", YT_CONST, YP_CONST, True)]:
        avgs = ["binary", "micro", "macro", "weighted"] if binary else ["micro", "macro", "weighted"]
        for avg in avgs:
            for zd in [0, 1]:
                key = f"PRF_{name}_{avg}_{zd}"
                out[key] = prf(yt, yp, avg, zd)
    # confusion_matrix (default labels = sorted unique of both, and explicit)
    out["CM_a"] = metrics.confusion_matrix(YT_A, YP_A).tolist()
    out["CM_explicit"] = metrics.confusion_matrix(YT_B, YP_B, labels=[0, 1]).tolist()
    out["CM_wide"] = metrics.confusion_matrix([0, 0, 5, 5, 3], [5, 0, 5, 3, 0], labels=[0, 3, 5]).tolist()
    return out


# ---------------------------------------------------------------------------
# Ranking metrics
# ---------------------------------------------------------------------------
YT_ROC = [0, 0, 1, 1]
YS_ROC = [0.1, 0.4, 0.35, 0.8]
YT_TIE = [0, 1, 0, 1, 1, 0]
YS_TIE = [0.5, 0.5, 0.5, 0.5, 0.2, 0.2]
YS_PERF = [0.1, 0.2, 0.9, 0.8]
YS_REV = [0.9, 0.8, 0.1, 0.2]
YT_ONE = [1, 1, 1]
YS_ONE = [0.3, 0.8, 0.1]


def ranking_cases():
    out = {}
    for name, yt, ys in [
        ("roc", YT_ROC, YS_ROC), ("tie", YT_TIE, YS_TIE), ("perf", YT_ROC, YS_PERF),
        ("rev", YT_ROC, YS_REV), ("one", YT_ONE, YS_ONE),
    ]:
        out[f"ROCAUC_{name}"] = float(metrics.roc_auc_score(yt, ys))
        fpr, tpr, th = metrics.roc_curve(yt, ys)
        out[f"ROC_FPR_{name}"] = fpr.tolist()
        out[f"ROC_TPR_{name}"] = tpr.tolist()
        out[f"ROC_THR_{name}"] = th.tolist()
        prec, rec, th = metrics.precision_recall_curve(yt, ys)
        out[f"PRC_P_{name}"] = prec.tolist()
        out[f"PRC_R_{name}"] = rec.tolist()
        out[f"PRC_THR_{name}"] = th.tolist()
        out[f"AP_{name}"] = float(metrics.average_precision_score(yt, ys))
    return out


# ---------------------------------------------------------------------------
# Log loss and regression metrics
# ---------------------------------------------------------------------------
YT_LL = [0, 1, 2, 2]
YP_LL = [[0.8, 0.1, 0.1], [0.2, 0.7, 0.1], [0.1, 0.2, 0.7], [0.6, 0.3, 0.1]]
YT_LLB = [0, 1, 1, 0]
YP_LLB = [[0.9, 0.1], [0.1, 0.9], [0.2, 0.8], [0.8, 0.2]]
YT_CLIP = [0, 1]
YP_CLIP = [[0.0, 1.0], [1.0, 0.0]]  # completely wrong: proba clipped to eps
YP_CLIP_HI = [[1.0, 0.0], [0.0, 1.0]]  # completely right: proba clipped to 1-eps


def regression_cases():
    out = {}
    for name, yt, yp in [
        ("ll", YT_LL, YP_LL), ("llb", YT_LLB, YP_LLB), ("clip", YT_CLIP, YP_CLIP),
        ("cliphi", YT_CLIP, YP_CLIP_HI),
    ]:
        out[f"LOGLOSS_{name}"] = float(metrics.log_loss(yt, yp))
    for name, yt, yp in [
        ("a", [3.0, -0.5, 2.0, 7.0], [2.5, 0.0, 2.0, 8.0]),
        ("perf", [1.0, 2.0, 3.0], [1.0, 2.0, 3.0]),
        ("neg", [-1.0, -2.0, -3.0], [-1.1, -2.2, -3.3]),
        ("const", [5.0, 5.0, 5.0], [1.0, 2.0, 3.0]),
        ("one", [4.0], [5.0]),
    ]:
        out[f"MSE_{name}"] = float(metrics.mean_squared_error(yt, yp))
        out[f"MAE_{name}"] = float(metrics.mean_absolute_error(yt, yp))
        out[f"R2_{name}"] = float(metrics.r2_score(yt, yp))
        out[f"EVS_{name}"] = float(metrics.explained_variance_score(yt, yp))
    # matthews_corrcoef
    for name, yt, yp in [
        ("a", YT_A, YP_A), ("perf", YT_PERF, YP_PERF), ("inverse", [0, 0, 1, 1], [1, 1, 0, 0]),
        ("const", YT_CONST, YP_CONST), ("imb", YT_IMB, YP_IMB),
    ]:
        out[f"MCC_{name}"] = float(metrics.matthews_corrcoef(yt, yp))
    # top_k_accuracy_score (multiclass 2D scores)
    YT_TK = [0, 1, 2, 2]
    YS_TK = [[0.5, 0.2, 0.2], [0.3, 0.4, 0.2], [0.2, 0.4, 0.3], [0.7, 0.2, 0.1]]
    YS_TK_TIE = [[0.1, 0.1, 0.1], [0.5, 0.5, 0.5], [0.2, 0.3, 0.3], [0.9, 0.05, 0.05]]
    for k in [1, 2, 3]:
        out[f"TOPK_k{k}"] = float(metrics.top_k_accuracy_score(YT_TK, YS_TK, k=k))
        out[f"TOPK_TIE_k{k}"] = float(metrics.top_k_accuracy_score(YT_TK, YS_TK_TIE, k=k))
    return out


# ---------------------------------------------------------------------------
# Preprocessing
# ---------------------------------------------------------------------------
X_SCALE = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]
X_SCALE_ZV = [[1.0, 5.0], [2.0, 5.0], [3.0, 5.0], [4.0, 5.0]]  # second col constant
X_MM = [[0.0, 2.0], [1.0, 4.0], [2.0, 6.0], [3.0, 8.0]]
X_ROB_EVEN = [[1.0], [2.0], [3.0], [100.0]]  # median interpolated
X_ROB_ODD = [[10.0, 1.0], [20.0, 2.0], [30.0, 3.0], [40.0, 4.0], [1000.0, 5.0]]
X_NORM = [[-2.0, 1.0, 2.0], [-1.0, 0.0, 1.0], [0.0, 0.0, 0.0]]
X_OHE = [[0, 1], [2, 0], [0, 0], [2, 1]]
Y_LE = [3, 1, 1, 3, 2, 0]


def scaled(out, prefix, scaler, X, attributes, transform="TRANSFORM"):
    """Fit `scaler` on `X` and record each named fitted attribute and the transform."""
    scaler.fit(X)
    for name, attribute in attributes:
        out[f"{prefix}_{name}"] = getattr(scaler, attribute).tolist()
    out[f"{prefix}_{transform}"] = scaler.transform(X).tolist()


def preprocessing_cases():
    out = {}

    scaled(out, "SS", preprocessing.StandardScaler(), X_SCALE, (("MEAN", "mean_"), ("VAR", "var_"), ("SCALE", "scale_")))
    out["SS_ZV_SCALE"] = preprocessing.StandardScaler().fit(X_SCALE_ZV).scale_.tolist()
    out["SS_ZV_TRANSFORM"] = preprocessing.StandardScaler().fit(X_SCALE_ZV).transform(X_SCALE_ZV).tolist()
    out["SS_NOMEAN"] = preprocessing.StandardScaler(with_mean=False).fit(X_SCALE).transform(X_SCALE).tolist()
    out["SS_NOSTD"] = preprocessing.StandardScaler(with_std=False).fit(X_SCALE).transform(X_SCALE).tolist()
    out["SS_NONE"] = preprocessing.StandardScaler(with_mean=False, with_std=False).fit(X_SCALE).transform(X_SCALE).tolist()

    scaled(out, "MM", preprocessing.MinMaxScaler(), X_MM, (("MIN", "data_min_"), ("MAX", "data_max_"), ("SCALE", "scale_"), ("OFFSET", "min_")))
    out["MM_RANGE"] = preprocessing.MinMaxScaler(feature_range=(-1, 1)).fit(X_MM).transform(X_MM).tolist()

    scaled(out, "RS", preprocessing.RobustScaler(), X_ROB_EVEN, (("CENTER_EVEN", "center_"), ("SCALE_EVEN", "scale_")), "TRANSFORM_EVEN")
    scaled(out, "RS", preprocessing.RobustScaler(), X_ROB_ODD, (("CENTER_ODD", "center_"), ("SCALE_ODD", "scale_")), "TRANSFORM_ODD")

    for norm in ["l1", "l2", "max"]:
        out[f"NORM_{norm}"] = preprocessing.normalize(X_NORM, norm=norm).tolist()

    ohe = preprocessing.OneHotEncoder(sparse_output=False)
    ohe.fit(X_OHE)
    out["OHE_CATS"] = [c.tolist() for c in ohe.categories_]
    out["OHE_TRANSFORM"] = ohe.transform(X_OHE).tolist()

    le = preprocessing.LabelEncoder()
    le.fit(Y_LE)
    out["LE_CLASSES"] = le.classes_.tolist()
    out["LE_TRANSFORM"] = le.transform(Y_LE).tolist()
    out["LE_TRANSFORM_NEW"] = le.transform([0, 2, 3, 3]).tolist()

    for degree, bias, inter in [(2, True, False), (2, False, False), (3, True, False),
                                (2, True, True), (3, False, True)]:
        pf = preprocessing.PolynomialFeatures(degree=degree, include_bias=bias,
                                              interaction_only=inter)
        pf.fit(X_MM)
        powers = pf.powers_.tolist()
        out[f"PF_POWERS_d{degree}_b{int(bias)}_i{int(inter)}"] = powers
        out[f"PF_TRANSFORM_d{degree}_b{int(bias)}_i{int(inter)}"] = pf.transform(X_MM).tolist()
    return out


# ---------------------------------------------------------------------------
# Model selection
# ---------------------------------------------------------------------------
def split_with_perm(perm, test_size):
    arr = np.asarray(perm, dtype=int)
    n = len(arr)
    return arr[: n - test_size].tolist(), arr[n - test_size:].tolist()


def model_selection_cases():
    out = {}
    # train_test_split: sklearn with shuffle=False (identity permutation) and
    # an explicit permutation applied by slicing (the port's contract).
    for n, k in [(10, 3), (6, 1), (6, 5), (5, 0)]:
        tr, te = split_with_perm(list(range(n)), k)
        out[f"TTS_TR_n{n}_k{k}"] = tr
        out[f"TTS_TE_n{n}_k{k}"] = te
    tr, te = split_with_perm([2, 0, 3, 1, 4, 5], 2)
    out["TTS_PERM_TR"] = tr
    out["TTS_PERM_TE"] = te
    # KFold
    for n, nsp in [(7, 3), (6, 3), (10, 4), (5, 5), (8, 2)]:
        folds = [[tr.tolist(), te.tolist()]
                 for tr, te in model_selection.KFold(n_splits=nsp).split(np.arange(n))]
        out[f"KF_n{n}_k{nsp}"] = folds
    # StratifiedKFold
    for y, nsp in [([0, 0, 1, 1, 0, 1], 3), ([0, 1, 0, 1, 2, 2, 0, 1, 2], 3),
                   ([0, 0, 0, 1, 1, 1], 2), ([2, 1, 0, 0, 1, 2, 0], 3)]:
        key = f"SKF_{''.join(str(v) for v in y)}_k{nsp}"
        folds = [[tr.tolist(), te.tolist()]
                 for tr, te in model_selection.StratifiedKFold(n_splits=nsp).split(np.zeros(len(y)), y)]
        out[key] = folds
    return out


def main():
    """Compute every case with the real package and write tests/expected.py."""
    cases = {}
    cases.update(classification_cases())
    cases.update(ranking_cases())
    cases.update(regression_cases())
    cases.update(preprocessing_cases())
    cases.update(model_selection_cases())
    write(
        "sklearn",
        "scikit-learn metrics, preprocessing and model_selection reference values.",
        VERSION,
        cases,
    )
    print(f"wrote {len(cases)} constants to packages/sklearn/tests/expected.py")


if __name__ == "__main__":
    main()
