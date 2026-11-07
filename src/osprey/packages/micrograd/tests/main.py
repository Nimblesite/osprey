# osprey: flavor=python
"""The @osprey/micrograd suite: 147 op cases (the original 37 plus 110
permutations over ReLU signs, powers, reused nodes, branching and long mixed
graphs), micrograd's own test_engine.py cases, two MLP forward/backwards, the
README demo's 20 SGD steps with per-step gradients, a tiny tanh network, a
double backward and six repeated-backward cases, each computed node for node
against src/osprey/packages/micrograd/oracle/micrograd.cpython's real-micrograd values.
Float comparisons use numpy's isclose rule at rtol 1e-12 (1e-9 for tanh/exp
cases), atol 0."""

from micrograd import Engine, Libm, NN
from micrograd_expected import Expected
from micrograd_tests import Ops
from testing import Check


class Loss:
    """A loss graph, its value and the gradients backward produced for it."""
    graph: Engine.Graph
    value: float
    grads: list[float]


def node_datas(graph):
    """Every node's data, numbered."""
    return toList(map(range(Engine.size(graph)), lambda i: Engine.data(graph, i)))


def graph_cases(prefix, datas, grads, want_datas, want_grads, rtol):
    """Data then gradient `list_cases` for one graph's nodes under `prefix`."""
    return listConcat(Ops.list_cases(f"{prefix}.data", datas, want_datas, rtol),
                      Ops.list_cases(f"{prefix}.grads", grads, want_grads, rtol))


def node_cases(graph, prefix, want_datas, want_grads, rtol):
    """Every node's data and gradient of `graph`, backward from its top node."""
    return graph_cases(prefix, node_datas(graph), Engine.backward(graph, Engine.top(graph)), want_datas, want_grads, rtol)


def trained(graph, outs, ys, n):
    """The loss `graph` reaches over `n` points, its gradients and value."""
    loss = sum_squares(graph, outs, ys, n)
    grads = Engine.backward(loss, Engine.top(loss))
    return Loss(graph=loss, value=Engine.data(loss, Engine.top(loss)), grads=grads)


def seeded(nin, nouts, params, xs):
    """A model holding `params` in a fresh graph, called on `xs`."""
    g = NN.init_mlp(Engine.empty(), nin, nouts, params)
    return NN.call_mlp(g, NN.mlp_of(nin, nouts), xs)


def layer_of(m, i):
    """Layer `i` of a model, or an empty one when the model is shorter."""
    return unwrap_or(listGet(m.layers, i), NN.Layer(neurons=[]))


def neuron_of(layer, i):
    """Neuron `i` of a layer, or an empty one when the layer is shorter."""
    return unwrap_or(listGet(layer.neurons, i), NN.Neuron(weights=[], bias=-1, nonlin=True))


def demo_mlp():
    """The demo MLP(3, [4, 4, 1]) and its first two layers/neurons as records."""
    m = NN.mlp_of(3, [4, 4, 1])
    layer0 = layer_of(m, 0)
    layer1 = layer_of(m, 1)
    return DemoMlp(m=m, layer0=layer0, n0=neuron_of(layer0, 0), layer1=layer1, n1=neuron_of(layer1, 0))


def seeded_mlp():
    """A fresh MLP(3, [4, 4, 1]) graph seeded with the oracle's weights."""
    return NN.init_mlp(Engine.empty(), 3, [4, 4, 1], Expected.MLP_WEIGHTS)


def demo_layer_pass(dm):
    """A seeded MLP's first `call_layer` pass over the oracle's inputs."""
    return NN.call_layer(seeded_mlp(), dm.layer0, ORACLE_XS)


class ForwardAll:
    """A finished forward pass: the graph and the ids this step hands on."""
    graph: Engine.Graph
    outs: list[int]


class TrainState:
    """The README demo's loop state: parameters, losses and per-step gradients."""
    params: list[float]
    losses: list[float]
    grads: list[list[float]]


class TanhState:
    """The tanh demo's loop state: parameters and the losses seen so far."""
    params: list[float]
    losses: list[float]


class TanhInit:
    """The untrained tanh net's loss and per-parameter gradients."""
    loss: float
    grads: list[float]


ORACLE_XS = [2.0, 3.0, -1.0]
"""The oracle's MLP demo inputs, shared by the single-pass NN cases."""


class DemoMlp:
    """The demo MLP(3, [4, 4, 1]) with its first two layers and neurons."""
    m: NN.MLP
    layer0: NN.Layer
    n0: NN.Neuron
    layer1: NN.Layer
    n1: NN.Neuron


class MoreState:
    """test_more_ops in progress: the graph and the live node ids."""
    graph: Engine.Graph
    aid: int
    bid: int
    cid: int
    did: int


def arena_cases():
    """Arena accessors and records: empty/size/top/value/node/data/ipow/zero_grad."""
    g = Engine.value(Engine.empty(), 3.0)
    nd = Engine.Node(op="", a=-1, b=-1, data=3.0, exponent=0)
    return [
        Check.equal("arena.empty.size", Engine.size(Engine.empty()), 0),
        Check.equal("arena.top", Engine.top(g), 0),
        Check.close("arena.value.data", Engine.data(g, 0), 3.0, 1e-12, 0.0),
        Check.truth("arena.node.op", Engine.node(g, 0).op == "", "node op"),
        Check.equal("arena.node.a", Engine.node(g, 0).a, -1),
        Check.close("arena.node.data", Engine.node(g, 0).data, 3.0, 1e-12, 0.0),
        Check.truth("arena.Node.op", nd.op == "", "Node op"),
        Check.close("arena.Node.data", nd.data, 3.0, 1e-12, 0.0),
        Check.close("arena.ipow.pos", Engine.ipow(2.0, 3), 8.0, 1e-12, 0.0),
        Check.close("arena.ipow.neg", Engine.ipow(4.0, -1), 0.25, 1e-12, 0.0),
        Check.close("arena.ipow.zero", Engine.ipow(5.0, 0), 1.0, 1e-12, 0.0),
        Check.close_all("arena.zero_grad", Engine.zero_grad(g), [0.0], 1e-12, 0.0),
    ]


def nn_records_cases():
    """NN's records and construction helpers: Neuron/Layer/MLP/Forward/neuron_at/layer_at."""
    na = NN.neuron_at(0, 3, True)
    la = NN.layer_at(16, 4, 4, True)
    first_of_layer = neuron_of(la, 0)
    dm = demo_mlp()
    return [
        Check.equal("nn.param_count.matches", NN.param_count(3, [4, 4, 1]), len(Expected.MLP_WEIGHTS)),
        Check.ints_equal("nn.neuron_at.weights", na.weights, [0, 1, 2]),
        Check.equal("nn.neuron_at.bias", na.bias, 3),
        Check.truth("nn.neuron_at.nonlin", na.nonlin, "nonlin flag"),
        Check.equal("nn.layer_at.count", len(la.neurons), 4),
        Check.ints_equal("nn.layer_at.weights", first_of_layer.weights, [16, 17, 18, 19]),
        Check.equal("nn.layer_at.bias", first_of_layer.bias, 20),
        Check.ints_equal("nn.Neuron.weights", dm.n0.weights, [0, 1, 2]),
        Check.equal("nn.Neuron.bias", dm.n0.bias, 3),
        Check.equal("nn.Layer.count", len(dm.layer0.neurons), 4),
    ]


def nn_record_cases():
    """The bare MLP and Forward records; a model with no layers adds no nodes."""
    empty = Engine.empty()
    return [
        Check.equal("nn.MLP.layers", len(NN.MLP(layers=[]).layers), 0),
        Check.equal("nn.MLP.no_layers", Engine.size(NN.call_mlp(empty, NN.MLP(layers=[]), [2.0, 3.0, -1.0])), Engine.size(empty)),
        Check.ints_equal("nn.Forward.outputs", NN.Forward(graph=empty, outputs=[0, 1]).outputs, [0, 1]),
    ]


def nn_neuron_cases():
    """`call_neuron`/`call_neuron_values` outputs match the oracle's nodes."""
    dm = demo_mlp()
    neuron = NN.call_neuron(seeded_mlp(), dm.n0, ORACLE_XS)
    layer0 = demo_layer_pass(dm)
    neuron_values = NN.call_neuron_values(layer0.graph, dm.n1, layer0.outputs)
    return [
        Check.close("nn.call_neuron", Engine.data(neuron, Engine.top(neuron)), unwrap_or(listGet(Expected.MLP_FORWARD_DATA, 50), 0.0), 1e-12, 0.0),
        Check.close("nn.call_neuron_values", Engine.data(neuron_values, Engine.top(neuron_values)), unwrap_or(listGet(Expected.MLP_FORWARD_DATA, 89), 0.0), 1e-12, 0.0),
    ]


def nn_layer_cases():
    """`call_layer`/`call_layer_values` output ids match the oracle's layout."""
    dm = demo_mlp()
    layer = demo_layer_pass(dm)
    layer_values = NN.call_layer_values(layer.graph, dm.layer1, layer.outputs)
    return [
        Check.ints_equal("nn.call_layer.outputs", layer.outputs, [50, 60, 70, 80]),
        Check.close("nn.call_layer.data", Engine.data(layer.graph, unwrap_or(listGet(layer.outputs, 3), 0)), unwrap_or(listGet(Expected.MLP_FORWARD_DATA, 80), 0.0), 1e-12, 0.0),
        Check.ints_equal("nn.call_layer_values.outputs", layer_values.outputs, [89, 98, 107, 116]),
        Check.close("nn.call_layer_values.data", Engine.data(layer_values.graph, unwrap_or(listGet(layer_values.outputs, 0), 0)), unwrap_or(listGet(Expected.MLP_FORWARD_DATA, 89), 0.0), 1e-12, 0.0),
    ]


def libm_cases():
    """Libm's exp_of/tanh_of against the oracle's exp/tanh op results."""
    return [
        Check.close("libm.tanh_of.neg", Libm.tanh_of(-4.0), unwrap_or(listGet(unwrap_or(listGet(Expected.OP_WANT, 29), []), 0), 0.0), 1e-9, 0.0),
        Check.close("libm.tanh_of.zero", Libm.tanh_of(0.0), unwrap_or(listGet(unwrap_or(listGet(Expected.OP_WANT, 31), []), 0), 0.0), 1e-9, 0.0),
        Check.close("libm.exp_of.pos", Libm.exp_of(2.0), unwrap_or(listGet(unwrap_or(listGet(Expected.OP_WANT, 33), []), 0), 0.0), 1e-9, 0.0),
        Check.close("libm.exp_of.zero", Libm.exp_of(0.0), unwrap_or(listGet(unwrap_or(listGet(Expected.OP_WANT, 34), []), 0), 0.0), 1e-9, 0.0),
    ]


def torch_case(name, got, want, i, rtol):
    """One `Check.close` against element `i` of an oracle's torch verdict."""
    return Check.close(name, got, unwrap_or(listGet(want, i), 0.0), rtol, 0.0)


def sanity_prefix(g):
    """`x = -4; z = 2*x + 2 + x` over a fresh graph."""
    x = Engine.value(g, -4.0)
    xid = Engine.top(x)
    z1 = Engine.mul_const(x, xid, 2.0)
    z2 = Engine.add_const(z1, Engine.top(z1), 2.0)
    z = Engine.add(z2, Engine.top(z2), xid)
    return ForwardAll(graph=z, outs=[xid, Engine.top(z)])


def sanity_suffix(g, xid, zid):
    """`q = z.relu() + z*x; h = (z*z).relu(); y = h + q + q*x`."""
    r = Engine.relu(g, zid)
    m = Engine.mul(r, zid, xid)
    q = Engine.add(m, Engine.top(r), Engine.top(m))
    qid = Engine.top(q)
    zz = Engine.mul(q, zid, zid)
    h = Engine.relu(zz, Engine.top(zz))
    hid = Engine.top(h)
    hq = Engine.add(h, hid, qid)
    qx = Engine.mul(hq, qid, xid)
    y = Engine.add(qx, Engine.top(hq), Engine.top(qx))
    return ForwardAll(graph=y, outs=[xid, Engine.top(y)])


def sanity_cases():
    """test_engine.py's test_sanity_check, node for node, plus torch's verdict."""
    pre = sanity_prefix(Engine.empty())
    zid = unwrap_or(listGet(pre.outs, 1), 0)
    xid = unwrap_or(listGet(pre.outs, 0), 0)
    done = sanity_suffix(pre.graph, xid, zid)
    y = done.graph
    yid = unwrap_or(listGet(done.outs, 1), 0)
    grads = Engine.backward(y, yid)
    return listConcat(graph_cases("sanity", node_datas(y), grads, Expected.SANITY_DATA, Expected.SANITY_GRADS, 1e-12), [
        torch_case("sanity.torch.data", Engine.data(y, yid), Expected.SANITY_TORCH, 0, 1e-12),
        torch_case("sanity.torch.grad", unwrap_or(listGet(grads, xid), 0.0), Expected.SANITY_TORCH, 1, 1e-12),
    ])


def more_init():
    """`c = a + b; d = a*b + b**3` over a fresh graph."""
    g = Engine.empty()
    a = Engine.value(g, -4.0)
    aid = Engine.top(a)
    b = Engine.value(a, 2.0)
    bid = Engine.top(b)
    c0 = Engine.add(b, aid, bid)
    cid0 = Engine.top(c0)
    ab = Engine.mul(c0, aid, bid)
    b3 = Engine.pow_const(ab, bid, 3)
    d0 = Engine.add(b3, Engine.top(ab), Engine.top(b3))
    return MoreState(graph=d0, aid=aid, bid=bid, cid=cid0, did=Engine.top(d0))


def more_c_ops(st):
    """`c += c + 1; c += 1 + c + (-a)`."""
    c1t = Engine.add_const(st.graph, st.cid, 1.0)
    c1 = Engine.add(c1t, st.cid, Engine.top(c1t))
    cid1 = Engine.top(c1)
    t1 = Engine.add_const(c1, cid1, 1.0)
    na = Engine.neg(t1, st.aid)
    t3 = Engine.add(na, Engine.top(t1), Engine.top(na))
    c2 = Engine.add(t3, cid1, Engine.top(t3))
    return MoreState(graph=c2, aid=st.aid, bid=st.bid, cid=Engine.top(c2), did=st.did)


def more_d_ops(st):
    """`d += d*2 + (b+a).relu(); d += 3*d + (b-a).relu()`."""
    d2m = Engine.mul_const(st.graph, st.did, 2.0)
    ba = Engine.add(d2m, st.bid, st.aid)
    ba_r = Engine.relu(ba, Engine.top(ba))
    d34 = Engine.add(ba_r, Engine.top(d2m), Engine.top(ba_r))
    d1 = Engine.add(d34, st.did, Engine.top(d34))
    did1 = Engine.top(d1)
    d3m = Engine.mul_const(d1, did1, 3.0)
    bma = Engine.sub(d3m, st.bid, st.aid)
    bma_r = Engine.relu(bma, Engine.top(bma))
    d56 = Engine.add(bma_r, Engine.top(d3m), Engine.top(bma_r))
    d2 = Engine.add(d56, did1, Engine.top(d56))
    return MoreState(graph=d2, aid=st.aid, bid=st.bid, cid=st.cid, did=Engine.top(d2))


def more_tail(st):
    """`e = c - d; f = e**2; g = f/2; g += 10/f`."""
    e = Engine.sub(st.graph, st.cid, st.did)
    eid = Engine.top(e)
    f = Engine.pow_const(e, eid, 2)
    fid = Engine.top(f)
    g2 = Engine.truediv_const(f, fid, 2.0)
    gid = Engine.top(g2)
    g3 = Engine.rtruediv_const(g2, fid, 10.0)
    gfinal = Engine.add(g3, gid, Engine.top(g3))
    return ForwardAll(graph=gfinal, outs=[st.aid, st.bid, Engine.top(gfinal)])


def more_cases():
    """test_engine.py's test_more_ops, node for node, plus torch's verdict."""
    done = more_tail(more_d_ops(more_c_ops(more_init())))
    g = done.graph
    gid2 = unwrap_or(listGet(done.outs, 2), 0)
    aid = unwrap_or(listGet(done.outs, 0), 0)
    bid = unwrap_or(listGet(done.outs, 1), 0)
    grads = Engine.backward(g, gid2)
    return listConcat(graph_cases("more", node_datas(g), grads, Expected.MORE_DATA, Expected.MORE_GRADS, 1e-12), [
        torch_case("more.torch.data", Engine.data(g, gid2), Expected.MORE_TORCH, 0, 1e-6),
        torch_case("more.torch.a", unwrap_or(listGet(grads, aid), 0.0), Expected.MORE_TORCH, 1, 1e-6),
        torch_case("more.torch.b", unwrap_or(listGet(grads, bid), 0.0), Expected.MORE_TORCH, 2, 1e-6),
    ])


def mlp_cases():
    """MLP(3, [4, 4, 1]) with the oracle's seeded weights: forward and backward."""
    return node_cases(seeded(3, [4, 4, 1], Expected.MLP_WEIGHTS, ORACLE_XS), "mlp", Expected.MLP_FORWARD_DATA, Expected.MLP_FORWARD_GRADS, 1e-12)


def mlp2_cases():
    """MLP(2, [3, 2, 1]) with the oracle's second seeded weights."""
    return node_cases(seeded(2, [3, 2, 1], Expected.MLP2_WEIGHTS, [0.5, -1.5]), "mlp2", Expected.MLP2_FORWARD_DATA, Expected.MLP2_FORWARD_GRADS, 1e-12)


def chain_forward(acc, gnext):
    """Append one forward pass's graph and top node id to the running pass."""
    return ForwardAll(graph=gnext, outs=listAppend(acc.outs, Engine.top(gnext)))


def forward_all(g, m, xs):
    """The README demo's four forward passes over one shared parameter graph."""
    return fold(range(4), ForwardAll(graph=g, outs=[]), lambda acc, i: chain_forward(acc, NN.call_mlp(acc.graph, m, xi(xs, i))))


def loss_step(st, i, outs, ys):
    """One summand of the loss: (out - y)**2 added onto the running sum."""
    d = Engine.sub_const(st.graph, unwrap_or(listGet(outs, i), 0), unwrap_or(listGet(ys, i), 0.0))
    p = Engine.pow_const(d, Engine.top(d), 2)
    g = Engine.add(p, st.id, Engine.top(p))
    return Engine.Cursor(graph=g, id=Engine.top(g))


def sum_squares(g, outs, ys, n):
    """micrograd's `sum((o - y)**2 ...)`: `(((0 + v0) + v1) + ...)` over n points."""
    d0 = Engine.sub_const(g, unwrap_or(listGet(outs, 0), 0), unwrap_or(listGet(ys, 0), 0.0))
    p0 = Engine.pow_const(d0, Engine.top(d0), 2)
    acc = Engine.add_const(p0, Engine.top(p0), 0.0)
    final = fold(range(1, n), Engine.Cursor(graph=acc, id=Engine.top(acc)), lambda st, i: loss_step(st, i, outs, ys))
    return final.graph


def readme_pass(params):
    """The README demo's MLP, seeded from `params`, run over the dataset."""
    g = NN.init_mlp(Engine.empty(), 3, [4, 4, 1], params)
    m = NN.mlp_of(3, [4, 4, 1])
    return forward_all(g, m, Expected.DATASET_X)


def train_step(st, _):
    """One README demo SGD step: forward, loss, backward, parameter update."""
    f = readme_pass(st.params)
    loss = trained(f.graph, f.outs, Expected.DATASET_Y, 4)
    ids = NN.parameters(NN.mlp_of(3, [4, 4, 1]))
    param_grads = toList(map(range(len(ids)), lambda i: unwrap_or(listGet(loss.grads, unwrap_or(listGet(ids, i), 0)), 0.0)))
    new_params = NN.sgd_step(st.params, loss.grads, ids, 0.05)
    return TrainState(params=new_params, losses=listAppend(st.losses, loss.value), grads=listAppend(st.grads, param_grads))


def readme_cases():
    """20 README demo SGD steps: the per-step losses and gradients, final params and preds."""
    st = fold(range(20), TrainState(params=Expected.MLP_WEIGHTS, losses=[], grads=[]), train_step)
    f = readme_pass(st.params)
    losses = listAppend(st.losses, trained(f.graph, f.outs, Expected.DATASET_Y, 4).value)
    preds = toList(map(range(4), lambda i: Engine.data(f.graph, unwrap_or(listGet(f.outs, i), 0))))
    loss_cases = listConcat(listConcat(Ops.list_cases("train.losses", losses, Expected.TRAIN_LOSSES, 1e-12), Ops.list_cases("train.params", st.params, Expected.TRAIN_PARAMS, 1e-12)), Ops.list_cases("train.preds", preds, Expected.TRAIN_PREDS, 1e-12))
    return listConcat(loss_cases, Ops.nested_cases("train.step_grads", st.grads, Expected.TRAIN_STEP_GRADS, 1e-12))


def xi(xs, i):
    """Row i of a nested float list."""
    return unwrap_or(listGet(xs, i), [])


def xij(xs, i, j):
    """Entry (i, j) of a nested float list."""
    return unwrap_or(listGet(xi(xs, i), j), 0.0)


def tanh_neuron(g, w0, w1, b, x0, x1):
    """One tanh neuron over two bare-number inputs: `tanh(b + w0*x0 + w1*x1)`."""
    m0 = Engine.mul_const(g, w0, x0)
    a0 = Engine.add(m0, b, Engine.top(m0))
    m1 = Engine.mul_const(a0, w1, x1)
    a1 = Engine.add(m1, Engine.top(a0), Engine.top(m1))
    return Engine.tanh(a1, Engine.top(a1))


def tanh_neuron_values(g, w0, w1, b, x0, x1):
    """One tanh neuron over two Value inputs: `tanh(b + w0*x0 + w1*x1)`."""
    m0 = Engine.mul(g, w0, x0)
    a0 = Engine.add(m0, b, Engine.top(m0))
    m1 = Engine.mul(a0, w1, x1)
    a1 = Engine.add(m1, Engine.top(a0), Engine.top(m1))
    return Engine.tanh(a1, Engine.top(a1))


def tanh_net(g, W, x0, x1):
    """Two hidden tanh neurons over (x0, x1), then one tanh output neuron."""
    h0 = tanh_neuron(g, unwrap_or(listGet(W, 0), 0), unwrap_or(listGet(W, 1), 0), unwrap_or(listGet(W, 2), 0), x0, x1)
    hid0 = Engine.top(h0)
    h1 = tanh_neuron(h0, unwrap_or(listGet(W, 3), 0), unwrap_or(listGet(W, 4), 0), unwrap_or(listGet(W, 5), 0), x0, x1)
    hid1 = Engine.top(h1)
    return tanh_neuron_values(h1, unwrap_or(listGet(W, 6), 0), unwrap_or(listGet(W, 7), 0), unwrap_or(listGet(W, 8), 0), hid0, hid1)


def tanh_weights(params):
    """Nine parameter leaves, ids 0..8."""
    return fold(range(9), Engine.empty(), lambda acc, i: Engine.value(acc, unwrap_or(listGet(params, i), 0.0)))


def tanh_param_ids():
    """The tanh demo's nine parameter node ids, 0..8."""
    return toList(map(range(9), lambda i: i))


def tanh_forward(g, W, xs):
    """Three tanh-net forward passes over the shared nine-parameter graph."""
    return fold(range(3), ForwardAll(graph=g, outs=[]), lambda acc, i: chain_forward(acc, tanh_net(acc.graph, W, xij(xs, i, 0), xij(xs, i, 1))))


def tanh_run(params):
    """A fresh tanh net from `params`: forward/backward over the tanh dataset."""
    g = tanh_weights(params)
    f = tanh_forward(g, tanh_param_ids(), Expected.TANH_X)
    return trained(f.graph, f.outs, Expected.TANH_Y, 3)


def tanh_step(st, _):
    """One tanh-demo SGD step: forward, loss, backward, parameter update."""
    loss = tanh_run(st.params)
    new_params = NN.sgd_step(st.params, loss.grads, tanh_param_ids(), 0.1)
    return TanhState(params=new_params, losses=listAppend(st.losses, loss.value))


def tanh_init():
    """The untrained tanh net's loss and per-parameter gradients."""
    loss = tanh_run(Expected.TANH_W0)
    return TanhInit(loss=loss.value, grads=toList(map(range(9), lambda i: unwrap_or(listGet(loss.grads, i), 0.0))))


def tanh_cases():
    """The tiny tanh network: init loss/grads, 12 SGD steps, final params."""
    init = tanh_init()
    st = fold(range(12), TanhState(params=Expected.TANH_W0, losses=[]), tanh_step)
    losses = listAppend(st.losses, tanh_run(st.params).value)
    return listConcat(listConcat([Check.close("tanh.init_loss", init.loss, Expected.TANH_INIT_LOSS, 1e-9, 0.0)], Ops.list_cases("tanh.init_grads", init.grads, Expected.TANH_INIT_GRADS, 1e-9)), listConcat(Ops.list_cases("tanh.losses", losses, Expected.TANH_LOSSES, 1e-9), Ops.list_cases("tanh.w", st.params, Expected.TANH_W, 1e-9)))


def twice_cases():
    """`y = a*b + a`: backward once, then again without zero_grad."""
    g = Engine.empty()
    a = Engine.value(g, -4.0)
    aid = Engine.top(a)
    b = Engine.value(a, 2.0)
    bid = Engine.top(b)
    m = Engine.mul(b, aid, bid)
    y = Engine.add(m, Engine.top(m), aid)
    yid = Engine.top(y)
    once = Engine.backward(y, yid)
    twice = Engine.backward_into(y, yid, once)
    return listConcat(Ops.list_cases("twice.once", [unwrap_or(listGet(once, aid), 0.0), unwrap_or(listGet(once, bid), 0.0)], Expected.TWICE_ONCE, 1e-12), Ops.list_cases("twice.twice", [unwrap_or(listGet(twice, aid), 0.0), unwrap_or(listGet(twice, bid), 0.0)], Expected.TWICE_TWICE, 1e-12))


def all_cases():
    """Every suite case, in a fixed order for the golden."""
    ops = toList(map(range(Ops.OP_COUNT), Ops.op_case))
    core = listConcat(listConcat(listConcat(listConcat(listConcat(ops, arena_cases()), nn_records_cases()), nn_record_cases()), nn_neuron_cases()), nn_layer_cases())
    net = listConcat(listConcat(listConcat(listConcat(listConcat(libm_cases(), sanity_cases()), more_cases()), mlp_cases()), readme_cases()), tanh_cases())
    extra = listConcat(mlp2_cases(), Ops.repeat_cases())
    return listConcat(listConcat(core, net), listConcat(twice_cases(), extra))


def main():
    return Check.run(all_cases)
