# osprey: flavor=python
"""@osprey/sklearn's suite: every exported function compared against values the
real scikit-learn 1.9.1 computed (src/osprey/packages/sklearn/oracle/sklearn.cpython)."""
from testing import Check
from sklearn_expected import Expected
from sklearn import Metrics
from sklearn import Preprocessing
from sklearn import ModelSelection
from sklearn import Stats

RT = 0.000000001
AT = 0.000000001
TIGHT = 0.000000000001

# Classification inputs.
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

# Ranking inputs.
YT_ROC = [0, 0, 1, 1]
YS_ROC = [0.1, 0.4, 0.35, 0.8]
YT_TIE = [0, 1, 0, 1, 1, 0]
YS_TIE = [0.5, 0.5, 0.5, 0.5, 0.2, 0.2]
YS_PERF = [0.1, 0.2, 0.9, 0.8]
YS_REV = [0.9, 0.8, 0.1, 0.2]
YT_ONE = [1, 1, 1]
YS_ONE = [0.3, 0.8, 0.1]

# Regression inputs.
YT_LL = [0, 1, 2, 2]
YP_LL = [[0.8, 0.1, 0.1], [0.2, 0.7, 0.1], [0.1, 0.2, 0.7], [0.6, 0.3, 0.1]]
YT_LLB = [0, 1, 1, 0]
YP_LLB = [[0.9, 0.1], [0.1, 0.9], [0.2, 0.8], [0.8, 0.2]]
YT_CLIP = [0, 1]
YP_CLIP = [[0.0, 1.0], [1.0, 0.0]]
YP_CLIP_HI = [[1.0, 0.0], [0.0, 1.0]]
YT_TK = [0, 1, 2, 2]
YS_TK = [[0.5, 0.2, 0.2], [0.3, 0.4, 0.2], [0.2, 0.4, 0.3], [0.7, 0.2, 0.1]]
YS_TK_TIE = [[0.1, 0.1, 0.1], [0.5, 0.5, 0.5], [0.2, 0.3, 0.3], [0.9, 0.05, 0.05]]

# Extended precision/recall inputs.
YT_ASYM3 = [0, 0, 1, 1, 2, 2, 2]
YP_ASYM3 = [0, 1, 1, 1, 2, 0, 2]
YT_RARE = [0, 0, 0, 0, 0, 1, 2]
YP_RARE = [0, 0, 1, 1, 2, 2, 2]
YT_SWAP3 = [2, 2, 0, 0, 1, 1]
YP_SWAP3 = [2, 0, 0, 2, 1, 0]
YT_WRONG4 = [0, 1, 2, 3]
YP_WRONG4 = [3, 2, 1, 0]
YT_ABSENT = [0, 0, 1, 1, 1]
YP_ABSENT = [0, 0, 0, 1, 1]
YT_SINGLE = [1, 1, 1, 1]
YP_SINGLE = [1, 1, 2, 2]

# Preprocessing inputs.
X_SCALE = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]
X_SCALE_ZV = [[1.0, 5.0], [2.0, 5.0], [3.0, 5.0], [4.0, 5.0]]
X_MM = [[0.0, 2.0], [1.0, 4.0], [2.0, 6.0], [3.0, 8.0]]
X_ROB_EVEN = [[1.0], [2.0], [3.0], [100.0]]
X_ROB_ODD = [[10.0, 1.0], [20.0, 2.0], [30.0, 3.0], [40.0, 4.0], [1000.0, 5.0]]
X_NORM = [[-2.0, 1.0, 2.0], [-1.0, 0.0, 1.0], [0.0, 0.0, 0.0]]
X_OHE = [[0, 1], [2, 0], [0, 0], [2, 1]]
Y_LE = [3, 1, 1, 3, 2, 0]

def flatten(xss):
    """A list of lists concatenated into one."""
    return foldList(xss, [], lambda acc, xs: listConcat(acc, xs))

def texts_to_floats(xs):
    """Oracle strings ("inf", "nan", "1.5") decoded to floats."""
    return toList(map(range(len(xs)), lambda i: Check.float_of(unwrap_or(listGet(xs, i), ""))))

def close_f(name, got, want, rtol, atol):
    """One float compared with numpy's isclose rule."""
    return Check.close(name, got, want, rtol, atol)

def close_metric(name, got, want):
    """One float compared with the suite's standard tolerances."""
    return close_f(name, got, want, RT, AT)

def close_list(name, got, want, rtol, atol):
    """Two flat float lists compared elementwise."""
    return Check.close_all(name, got, want, rtol, atol)

def tight_list(name, got, want):
    """Two flat float lists compared with the suite's tight tolerances."""
    return close_list(name, got, want, TIGHT, TIGHT)

def close_matrix(name, got, want, rtol, atol):
    """Two float matrices compared elementwise."""
    return Check.close_all(name, flatten(got), flatten(want), rtol, atol)

def equal_ints(name, got, want):
    """Two int lists compared exactly."""
    return Check.ints_equal(name, got, want)

def equal_int_matrix(name, got, want):
    """Two int matrices compared exactly."""
    return Check.ints_equal(name, flatten(got), flatten(want))

def range_list(n):
    """The indices 0..n-1 as a list."""
    return toList(map(range(n), lambda i: i))

class PrfSpec:
    """One precision/recall/f1 grid cell."""
    name: str
    yt: list[int]
    yp: list[int]
    avg: str
    zd: int
    want: list[float]

def prf_dataset(name, yt, yp, avg, w0, w1):
    """The zero_division 0 and 1 cells for one dataset and average."""
    return [
        PrfSpec(name=f"{name}.0", yt=yt, yp=yp, avg=avg, zd=0, want=w0),
        PrfSpec(name=f"{name}.1", yt=yt, yp=yp, avg=avg, zd=1, want=w1),
    ]

def prf_specs():
    """Every average x zero_division cell for five label configurations."""
    return flatten([
        prf_dataset("a.micro", YT_A, YP_A, "micro", Expected.PRF_a_micro_0, Expected.PRF_a_micro_1),
        prf_dataset("a.macro", YT_A, YP_A, "macro", Expected.PRF_a_macro_0, Expected.PRF_a_macro_1),
        prf_dataset("a.weighted", YT_A, YP_A, "weighted", Expected.PRF_a_weighted_0, Expected.PRF_a_weighted_1),
        prf_dataset("b.binary", YT_B, YP_B, "binary", Expected.PRF_b_binary_0, Expected.PRF_b_binary_1),
        prf_dataset("b.micro", YT_B, YP_B, "micro", Expected.PRF_b_micro_0, Expected.PRF_b_micro_1),
        prf_dataset("b.macro", YT_B, YP_B, "macro", Expected.PRF_b_macro_0, Expected.PRF_b_macro_1),
        prf_dataset("b.weighted", YT_B, YP_B, "weighted", Expected.PRF_b_weighted_0, Expected.PRF_b_weighted_1),
        prf_dataset("zero.binary", YT_ZERO, YP_ZERO, "binary", Expected.PRF_zero_binary_0, Expected.PRF_zero_binary_1),
        prf_dataset("zero.micro", YT_ZERO, YP_ZERO, "micro", Expected.PRF_zero_micro_0, Expected.PRF_zero_micro_1),
        prf_dataset("zero.macro", YT_ZERO, YP_ZERO, "macro", Expected.PRF_zero_macro_0, Expected.PRF_zero_macro_1),
        prf_dataset("zero.weighted", YT_ZERO, YP_ZERO, "weighted", Expected.PRF_zero_weighted_0, Expected.PRF_zero_weighted_1),
        prf_dataset("imb.binary", YT_IMB, YP_IMB, "binary", Expected.PRF_imb_binary_0, Expected.PRF_imb_binary_1),
        prf_dataset("imb.micro", YT_IMB, YP_IMB, "micro", Expected.PRF_imb_micro_0, Expected.PRF_imb_micro_1),
        prf_dataset("imb.macro", YT_IMB, YP_IMB, "macro", Expected.PRF_imb_macro_0, Expected.PRF_imb_macro_1),
        prf_dataset("imb.weighted", YT_IMB, YP_IMB, "weighted", Expected.PRF_imb_weighted_0, Expected.PRF_imb_weighted_1),
        prf_dataset("const.binary", YT_CONST, YP_CONST, "binary", Expected.PRF_const_binary_0, Expected.PRF_const_binary_1),
        prf_dataset("const.micro", YT_CONST, YP_CONST, "micro", Expected.PRF_const_micro_0, Expected.PRF_const_micro_1),
        prf_dataset("const.macro", YT_CONST, YP_CONST, "macro", Expected.PRF_const_macro_0, Expected.PRF_const_macro_1),
        prf_dataset("const.weighted", YT_CONST, YP_CONST, "weighted", Expected.PRF_const_weighted_0, Expected.PRF_const_weighted_1),
    ])

def prf_one(spec):
    """The three assertions for one grid cell."""
    return [
        close_metric(f"precision.{spec.name}", Metrics.precision_score(spec.yt, spec.yp, spec.avg, spec.zd), unwrap_or(listGet(spec.want, 0), 0.0)),
        close_metric(f"recall.{spec.name}", Metrics.recall_score(spec.yt, spec.yp, spec.avg, spec.zd), unwrap_or(listGet(spec.want, 1), 0.0)),
        close_metric(f"f1.{spec.name}", Metrics.f1_score(spec.yt, spec.yp, spec.avg, spec.zd), unwrap_or(listGet(spec.want, 2), 0.0)),
    ]

def prf_cases(specs):
    """precision, recall and f1 across a grid of cells."""
    return foldList(specs, [], lambda acc, spec: listConcat(acc, prf_one(spec)))

def accuracy_cases():
    """accuracy_score against the oracle."""
    return [
        close_metric("accuracy.a", Metrics.accuracy_score(YT_A, YP_A), Expected.ACC_a),
        close_metric("accuracy.perfect", Metrics.accuracy_score(YT_PERF, YP_PERF), Expected.ACC_perfect),
        close_metric("accuracy.const", Metrics.accuracy_score(YT_CONST, YP_CONST), Expected.ACC_const),
        close_metric("accuracy.imb", Metrics.accuracy_score(YT_IMB, YP_IMB), Expected.ACC_imb),
    ]

def cm_case(name, yt, yp, labels, want):
    """One confusion_matrix comparison against the oracle."""
    return equal_int_matrix(f"cm.{name}", Metrics.confusion_matrix(yt, yp, labels), want)

def confusion_cases():
    """confusion_matrix against the oracle."""
    return [
        cm_case("a", YT_A, YP_A, [0, 1, 2], Expected.CM_a),
        cm_case("explicit", YT_B, YP_B, [0, 1], Expected.CM_explicit),
        cm_case("wide", [0, 0, 5, 5, 3], [5, 0, 5, 3, 0], [0, 3, 5], Expected.CM_wide),
    ]

def roc_case(key, yt, ys, auc_want, fpr_want, tpr_want, thr_want, prc_p, prc_r, prc_thr, ap_want):
    """roc_curve, precision_recall_curve, roc_auc and average_precision."""
    curve = Metrics.roc_curve(yt, ys)
    pr = Metrics.precision_recall_curve(yt, ys)
    return [
        close_metric(f"roc_auc.{key}", Metrics.roc_auc_score(yt, ys), auc_want),
        tight_list(f"roc_fpr.{key}", curve.fpr, fpr_want),
        tight_list(f"roc_tpr.{key}", curve.tpr, tpr_want),
        tight_list(f"roc_thr.{key}", curve.thresholds, texts_to_floats(thr_want)),
        tight_list(f"prc_p.{key}", pr.precision, prc_p),
        tight_list(f"prc_r.{key}", pr.recall, prc_r),
        tight_list(f"prc_thr.{key}", pr.thresholds, prc_thr),
        close_metric(f"ap.{key}", Metrics.average_precision_score(yt, ys), ap_want),
    ]

def ranking_cases():
    """All ranking metrics across perfect, tied, reversed and single-class."""
    return flatten([
        roc_case("roc", YT_ROC, YS_ROC, Expected.ROCAUC_roc, Expected.ROC_FPR_roc, Expected.ROC_TPR_roc, Expected.ROC_THR_roc, Expected.PRC_P_roc, Expected.PRC_R_roc, Expected.PRC_THR_roc, Expected.AP_roc),
        roc_case("tie", YT_TIE, YS_TIE, Expected.ROCAUC_tie, Expected.ROC_FPR_tie, Expected.ROC_TPR_tie, Expected.ROC_THR_tie, Expected.PRC_P_tie, Expected.PRC_R_tie, Expected.PRC_THR_tie, Expected.AP_tie),
        roc_case("perf", YT_ROC, YS_PERF, Expected.ROCAUC_perf, Expected.ROC_FPR_perf, Expected.ROC_TPR_perf, Expected.ROC_THR_perf, Expected.PRC_P_perf, Expected.PRC_R_perf, Expected.PRC_THR_perf, Expected.AP_perf),
        roc_case("rev", YT_ROC, YS_REV, Expected.ROCAUC_rev, Expected.ROC_FPR_rev, Expected.ROC_TPR_rev, Expected.ROC_THR_rev, Expected.PRC_P_rev, Expected.PRC_R_rev, Expected.PRC_THR_rev, Expected.AP_rev),
        roc_case("one", YT_ONE, YS_ONE, Check.float_of(Expected.ROCAUC_one), texts_to_floats(Expected.ROC_FPR_one), Expected.ROC_TPR_one, Expected.ROC_THR_one, Expected.PRC_P_one, Expected.PRC_R_one, Expected.PRC_THR_one, Expected.AP_one),
    ])

def ll_case(name, yt, yp, want, atol):
    """One log_loss comparison, with its own absolute tolerance."""
    return close_f(f"logloss.{name}", Metrics.log_loss(yt, yp), want, RT, atol)

def logloss_cases():
    """log_loss, including the machine-epsilon clipping path."""
    return [
        ll_case("ll", YT_LL, YP_LL, Expected.LOGLOSS_ll, AT),
        ll_case("llb", YT_LLB, YP_LLB, Expected.LOGLOSS_llb, AT),
        ll_case("clip", YT_CLIP, YP_CLIP, Expected.LOGLOSS_clip, AT),
        ll_case("cliphi", YT_CLIP, YP_CLIP_HI, Expected.LOGLOSS_cliphi, 0.0),
    ]

def reg_pair(name, yt, yp, mse_want, mae_want, r2_want, evs_want):
    """The four regression metrics for one pair of lists."""
    return [
        close_metric(f"mse.{name}", Metrics.mean_squared_error(yt, yp), mse_want),
        close_metric(f"mae.{name}", Metrics.mean_absolute_error(yt, yp), mae_want),
        close_metric(f"r2.{name}", Metrics.r2_score(yt, yp), r2_want),
        close_metric(f"evs.{name}", Metrics.explained_variance_score(yt, yp), evs_want),
    ]

def mse_cases():
    """Regression scores over a grid of target/prediction pairs."""
    return flatten([
        reg_pair("a", [3.0, -0.5, 2.0, 7.0], [2.5, 0.0, 2.0, 8.0], Expected.MSE_a, Expected.MAE_a, Expected.R2_a, Expected.EVS_a),
        reg_pair("perf", [1.0, 2.0, 3.0], [1.0, 2.0, 3.0], Expected.MSE_perf, Expected.MAE_perf, Expected.R2_perf, Expected.EVS_perf),
        reg_pair("neg", [-1.0, -2.0, -3.0], [-1.1, -2.2, -3.3], Expected.MSE_neg, Expected.MAE_neg, Expected.R2_neg, Expected.EVS_neg),
        reg_pair("const", [5.0, 5.0, 5.0], [1.0, 2.0, 3.0], Expected.MSE_const, Expected.MAE_const, Expected.R2_const, Expected.EVS_const),
        reg_pair("one", [4.0], [5.0], Expected.MSE_one, Expected.MAE_one, Check.float_of(Expected.R2_one), Expected.EVS_one),
    ])

def mcc_cases():
    """matthews_corrcoef over five configurations."""
    return [
        close_metric("mcc.a", Metrics.matthews_corrcoef(YT_A, YP_A), Expected.MCC_a),
        close_metric("mcc.perf", Metrics.matthews_corrcoef(YT_PERF, YP_PERF), Expected.MCC_perf),
        close_metric("mcc.inverse", Metrics.matthews_corrcoef([0, 0, 1, 1], [1, 1, 0, 0]), Expected.MCC_inverse),
        close_metric("mcc.const", Metrics.matthews_corrcoef(YT_CONST, YP_CONST), Expected.MCC_const),
        close_metric("mcc.imb", Metrics.matthews_corrcoef(YT_IMB, YP_IMB), Expected.MCC_imb),
    ]

def topk_row(tag, ys, wants):
    """top_k_accuracy_score for k = 1..3 against the per-k oracle values."""
    return toList(map(range(len(wants)), lambda i: close_metric(f"topk.{tag}k{i + 1}", Metrics.top_k_accuracy_score(YT_TK, ys, i + 1), unwrap_or(listGet(wants, i), 0.0))))

def topk_cases():
    """top_k_accuracy_score for k in 1..3, with and without tied scores."""
    return listConcat(
        topk_row("", YS_TK, [Expected.TOPK_k1, Expected.TOPK_k2, Expected.TOPK_k3]),
        topk_row("tie.", YS_TK_TIE, [Expected.TOPK_TIE_k1, Expected.TOPK_TIE_k2, Expected.TOPK_TIE_k3]),
    )

def fit_standard(with_mean, with_std, xs):
    """A StandardScaler fitted on xs with the given flags."""
    return Preprocessing.standard_scaler_fit(Preprocessing.StandardScaler(with_mean=with_mean, with_std=with_std), xs)

def fit_minmax(lo, hi, xs):
    """A MinMaxScaler fitted on xs mapping features into [lo, hi]."""
    return Preprocessing.minmax_scaler_fit(Preprocessing.MinMaxScaler(feature_min=lo, feature_max=hi), xs)

def fit_robust(xs):
    """A RobustScaler fitted on xs with the default quartiles."""
    return Preprocessing.robust_scaler_fit(Preprocessing.RobustScaler(quantile_min=25.0, quantile_max=75.0), xs)

def standard_scaler_cases():
    """StandardScaler fits and transforms against the oracle."""
    ss_fit = fit_standard(True, True, X_SCALE)
    ss_zv = fit_standard(True, True, X_SCALE_ZV)
    ss_nomean = fit_standard(False, True, X_SCALE)
    ss_nostd = fit_standard(True, False, X_SCALE)
    ss_none = fit_standard(False, False, X_SCALE)
    return [
        close_list("ss.mean", ss_fit.mean, Expected.SS_MEAN, RT, AT),
        close_list("ss.scale", ss_fit.scale, Expected.SS_SCALE, RT, AT),
        close_matrix("ss.transform", Preprocessing.standard_scaler_transform(ss_fit, X_SCALE), Expected.SS_TRANSFORM, RT, AT),
        close_list("ss.zv.scale", ss_zv.scale, Expected.SS_ZV_SCALE, RT, AT),
        close_matrix("ss.zv.transform", Preprocessing.standard_scaler_transform(ss_zv, X_SCALE_ZV), Expected.SS_ZV_TRANSFORM, RT, AT),
        close_matrix("ss.nomean", Preprocessing.standard_scaler_transform(ss_nomean, X_SCALE), Expected.SS_NOMEAN, RT, AT),
        close_matrix("ss.nostd", Preprocessing.standard_scaler_transform(ss_nostd, X_SCALE), Expected.SS_NOSTD, RT, AT),
        close_matrix("ss.none", Preprocessing.standard_scaler_transform(ss_none, X_SCALE), Expected.SS_NONE, RT, AT),
    ]

def minmax_scaler_cases():
    """MinMaxScaler fits and transforms against the oracle."""
    mm = fit_minmax(0.0, 1.0, X_MM)
    mm_r = fit_minmax(-1.0, 1.0, X_MM)
    return [
        close_list("mm.min", mm.data_min, Expected.MM_MIN, RT, AT),
        close_list("mm.max", mm.data_max, Expected.MM_MAX, RT, AT),
        close_list("mm.scale", mm.scale, Expected.MM_SCALE, RT, AT),
        close_list("mm.offset", mm.min_, Expected.MM_OFFSET, RT, AT),
        close_matrix("mm.transform", Preprocessing.minmax_scaler_transform(mm, X_MM), Expected.MM_TRANSFORM, RT, AT),
        close_matrix("mm.range", Preprocessing.minmax_scaler_transform(mm_r, X_MM), Expected.MM_RANGE, RT, AT),
    ]

def robust_scaler_cases():
    """RobustScaler fits and transforms against the oracle."""
    rs_e = fit_robust(X_ROB_EVEN)
    rs_o = fit_robust(X_ROB_ODD)
    return [
        close_list("rs.center.even", rs_e.center, Expected.RS_CENTER_EVEN, RT, AT),
        close_list("rs.scale.even", rs_e.scale, Expected.RS_SCALE_EVEN, RT, AT),
        close_matrix("rs.transform.even", Preprocessing.robust_scaler_transform(rs_e, X_ROB_EVEN), Expected.RS_TRANSFORM_EVEN, RT, AT),
        close_list("rs.center.odd", rs_o.center, Expected.RS_CENTER_ODD, RT, AT),
        close_list("rs.scale.odd", rs_o.scale, Expected.RS_SCALE_ODD, RT, AT),
        close_matrix("rs.transform.odd", Preprocessing.robust_scaler_transform(rs_o, X_ROB_ODD), Expected.RS_TRANSFORM_ODD, RT, AT),
    ]

def scaler_cases():
    """StandardScaler, MinMaxScaler and RobustScaler against the oracle."""
    return flatten([standard_scaler_cases(), minmax_scaler_cases(), robust_scaler_cases()])

def normalize_cases():
    """normalize under l1, l2 and max."""
    return [
        close_matrix("norm.l1", Preprocessing.normalize(X_NORM, "l1"), Expected.NORM_l1, RT, AT),
        close_matrix("norm.l2", Preprocessing.normalize(X_NORM, "l2"), Expected.NORM_l2, RT, AT),
        close_matrix("norm.max", Preprocessing.normalize(X_NORM, "max"), Expected.NORM_max, RT, AT),
    ]

def encoder_cases():
    """OneHotEncoder and LabelEncoder against the oracle."""
    ohe_cats = Preprocessing.one_hot_encoder_fit(X_OHE)
    le_classes = Preprocessing.label_encoder_fit(Y_LE)
    return [
        equal_int_matrix("ohe.cats", ohe_cats, Expected.OHE_CATS),
        close_matrix("ohe.transform", Preprocessing.one_hot_encoder_transform(ohe_cats, X_OHE), Expected.OHE_TRANSFORM, TIGHT, TIGHT),
        equal_ints("le.classes", le_classes, Expected.LE_CLASSES),
        equal_ints("le.transform", Preprocessing.label_encoder_transform(le_classes, Y_LE), Expected.LE_TRANSFORM),
        equal_ints("le.transform.new", Preprocessing.label_encoder_transform(le_classes, [0, 2, 3, 3]), Expected.LE_TRANSFORM_NEW),
    ]

def poly_case(degree, bias, inter, powers_want, transform_want):
    """polynomial_powers and its transform for one configuration."""
    flags = f"b{1 if bias else 0}.i{1 if inter else 0}"
    powers = Preprocessing.polynomial_powers(Preprocessing.PolynomialFeatures(degree=degree, include_bias=bias, interaction_only=inter), 2)
    return [
        equal_int_matrix(f"pf.powers.d{degree}.{flags}", powers, powers_want),
        close_matrix(f"pf.transform.d{degree}.{flags}", Preprocessing.polynomial_features_transform(powers, X_MM), transform_want, RT, AT),
    ]

def polynomial_cases():
    """PolynomialFeatures across degree, bias and interaction_only."""
    return flatten([
        poly_case(2, True, False, Expected.PF_POWERS_d2_b1_i0, Expected.PF_TRANSFORM_d2_b1_i0),
        poly_case(2, False, False, Expected.PF_POWERS_d2_b0_i0, Expected.PF_TRANSFORM_d2_b0_i0),
        poly_case(3, True, False, Expected.PF_POWERS_d3_b1_i0, Expected.PF_TRANSFORM_d3_b1_i0),
        poly_case(2, True, True, Expected.PF_POWERS_d2_b1_i1, Expected.PF_TRANSFORM_d2_b1_i1),
        poly_case(3, False, True, Expected.PF_POWERS_d3_b0_i1, Expected.PF_TRANSFORM_d3_b0_i1),
    ])

def tts_case(name, perm, k, tr_want, te_want):
    """train_test_split for one permutation and test size."""
    split = ModelSelection.train_test_split(perm, k)
    return [equal_ints(f"tts.train.{name}", split.train, tr_want), equal_ints(f"tts.test.{name}", split.test, te_want)]

def tts_cases():
    """train_test_split over several sizes and a shuffled permutation."""
    return flatten([
        tts_case("n10.k3", range_list(10), 3, Expected.TTS_TR_n10_k3, Expected.TTS_TE_n10_k3),
        tts_case("n6.k1", range_list(6), 1, Expected.TTS_TR_n6_k1, Expected.TTS_TE_n6_k1),
        tts_case("n6.k5", range_list(6), 5, Expected.TTS_TR_n6_k5, Expected.TTS_TE_n6_k5),
        tts_case("n5.k0", range_list(5), 0, Expected.TTS_TR_n5_k0, Expected.TTS_TE_n5_k0),
        tts_case("perm", [2, 0, 3, 1, 4, 5], 2, Expected.TTS_PERM_TR, Expected.TTS_PERM_TE),
    ])

def split_grid(name, folds, want):
    """Compares a list of Split records against [[train, test], ...]."""
    trains = toList(map(range(len(folds)), lambda i: equal_ints(f"{name}.train.{i}", unwrap_or(listGet(folds, i), ModelSelection.Split(train=[], test=[])).train, unwrap_or(listGet(unwrap_or(listGet(want, i), []), 0), []))))
    tests = toList(map(range(len(folds)), lambda i: equal_ints(f"{name}.test.{i}", unwrap_or(listGet(folds, i), ModelSelection.Split(train=[], test=[])).test, unwrap_or(listGet(unwrap_or(listGet(want, i), []), 1), []))))
    return listConcat(trains, tests)

def kfold_cases():
    """KFold splits for several n and n_splits."""
    return flatten([
        split_grid("kf.n7.k3", ModelSelection.kfold_splits(7, 3), Expected.KF_n7_k3),
        split_grid("kf.n6.k3", ModelSelection.kfold_splits(6, 3), Expected.KF_n6_k3),
        split_grid("kf.n10.k4", ModelSelection.kfold_splits(10, 4), Expected.KF_n10_k4),
        split_grid("kf.n5.k5", ModelSelection.kfold_splits(5, 5), Expected.KF_n5_k5),
        split_grid("kf.n8.k2", ModelSelection.kfold_splits(8, 2), Expected.KF_n8_k2),
    ])

def skf_cases():
    """StratifiedKFold splits for several class distributions."""
    return flatten([
        split_grid("skf.001101.k3", ModelSelection.stratified_kfold_splits([0, 0, 1, 1, 0, 1], 3), Expected.SKF_001101_k3),
        split_grid("skf.010122012.k3", ModelSelection.stratified_kfold_splits([0, 1, 0, 1, 2, 2, 0, 1, 2], 3), Expected.SKF_010122012_k3),
        split_grid("skf.000111.k2", ModelSelection.stratified_kfold_splits([0, 0, 0, 1, 1, 1], 2), Expected.SKF_000111_k2),
        split_grid("skf.2100120.k3", ModelSelection.stratified_kfold_splits([2, 1, 0, 0, 1, 2, 0], 3), Expected.SKF_2100120_k3),
    ])

def stats_cases():
    """The shared Stats helpers."""
    return [
        close_f("stats.sum", Stats.sum_float([1.0, 2.5, 3.0]), 6.5, TIGHT, TIGHT),
        Check.equal("stats.cmp_int_less", Stats.cmp_int(2, 5), -1),
        Check.equal("stats.cmp_int_more", Stats.cmp_int(5, 2), 1),
        Check.equal("stats.cmp_int_same", Stats.cmp_int(3, 3), 0),
        Check.ints_equal("stats.encode_labels", Stats.encode_labels([3, 7, 9], [9, 3, 7, 9]), [2, 0, 1, 2]),
        Check.ints_equal("stats.map_indexed", Stats.map_indexed([1.5, 2.5], lambda i: i * 4), [0, 4]),
        close_f("stats.mean", Stats.mean_float([1.0, 2.0, 3.0]), 2.0, TIGHT, TIGHT),
        close_f("stats.min", Stats.min_float([3.0, 1.0, 2.0]), 1.0, TIGHT, TIGHT),
        close_f("stats.max", Stats.max_float([3.0, 1.0, 2.0]), 3.0, TIGHT, TIGHT),
        equal_ints("stats.argsort", Stats.argsort_float([3.0, 1.0, 3.0, 0.5]), [3, 1, 0, 2]),
        equal_ints("stats.unique", Stats.unique_ints([3, 1, 3, 2, 1]), [1, 2, 3]),
        equal_ints("stats.index_of", [Stats.index_of([10, 20, 30], 20), Stats.index_of([10, 20, 30], 99)], [1, -1]),
    ]

def record_cases():
    """Constructs each exported record type once so the suite names it."""
    r = Metrics.RocCurve(fpr=[0.0], tpr=[0.0], thresholds=[0.0])
    p = Metrics.PrecisionRecallCurve(precision=[0.0], recall=[0.0], thresholds=[0.0])
    s = Preprocessing.StandardScalerFitted(with_mean=True, with_std=True, mean=[0.0], scale=[0.0])
    m = Preprocessing.MinMaxScalerFitted(data_min=[0.0], data_max=[0.0], scale=[0.0], min_=[0.0])
    rb = Preprocessing.RobustScalerFitted(center=[0.0], scale=[0.0])
    return [
        Check.truth("records.roccurve", len(r.fpr) == 1, "RocCurve"),
        Check.truth("records.prcurve", len(p.precision) == 1, "PrecisionRecallCurve"),
        Check.truth("records.ssfitted", s.with_mean and len(s.mean) == 1, "StandardScalerFitted"),
        Check.truth("records.mmfitted", len(m.scale) == 1, "MinMaxScalerFitted"),
        Check.truth("records.rsfitted", len(rb.center) == 1, "RobustScalerFitted"),
    ]

def ext_prf_specs():
    """Extended grid: asymmetric multiclass, tie and prediction-skew rows, both zero_division values."""
    return flatten([
        prf_dataset("asym3.micro", YT_ASYM3, YP_ASYM3, "micro", Expected.XPRF_asym3_micro_0, Expected.XPRF_asym3_micro_1),
        prf_dataset("asym3.macro", YT_ASYM3, YP_ASYM3, "macro", Expected.XPRF_asym3_macro_0, Expected.XPRF_asym3_macro_1),
        prf_dataset("asym3.weighted", YT_ASYM3, YP_ASYM3, "weighted", Expected.XPRF_asym3_weighted_0, Expected.XPRF_asym3_weighted_1),
        prf_dataset("rare.micro", YT_RARE, YP_RARE, "micro", Expected.XPRF_rare_micro_0, Expected.XPRF_rare_micro_1),
        prf_dataset("rare.macro", YT_RARE, YP_RARE, "macro", Expected.XPRF_rare_macro_0, Expected.XPRF_rare_macro_1),
        prf_dataset("rare.weighted", YT_RARE, YP_RARE, "weighted", Expected.XPRF_rare_weighted_0, Expected.XPRF_rare_weighted_1),
        prf_dataset("swap3.micro", YT_SWAP3, YP_SWAP3, "micro", Expected.XPRF_swap3_micro_0, Expected.XPRF_swap3_micro_1),
        prf_dataset("swap3.macro", YT_SWAP3, YP_SWAP3, "macro", Expected.XPRF_swap3_macro_0, Expected.XPRF_swap3_macro_1),
        prf_dataset("swap3.weighted", YT_SWAP3, YP_SWAP3, "weighted", Expected.XPRF_swap3_weighted_0, Expected.XPRF_swap3_weighted_1),
        prf_dataset("wrong4.micro", YT_WRONG4, YP_WRONG4, "micro", Expected.XPRF_wrong4_micro_0, Expected.XPRF_wrong4_micro_1),
        prf_dataset("wrong4.macro", YT_WRONG4, YP_WRONG4, "macro", Expected.XPRF_wrong4_macro_0, Expected.XPRF_wrong4_macro_1),
        prf_dataset("wrong4.weighted", YT_WRONG4, YP_WRONG4, "weighted", Expected.XPRF_wrong4_weighted_0, Expected.XPRF_wrong4_weighted_1),
        prf_dataset("absent.micro", YT_ABSENT, YP_ABSENT, "micro", Expected.XPRF_absent_micro_0, Expected.XPRF_absent_micro_1),
        prf_dataset("absent.macro", YT_ABSENT, YP_ABSENT, "macro", Expected.XPRF_absent_macro_0, Expected.XPRF_absent_macro_1),
        prf_dataset("absent.weighted", YT_ABSENT, YP_ABSENT, "weighted", Expected.XPRF_absent_weighted_0, Expected.XPRF_absent_weighted_1),
        prf_dataset("single.micro", YT_SINGLE, YP_SINGLE, "micro", Expected.XPRF_single_micro_0, Expected.XPRF_single_micro_1),
        prf_dataset("single.macro", YT_SINGLE, YP_SINGLE, "macro", Expected.XPRF_single_macro_0, Expected.XPRF_single_macro_1),
        prf_dataset("single.weighted", YT_SINGLE, YP_SINGLE, "weighted", Expected.XPRF_single_weighted_0, Expected.XPRF_single_weighted_1),
        prf_dataset("bin.rare", [0, 0, 0, 0, 0, 0, 1, 1], [0, 0, 0, 1, 1, 1, 1, 0], "binary", Expected.XPRF_bin_rare_binary_0, Expected.XPRF_bin_rare_binary_1),
        prf_dataset("bin.constpred", [0, 1, 0, 1, 0, 1], [0, 0, 0, 0, 0, 0], "binary", Expected.XPRF_bin_constpred_binary_0, Expected.XPRF_bin_constpred_binary_1),
        prf_dataset("bin.ties", [0, 1, 0, 1, 0, 1], [1, 0, 1, 0, 1, 0], "binary", Expected.XPRF_bin_ties_binary_0, Expected.XPRF_bin_ties_binary_1),
    ])

def ext_reg_cases():
    """Regression scores over ties, shifted/scaled, zero/negative and imbalanced targets."""
    return flatten([
        reg_pair("tie", [2.0, 2.0, 3.0, 3.0], [2.0, 3.0, 2.0, 3.0], Expected.XREG_MSE_tie, Expected.XREG_MAE_tie, Expected.XREG_R2_tie, Expected.XREG_EVS_tie),
        reg_pair("shift", [11.0, 12.0, 13.0, 14.0], [10.5, 12.0, 13.5, 14.0], Expected.XREG_MSE_shift, Expected.XREG_MAE_shift, Expected.XREG_R2_shift, Expected.XREG_EVS_shift),
        reg_pair("scale", [10.0, -5.0, 20.0], [11.0, -4.5, 19.0], Expected.XREG_MSE_scale, Expected.XREG_MAE_scale, Expected.XREG_R2_scale, Expected.XREG_EVS_scale),
        reg_pair("cross0", [-2.0, -1.0, 1.0, 2.0], [-1.5, -1.5, 1.5, 1.5], Expected.XREG_MSE_cross0, Expected.XREG_MAE_cross0, Expected.XREG_R2_cross0, Expected.XREG_EVS_cross0),
        reg_pair("negsmall", [-3.0, -2.0, -1.0], [-3.0, -2.0, -2.0], Expected.XREG_MSE_negsmall, Expected.XREG_MAE_negsmall, Expected.XREG_R2_negsmall, Expected.XREG_EVS_negsmall),
        reg_pair("zeropred", [1.0, 2.0, 3.0], [0.0, 0.0, 0.0], Expected.XREG_MSE_zeropred, Expected.XREG_MAE_zeropred, Expected.XREG_R2_zeropred, Expected.XREG_EVS_zeropred),
        reg_pair("constpred", [5.0, 7.0, 9.0], [7.0, 7.0, 7.0], Expected.XREG_MSE_constpred, Expected.XREG_MAE_constpred, Expected.XREG_R2_constpred, Expected.XREG_EVS_constpred),
        reg_pair("flat", [4.0, 4.0, 4.0], [4.0, 4.0, 4.0], Expected.XREG_MSE_flat, Expected.XREG_MAE_flat, Expected.XREG_R2_flat, Expected.XREG_EVS_flat),
        reg_pair("imbal", [0.0, 0.0, 0.0, 0.0, 100.0], [1.0, 0.0, 0.0, 0.0, 99.0], Expected.XREG_MSE_imbal, Expected.XREG_MAE_imbal, Expected.XREG_R2_imbal, Expected.XREG_EVS_imbal),
        reg_pair("ties3", [1.0, 1.0, 1.0, 2.0, 2.0], [1.0, 2.0, 1.0, 2.0, 1.0], Expected.XREG_MSE_ties3, Expected.XREG_MAE_ties3, Expected.XREG_R2_ties3, Expected.XREG_EVS_ties3),
        reg_pair("shiftneg", [-10.0, -20.0, -30.0], [-9.0, -22.0, -31.0], Expected.XREG_MSE_shiftneg, Expected.XREG_MAE_shiftneg, Expected.XREG_R2_shiftneg, Expected.XREG_EVS_shiftneg),
        reg_pair("scaleup", [0.5, 1.0, 1.5], [0.4, 1.1, 1.6], Expected.XREG_MSE_scaleup, Expected.XREG_MAE_scaleup, Expected.XREG_R2_scaleup, Expected.XREG_EVS_scaleup),
    ])

def ext_cases():
    """The extended asymmetric multiclass and regression input matrix."""
    return listConcat(prf_cases(ext_prf_specs()), ext_reg_cases())

def metric_cases():
    """Classification and ranking metric cases."""
    return flatten([accuracy_cases(), prf_cases(prf_specs()), confusion_cases(), ranking_cases()])

def regression_cases():
    """Loss and regression score cases."""
    return flatten([logloss_cases(), mse_cases(), mcc_cases(), topk_cases()])

def preprocessing_cases():
    """Scaler, normalizer, encoder and polynomial cases."""
    return flatten([scaler_cases(), normalize_cases(), encoder_cases(), polynomial_cases()])

def model_selection_cases():
    """Splitter cases."""
    return flatten([tts_cases(), kfold_cases(), skf_cases()])

def all_cases():
    """The full outcome list."""
    return flatten([
        metric_cases(),
        regression_cases(),
        preprocessing_cases(),
        model_selection_cases(),
        stats_cases(),
        record_cases(),
        ext_cases(),
    ])

def main():
    return Check.run(all_cases)
