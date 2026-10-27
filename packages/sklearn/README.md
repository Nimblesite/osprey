# @osprey/sklearn

A port of [scikit-learn](https://scikit-learn.org/) 1.9.1 essentials to the
Osprey language's Python flavor. Every function is compared against the real
scikit-learn (numpy 2.5.4) by `scripts/oracles/sklearn.py`, which writes
`tests/expected.py`; the suite (`tests/main.py`) checks each exported name and
reports through `@osprey/testing`.

Data is plain Osprey lists (`list[int]`, `list[float]`, `list[list[float]]`),
which is exactly what scikit-learn accepts as array-likes. Estimators are
immutable records of their hyperparameters; `*_fit` returns a fitted record and
`*_transform` applies it, so `StandardScaler().fit(X).transform(X)` becomes
`standard_scaler_transform(standard_scaler_fit(scaler, X), X)`.

## Modules

| Module | Contents |
| --- | --- |
| `sklearn.Stats` | Shared support helpers (sum/mean/min/max, stable argsort, unique, index-of) |
| `sklearn.Metrics` | `accuracy_score`, `precision_score`, `recall_score`, `f1_score`, `confusion_matrix`, `roc_auc_score`, `roc_curve`, `precision_recall_curve`, `average_precision_score`, `log_loss`, `mean_squared_error`, `mean_absolute_error`, `r2_score`, `explained_variance_score`, `matthews_corrcoef`, `top_k_accuracy_score` |
| `sklearn.Preprocessing` | `StandardScaler`, `MinMaxScaler`, `RobustScaler`, `normalize` (Normalizer), OneHotEncoder (`one_hot_encoder_fit`/`_transform`), LabelEncoder (`label_encoder_fit`/`_transform`), `PolynomialFeatures` (`polynomial_powers`/`polynomial_features_transform`) |
| `sklearn.ModelSelection` | `train_test_split`, `kfold_splits` (KFold), `stratified_kfold_splits` (StratifiedKFold) |

## API map

Every ported name maps to the original as follows (all are module-level
functions or records over lists):

| Port name | Original |
| --- | --- |
| `Metrics.accuracy_score(y_true, y_pred)` | `sklearn.metrics.accuracy_score` (normalize=True) |
| `Metrics.precision_score(y_true, y_pred, average, zero_division)` | `sklearn.metrics.precision_score` |
| `Metrics.recall_score(y_true, y_pred, average, zero_division)` | `sklearn.metrics.recall_score` |
| `Metrics.f1_score(y_true, y_pred, average, zero_division)` | `sklearn.metrics.f1_score` |
| `Metrics.confusion_matrix(y_true, y_pred, labels)` | `sklearn.metrics.confusion_matrix` (explicit labels) |
| `Metrics.roc_auc_score(y_true, y_score)` | `sklearn.metrics.roc_auc_score` (binary) |
| `Metrics.roc_curve(y_true, y_score)` | `sklearn.metrics.roc_curve` (pos_label=1, drop_intermediate=True) |
| `Metrics.precision_recall_curve(y_true, y_score)` | `sklearn.metrics.precision_recall_curve` |
| `Metrics.average_precision_score(y_true, y_score)` | `sklearn.metrics.average_precision_score` |
| `Metrics.log_loss(y_true, y_proba)` | `sklearn.metrics.log_loss` (rows of per-class probabilities) |
| `Metrics.mean_squared_error(y_true, y_pred)` | `sklearn.metrics.mean_squared_error` |
| `Metrics.mean_absolute_error(y_true, y_pred)` | `sklearn.metrics.mean_absolute_error` |
| `Metrics.r2_score(y_true, y_pred)` | `sklearn.metrics.r2_score` (single output, force_finite) |
| `Metrics.explained_variance_score(y_true, y_pred)` | `sklearn.metrics.explained_variance_score` |
| `Metrics.matthews_corrcoef(y_true, y_pred)` | `sklearn.metrics.matthews_corrcoef` |
| `Metrics.top_k_accuracy_score(y_true, y_score, k)` | `sklearn.metrics.top_k_accuracy_score` (multiclass 2D scores) |
| `Preprocessing.StandardScaler` + `standard_scaler_fit`/`standard_scaler_transform` | `sklearn.preprocessing.StandardScaler` |
| `Preprocessing.MinMaxScaler` + `minmax_scaler_fit`/`minmax_scaler_transform` | `sklearn.preprocessing.MinMaxScaler` |
| `Preprocessing.RobustScaler` + `robust_scaler_fit`/`robust_scaler_transform` | `sklearn.preprocessing.RobustScaler` |
| `Preprocessing.normalize(X, norm)` | `sklearn.preprocessing.normalize` / `Normalizer` |
| `Preprocessing.label_encoder_fit`/`label_encoder_transform` | `sklearn.preprocessing.LabelEncoder` |
| `Preprocessing.one_hot_encoder_fit`/`one_hot_encoder_transform` | `sklearn.preprocessing.OneHotEncoder` (dense output) |
| `Preprocessing.PolynomialFeatures` + `polynomial_powers`/`polynomial_features_transform` | `sklearn.preprocessing.PolynomialFeatures` |
| `ModelSelection.train_test_split(permutation, test_size)` | `sklearn.model_selection.train_test_split` |
| `ModelSelection.kfold_splits(n_samples, n_splits)` | `sklearn.model_selection.KFold(n_splits).split` (no shuffle) |
| `ModelSelection.stratified_kfold_splits(y, n_splits)` | `sklearn.model_selection.StratifiedKFold(n_splits).split` (no shuffle) |

`Stats` is a support module, not scikit-learn API; its helpers are exported
because the other modules import them, and each is tested.

## Divergences

- **Immutable estimator records.** scikit-learn's estimators are mutable objects
  whose `fit` mutates `self`; Osprey values are immutable, so hyperparameters
  are a record, `fit` returns a fitted record, and `transform` takes it. The
  record field names follow the learned attributes (`mean`, `scale`,
  `data_min`, `center`, ...).
- **No default arguments.** The Python flavor has no default/keyword-only
  parameters, so every argument is explicit (e.g. `with_mean`/`with_std`,
  `quantile_min`/`quantile_max`, `feature_min`/`feature_max`, `degree`/
  `include_bias`/`interaction_only`).
- **`average` is a string, `zero_division` is 0 or 1.** scikit-learn also
  accepts `None`, `"samples"`, and `zero_division="warn"`/`np.nan`; the port
  supports `"binary"`/`"micro"`/`"macro"`/`"weighted"` and integer 0/1.
- **`log_loss` and `top_k_accuracy_score` take the multiclass 2D shape** (one
  row of per-class probabilities/scores per sample). The binary one-column
  spelling is expressed as its two-column equivalent.
- **`pos_label` is fixed to 1** for `roc_curve`, `precision_recall_curve`,
  `roc_auc_score` and `average_precision_score`; no `sample_weight`, no
  `labels`/`pos_label` selection.
- **No `normalize=False`, `multioutput`, `sample_weight`, `force_finite`
  toggle**, `drop_intermediate` toggle, or `drop`/`handle_unknown` for the
  encoder. The ported defaults are documented in each function.
- **`train_test_split` takes an explicit permutation** (the shuffled index
  order) instead of `random_state`; `KFold` and `StratifiedKFold` never
  shuffle.
- **Regression scores are single-output.** Multi-output averaging is out of
  scope.
- **`PolynomialFeatures`** exposes `polynomial_powers` (the fitted
  `powers_` matrix) and `polynomial_features_transform`; `degree` 2 and 3 are
  covered, with `include_bias` and `interaction_only`.

The suite asserts `312` values: 307 oracle-computed comparisons plus 5 record
construction checks. scikit-learn's tie-breaking is reproduced exactly
(stable descending score sorts for curves, highest-index-first for top-k,
appearance-order class encoding for StratifiedKFold).
