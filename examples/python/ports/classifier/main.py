# osprey: flavor=python
"""Karpathy's micrograd demo as a runnable classifier, in Osprey's Python flavor.

The original notebook trains a 2->16->16->1 MLP with the max-margin loss plus L2
regularization on `make_moons`, then plots the decision boundary. This port keeps
the model, the hinge loss, the coefficient-1 accuracy check and the learning-rate
schedule; it replaces random `make_moons` with sixteen fixed XOR points, reports
metrics instead of plotting, and round-trips the trained weights through a
safetensors checkpoint. `micrograd` supplies the graph, forward pass, backward
pass and SGD step; `sklearn` supplies the split, scaler and metrics.
"""

from micrograd import Engine, NN
from sklearn import Metrics, ModelSelection, Preprocessing

from port_common import Checkpoint, Policy

HIDDEN = [8, 8, 1]
INPUTS = [-1.0, 1.0]
STEPS = 30
LEARNING_RATE = 1.0
DECAY = 0.9
ALPHA = 0.0001
TEST_SIZE = 4
SEED = 1337
LCG_A = 1103515245
LCG_C = 12345
LCG_M = 2147483648
SPAN = 2001
HALF_SPAN = 1000.0
SCALE = 2000.0
GRID_RADIUS = 4
GRID_STEP = 0.75
GRID_START = -3.0

DATA = [
    [-2.0, -2.0], [-2.25, -1.6], [-1.6, -2.2], [-1.75, -1.75],
    [2.0, 2.0], [2.2, 1.6], [1.6, 2.2], [1.8, 1.8],
    [-2.0, 2.0], [-2.2, 1.6], [-1.6, 2.2], [-1.8, 1.8],
    [2.0, -2.0], [2.2, -1.6], [1.6, -2.2], [1.8, -1.8],
]
LABELS = [-1, -1, -1, -1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1]
PERMUTATION = [7, 2, 13, 0, 9, 4, 11, 6, 15, 1, 8, 3, 14, 5, 10, 12]


class Prepared:
    """The split data and the scaler fitted on the training rows."""
    train_x: list[list[float]]
    train_y: list[int]
    test_x: list[list[float]]
    test_y: list[int]
    fitted: Preprocessing.StandardScalerFitted


class InitState:
    """A deterministic parameter draw in progress."""
    seed: int
    values: list[float]


class ForwardAll:
    """A finished forward pass: the graph and every point's output id."""
    graph: Engine.Graph
    ids: list[int]


class LossResult:
    """The scalar loss node and the graph that holds it."""
    graph: Engine.Graph
    id: int


class TrainState:
    """The parameters after each step and the loss each step measured."""
    params: list[float]
    losses: list[float]


class Report:
    """Everything the classifier prints and asserts about."""
    first_loss: float
    last_loss: float
    accuracy: float
    precision: float
    recall: float
    f1: float
    matrix: list[list[int]]
    params: list[float]
    checkpoint: list[float]
    checkpoint_size: int
    grid_positive: int
    grid_total: int


def rows_at(rows, indices):
    """The rows named by `indices`, in that order."""
    return toList(map(range(len(indices)), lambda i: unwrap_or(listGet(rows, unwrap_or(listGet(indices, i), 0)), [])))


def labels_at(labels, indices):
    """The labels named by `indices`, in that order."""
    return toList(map(range(len(indices)), lambda i: unwrap_or(listGet(labels, unwrap_or(listGet(indices, i), 0)), 0)))


def as_binary(labels):
    """Map micrograd's -1/1 labels to sklearn's 0/1 labels."""
    return mapList(labels, lambda y: 1 if y > 0 else 0)


def prepare():
    """Split deterministically, then fit the scaler on the training rows only."""
    split = ModelSelection.train_test_split(PERMUTATION, TEST_SIZE)
    train_x = rows_at(DATA, split.train)
    test_x = rows_at(DATA, split.test)
    scaler = Preprocessing.StandardScaler(with_mean=True, with_std=True)
    fitted = Preprocessing.standard_scaler_fit(scaler, train_x)
    return Prepared(
        train_x=Preprocessing.standard_scaler_transform(fitted, train_x),
        train_y=labels_at(LABELS, split.train),
        test_x=Preprocessing.standard_scaler_transform(fitted, test_x),
        test_y=labels_at(LABELS, split.test),
        fitted=fitted,
    )


def lcg_next(seed):
    """The next state of a small linear-congruential generator."""
    return (seed * LCG_A + LCG_C) % LCG_M


def draw_param(state, _):
    """One deterministic weight in [-0.5, 0.5]."""
    seed = lcg_next(state.seed)
    return InitState(seed=seed, values=listAppend(state.values, (toFloat(seed % SPAN) - HALF_SPAN) / SCALE))


def initial_params(count):
    """The replacement for the notebook's `random.seed(1337)` initialization."""
    return fold(range(count), InitState(seed=SEED, values=[]), draw_param).values


def forward_step(state, row):
    """One point through the MLP, appended to the shared graph."""
    graph = NN.call_mlp(state.graph, NN.mlp_of(len(INPUTS), HIDDEN), row)
    return ForwardAll(graph=graph, ids=listAppend(state.ids, Engine.top(graph)))


def forward_all(graph, xs):
    """Every point forward, without rebuilding the model records."""
    return fold(range(len(xs)), ForwardAll(graph=graph, ids=[]), lambda state, i: forward_step(state, unwrap_or(listGet(xs, i), [])))


def hinge_step(state, i, outs, ys):
    """One `(1 - y*score).relu()` term added to the running total."""
    score = unwrap_or(listGet(outs, i), 0)
    y = unwrap_or(listGet(ys, i), 1)
    product = Engine.mul_const(state.graph, score, toFloat(y))
    margin = Engine.rsub_const(product, Engine.top(product), 1.0)
    term = Engine.relu(margin, Engine.top(margin))
    added = Engine.add(term, state.id, Engine.top(term))
    return Engine.Cursor(graph=added, id=Engine.top(added))


def total_loss(graph, outs, ys, ids):
    """Mean hinge loss plus `ALPHA * sum(p*p)`, as the notebook computes it."""
    seed_graph = Engine.add_const(graph, unwrap_or(listGet(outs, 0), 0), 0.0)
    seed = Engine.Cursor(graph=seed_graph, id=Engine.top(seed_graph))
    summed = fold(range(len(outs)), seed, lambda state, i: hinge_step(state, i, outs, ys))
    mean = Engine.truediv_const(summed.graph, summed.id, toFloat(len(outs)))
    zero = Engine.value(mean, 0.0)
    reg_seed = Engine.Cursor(graph=zero, id=Engine.top(zero))
    reg = fold(range(len(ids)), reg_seed, lambda state, i: reg_add(state, i, ids))
    scaled = Engine.mul_const(reg.graph, reg.id, ALPHA)
    total = Engine.add(scaled, Engine.top(scaled), Engine.top(mean))
    return LossResult(graph=total, id=Engine.top(total))


def reg_add(state, i, ids):
    """Add one parameter square to the running regularization sum."""
    node = unwrap_or(listGet(ids, i), 0)
    square = Engine.mul(state.graph, node, node)
    added = Engine.add(square, state.id, Engine.top(square))
    return Engine.Cursor(graph=added, id=Engine.top(added))


def train_step_with(state, step, data):
    """The training step over already-prepared data."""
    graph = NN.init_mlp(Engine.empty(), len(INPUTS), HIDDEN, state.params)
    model = NN.mlp_of(len(INPUTS), HIDDEN)
    forward = forward_all(graph, data.train_x)
    loss = total_loss(forward.graph, forward.ids, data.train_y, NN.parameters(model))
    grads = Engine.backward(loss.graph, loss.id)
    lr = LEARNING_RATE - DECAY * toFloat(step) / toFloat(STEPS)
    return TrainState(params=NN.sgd_step(state.params, grads, NN.parameters(model), lr), losses=listAppend(state.losses, Engine.data(loss.graph, loss.id)))


def count_params(nin, nouts):
    """micrograd's parameter count, before any graph exists."""
    return NN.param_count(nin, nouts)


def predict_all(xs, params):
    """Binary predictions for every row, using the notebook's `score > 0` rule."""
    graph = NN.init_mlp(Engine.empty(), len(INPUTS), HIDDEN, params)
    forward = forward_all(graph, xs)
    return mapList(forward.ids, lambda node: 1 if Engine.data(forward.graph, node) > 0.0 else 0)


def grid_values():
    """The nine coordinates of the notebook's decision-boundary grid."""
    return toList(map(range(GRID_RADIUS * 2 + 1), lambda i: GRID_START + toFloat(i) * GRID_STEP))


def grid_rows():
    """The 81 grid points, in row-major order."""
    xs = grid_values()
    return fold(range(len(xs)), [], lambda acc, i: listConcat(acc, mapList(xs, lambda y: [unwrap_or(listGet(xs, i), 0.0), y])))


def report_of(data, trained):
    """Compute every reported number from the trained parameters."""
    test_predictions = predict_all(data.test_x, trained.params)
    test_labels = as_binary(data.test_y)
    checkpoint = Checkpoint.encode_f64("weights", trained.params)
    decoded = unwrap_or(Checkpoint.decode_f64(checkpoint, "weights"), [])
    grid = Preprocessing.standard_scaler_transform(data.fitted, grid_rows())
    grid_predictions = predict_all(grid, trained.params)
    return Report(
        first_loss=unwrap_or(listGet(trained.losses, 0), 0.0),
        last_loss=unwrap_or(listGet(trained.losses, len(trained.losses) - 1), 0.0),
        accuracy=Metrics.accuracy_score(test_labels, test_predictions),
        precision=Metrics.precision_score(test_labels, test_predictions, "binary", 0),
        recall=Metrics.recall_score(test_labels, test_predictions, "binary", 0),
        f1=Metrics.f1_score(test_labels, test_predictions, "binary", 0),
        matrix=Metrics.confusion_matrix(test_labels, test_predictions, [0, 1]),
        params=trained.params,
        checkpoint=decoded,
        checkpoint_size=len(checkpoint),
        grid_positive=foldList(grid_predictions, 0, lambda acc, p: acc + p),
        grid_total=len(grid_predictions),
    )


def fmt(value):
    """Four decimal places, stable across machines for these small values."""
    return toString(round(value * 10000.0) / 10000.0)


def fmt_matrix(matrix):
    """A compact one-line confusion matrix."""
    return join(mapList(matrix, lambda row: "[" + join(mapList(row, lambda value: toString(value)), ",") + "]"), " ")


def fmt_report(report):
    """One consolidated report, replacing the notebook's plot."""
    return join([
        "classifier: micrograd MLP 2->8->8->1, hinge loss + L2",
        "data: 16 fixed XOR points, train=12 test=4, standardised",
        f"params={len(report.params)} steps={STEPS} loss {fmt(report.first_loss)} -> {fmt(report.last_loss)}",
        f"test accuracy={fmt(report.accuracy)} precision={fmt(report.precision)} recall={fmt(report.recall)} f1={fmt(report.f1)}",
        "confusion=" + fmt_matrix(report.matrix),
        f"checkpoint: safetensors bytes={report.checkpoint_size} decoded={len(report.checkpoint)} exact={structuralEquals(report.checkpoint, report.params)}",
        f"decision grid: positive={report.grid_positive}/{report.grid_total}",
    ], "\n")


def main():
    with Policy.arithmetic:
        data = prepare()
        trained = fold(range(STEPS), TrainState(params=initial_params(count_params(len(INPUTS), HIDDEN)), losses=[]), lambda state, step: train_step_with(state, step, data))
        report = report_of(data, trained)
        print(fmt_report(report))
        checkAll("classifier", [
            report.first_loss > report.last_loss,
            report.accuracy >= 0.75,
            report.accuracy == 1.0,
            len(report.checkpoint) == len(report.params),
            structuralEquals(report.checkpoint, report.params),
            report.checkpoint_size > len(report.params) * 8,
            report.grid_total == 81,
            report.grid_positive > 0,
            report.grid_positive < report.grid_total,
            len(data.test_y) == TEST_SIZE,
        ])

