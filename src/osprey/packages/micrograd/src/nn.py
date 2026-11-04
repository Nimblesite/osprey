# osprey: flavor=python
# osprey: namespace=micrograd
# osprey: module=NN
"""micrograd's neural network library over the graph arena.

`Neuron`, `Layer` and `MLP` are records of node ids into a graph built by
`init_mlp`, which appends parameter leaves in exactly micrograd's
`parameters()` order — per layer, per neuron, the weights then the bias — so a
fresh graph's first ids are those parameters. `call_neuron`, `call_layer` and
`call_mlp` are micrograd's `__call__`s: the activation starts at the bias and
each `wi * xi` is folded on (a leaf and a `*` node for a bare number input, a
bare `*` node for a Value input), then `relu` when the neuron is nonlinear.
`sgd_step` is the README demo's update loop as a function: it returns the new
parameter list rather than mutating `p.data`."""

from micrograd.Engine import Cursor, Graph, add, mul, mul_const, relu, top, value

__all__ = [
    "Neuron", "Layer", "MLP", "Forward", "param_count", "init_mlp", "neuron_at", "layer_at",
    "mlp_of", "call_neuron", "call_neuron_values", "call_layer", "call_layer_values",
    "call_mlp", "parameters", "sgd_step",
]


class Neuron:
    """micrograd's `nn.Neuron`: its weight node ids, its bias node id and its
    nonlin flag."""
    weights: list[int]
    bias: int
    nonlin: bool


class Layer:
    """micrograd's `nn.Layer`: its neurons, in construction order."""
    neurons: list[Neuron]


class MLP:
    """micrograd's `nn.MLP`: its layers, in construction order."""
    layers: list[Layer]


class Forward:
    """An MLP forward pass in progress: the graph and the latest layer's (or
    point's) output ids."""
    graph: Graph
    outputs: list[int]


class Built:
    """MLP construction walking parameter ids: the next offset and the layers."""
    offset: int
    layers: list[Layer]


def no_neuron():
    """A neutral neuron for a missing list entry."""
    return Neuron(weights=[], bias=-1, nonlin=True)


def no_layer():
    """A neutral layer for a missing list entry."""
    return Layer(neurons=[])


def param_count(nin, nouts):
    """The length of micrograd's `MLP.parameters()`: per layer, per neuron, the
    weights then the bias."""
    sizes = listConcat([nin], nouts)
    return fold(range(len(nouts)), 0, lambda total, i: total + (unwrap_or(listGet(sizes, i), 0) + 1) * unwrap_or(listGet(nouts, i), 0))


def init_mlp(g, nin, nouts, params):
    """micrograd's `MLP(nin, nouts)` construction: parameter leaves in
    `parameters()` order, taking their data from `params`. Build on an empty
    graph so the ids stay equal to `parameters()`."""
    return fold(range(param_count(nin, nouts)), g, lambda acc, i: value(acc, unwrap_or(listGet(params, i), 0.0)))


def neuron_at(offset, nin, nonlin):
    """The `Neuron` whose weights start at parameter id `offset`."""
    return Neuron(weights=toList(map(range(nin), lambda i: offset + i)), bias=offset + nin, nonlin=nonlin)


def layer_at(offset, nin, nout, nonlin):
    """The `Layer` whose neurons start at parameter id `offset`, each consuming
    `nin + 1` ids."""
    return Layer(neurons=toList(map(range(nout), lambda j: neuron_at(offset + j * (nin + 1), nin, nonlin))))


def mlp_of(nin, nouts):
    """The `MLP` records over a graph built by `init_mlp`: layers in
    construction order, each layer nonlinear except the last, as micrograd's
    MLP builds them."""
    sizes = listConcat([nin], nouts)
    last = len(nouts) - 1
    walked = fold(range(len(nouts)), Built(offset=0, layers=[]), lambda st, i: Built(
        offset=st.offset + (unwrap_or(listGet(sizes, i), 0) + 1) * unwrap_or(listGet(nouts, i), 0),
        layers=listAppend(st.layers, layer_at(st.offset, unwrap_or(listGet(sizes, i), 0), unwrap_or(listGet(nouts, i), 0), i != last)),
    ))
    return MLP(layers=walked.layers)


def step_input(acc, w, x):
    """One input of a neuron: `wi * xi` (a leaf and a `*` node for a bare
    number) added onto the running activation."""
    g2 = mul_const(acc.graph, w, x)
    g3 = add(g2, acc.id, top(g2))
    return Cursor(graph=g3, id=top(g3))


def step_value(acc, w, x):
    """One Value input of a neuron: `wi * xi` (a bare `*` node) added onto the
    running activation."""
    g2 = mul(acc.graph, w, x)
    g3 = add(g2, acc.id, top(g2))
    return Cursor(graph=g3, id=top(g3))


def call_neuron(g, n, xs):
    """micrograd's `Neuron.__call__` with bare-number inputs: the activation is
    `sum(wi * xi, b)` — the bias first, each product a leaf and a `*` node —
    then `relu` when the neuron is nonlinear."""
    walked = fold(range(len(n.weights)), Cursor(graph=g, id=n.bias), lambda acc, i: step_input(acc, unwrap_or(listGet(n.weights, i), -1), unwrap_or(listGet(xs, i), 0.0)))
    return relu(walked.graph, walked.id) if n.nonlin else walked.graph


def call_neuron_values(g, n, x_ids):
    """micrograd's `Neuron.__call__` over Value inputs (node ids): the same
    accumulation without wrapping the inputs in leaves."""
    walked = fold(range(len(n.weights)), Cursor(graph=g, id=n.bias), lambda acc, i: step_value(acc, unwrap_or(listGet(n.weights, i), -1), unwrap_or(listGet(x_ids, i), -1)))
    return relu(walked.graph, walked.id) if n.nonlin else walked.graph


def layer_neuron_step(acc, n, xs):
    """One neuron over bare inputs: append its output id to the running list."""
    g2 = call_neuron(acc.graph, n, xs)
    return Forward(graph=g2, outputs=listAppend(acc.outputs, top(g2)))


def layer_neuron_values_step(acc, n, x_ids):
    """One neuron over Value inputs: append its output id to the running list."""
    g2 = call_neuron_values(acc.graph, n, x_ids)
    return Forward(graph=g2, outputs=listAppend(acc.outputs, top(g2)))


def call_layer(g, l, xs):
    """micrograd's `Layer.__call__` with bare-number inputs: each neuron over
    the same inputs, in construction order; returns the graph and output ids."""
    return fold(range(len(l.neurons)), Forward(graph=g, outputs=[]), lambda acc, j: layer_neuron_step(acc, unwrap_or(listGet(l.neurons, j), no_neuron()), xs))


def call_layer_values(g, l, x_ids):
    """micrograd's `Layer.__call__` over Value inputs: each neuron over the
    same output ids, in construction order; returns the graph and output ids."""
    return fold(range(len(l.neurons)), Forward(graph=g, outputs=[]), lambda acc, j: layer_neuron_values_step(acc, unwrap_or(listGet(l.neurons, j), no_neuron()), x_ids))


def call_mlp(g, m, xs):
    """micrograd's `MLP.__call__`: layer 0 over the bare-number inputs, then
    every later layer over the previous layer's output ids. With no layers the
    graph is returned unchanged."""
    first_layer = unwrap_or(listGet(m.layers, 0), no_layer())
    walked = fold(range(1, len(m.layers)), call_layer(g, first_layer, xs), lambda st, i: call_layer_values(st.graph, unwrap_or(listGet(m.layers, i), no_layer()), st.outputs))
    return walked.graph


def layer_params(l):
    """One layer's parameters in micrograd's `Layer.parameters()` order."""
    return foldList(l.neurons, [], lambda acc, n: listConcat(acc, listAppend(n.weights, n.bias)))


def parameters(m):
    """micrograd's `MLP.parameters()`: every neuron's weights then bias, layer
    by layer — on a fresh graph, the ids 0..param_count-1 in order."""
    return foldList(m.layers, [], lambda acc, l: listConcat(acc, layer_params(l)))


def sgd_step(params: list[float], grads: list[float], ids: list[int], lr: float) -> list[float]:
    """The README demo's update as a function: `p.data += -lr * p.grad` for
    every parameter, returning the new parameter list. `grads` is indexed by
    node id; `ids` selects each parameter's gradient."""
    return toList(map(range(len(params)), lambda i: unwrap_or(listGet(params, i), 0.0) + (-lr) * unwrap_or(listGet(grads, unwrap_or(listGet(ids, i), 0)), 0.0)))
