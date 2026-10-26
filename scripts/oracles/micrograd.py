"""Generates packages/micrograd/tests/expected.py from the real micrograd.

Runs in the development oracle venv (CPython + micrograd + torch), never in CI.
It ports micrograd's own test cases (test/test_engine.py) and the README demo,
computes every case with the real engine, and writes the values the Osprey
suite compares against. Regeneration is deterministic: the dataset, the seeds
and the traversal order are all fixed.

micrograd 0.1.0's Value has no tanh/exp, so this script adds the
implementations Karpathy wrote in the nn-zero-to-hero micrograd lectures
(lectures/micrograd/micrograd_lecture_second_half_roughly.ipynb) verbatim.
micrograd's `backward` iterates `v._prev`, a set, whose order follows object
hashes and so varies run to run; the patch below keeps the algorithm but walks
the deduplicated operands in operand order, which is exactly the order the
Osprey port's build_topo uses, so gradients are reproducible. All gradient
arithmetic still runs micrograd's own `_backward` closures.

Usage: $ORACLE scripts/oracles/micrograd.py   (from the repository root; writes
tests/expected.py through the shared writer in scripts/oracles/expected_module.py)
"""

import math
import os
import random
import sys
from importlib.metadata import version

from expected_module import write

import micrograd
import torch
from micrograd.engine import Value
from micrograd.nn import MLP

# --- Patches: creation registry, lecture tanh/exp, deterministic backward ---

CREATED = []
_ORIG_INIT = Value.__init__


def _init(self, data, _children=(), _op=""):
    """micrograd's __init__, plus the creation registry and the operand-ordered
    children that make traversal deterministic."""
    _ORIG_INIT(self, data, _children, _op)
    CREATED.append(self)
    ordered = []
    for child in _children:
        if child not in ordered:
            ordered.append(child)
    self._ordered_prev = ordered


Value.__init__ = _init


def _tanh(self):
    """The lecture's tanh, verbatim."""
    x = self.data
    t = (math.exp(2 * x) - 1) / (math.exp(2 * x) + 1)
    out = Value(t, (self,), "tanh")

    def _backward():
        self.grad += (1 - t**2) * out.grad

    out._backward = _backward
    return out


def _exp(self):
    """The lecture's exp, verbatim."""
    x = self.data
    out = Value(math.exp(x), (self,), "exp")

    def _backward():
        self.grad += out.data * out.grad

    out._backward = _backward
    return out


Value.tanh = _tanh
Value.exp = _exp


def _backward(self):
    """micrograd's backward, verbatim except that build_topo walks the
    operand-ordered children instead of the `_prev` set."""
    topo = []
    visited = set()

    def build_topo(v):
        if v not in visited:
            visited.add(v)
            for child in v._ordered_prev:
                build_topo(child)
            topo.append(v)

    build_topo(self)
    self.grad = 1
    for v in reversed(topo):
        v._backward()


Value.backward = _backward


# --- Case: the op battery (mirrored node for node by the port) ---

def run_case(inputs, build):
    """One op case: fresh Values, backward, [out.data] + the watched grads."""
    vals = [Value(x) for x in inputs]
    out = build(*vals)
    out.backward()
    return [float(out.data)] + [float(vals[i].grad) for i in range(len(vals))]


def diamond(a, b):
    """(a + b) * (a + b): one shared subexpression used twice."""
    z = a + b
    return z * z


OP_CASES = [
    ("op.add", [-4.0, 2.0], lambda a, b: a + b),
    ("op.add_const", [-4.0], lambda a: a + 2.0),
    ("op.radd", [-4.0], lambda a: 2.0 + a),
    ("op.mul", [-4.0, 2.0], lambda a, b: a * b),
    ("op.mul_const", [-4.0], lambda a: a * 2.0),
    ("op.rmul", [-4.0], lambda a: 2.0 * a),
    ("op.pow2", [-4.0], lambda a: a**2),
    ("op.pow3", [2.0], lambda b: b**3),
    ("op.pow1", [-4.0], lambda a: a**1),
    ("op.pow0", [-4.0], lambda a: a**0),
    ("op.pow_neg1", [2.0], lambda b: b**-1),
    ("op.pow_neg2", [-4.0], lambda a: a**-2),
    ("op.relu_neg", [-4.0], lambda a: a.relu()),
    ("op.relu_pos", [2.0], lambda a: a.relu()),
    ("op.relu_zero", [0.0], lambda a: a.relu()),
    ("op.relu_negzero", [-0.0], lambda a: a.relu()),
    ("op.neg", [-4.0], lambda a: -a),
    ("op.sub", [-4.0, 2.0], lambda a, b: a - b),
    ("op.rsub", [-4.0], lambda a: 2.0 - a),
    ("op.truediv", [-4.0, 2.0], lambda a, b: a / b),
    ("op.truediv_const", [-4.0], lambda a: a / 2.0),
    ("op.rtruediv", [-4.0], lambda a: 10.0 / a),
    ("op.self_add", [3.0], lambda x: x + x),
    ("op.self_mul", [3.0], lambda x: x * x),
    ("op.diamond", [-4.0, 2.0], diamond),
    ("op.reuse", [-4.0, 2.0], lambda a, b: a * b + a),
    ("op.chain", [-4.0, 2.0], lambda a, b: 2.0 * a + b * 3.0),
    ("op.large", [1e150, 1e10], lambda a, b: a * b),
    ("op.small", [1e-150, 1e-10], lambda a, b: a * b),
    ("op.tanh_neg", [-4.0], lambda a: a.tanh()),
    ("op.tanh_pos", [2.0], lambda a: a.tanh()),
    ("op.tanh_zero", [0.0], lambda a: a.tanh()),
    ("op.exp_neg", [-4.0], lambda a: a.exp()),
    ("op.exp_pos", [2.0], lambda a: a.exp()),
    ("op.exp_zero", [0.0], lambda a: a.exp()),
    ("op.tanh_mul", [-4.0, 2.0], lambda a, b: a.tanh() * b),
    ("op.exp_add", [-4.0, 2.0], lambda a, b: b.exp() + a),
]
# Everything the tanh/exp ops touch compares at 1e-9; the rest is exact
# arithmetic and compares at 1e-12 ([PORT-TEST-ORACLE]).
OP_RTOL = [1e-9 if "tanh" in name or "exp" in name else 1e-12 for name, _, _ in OP_CASES]


# --- Case: test_engine.py's test_sanity_check and test_more_ops ---

def sanity_case():
    """test_sanity_check's micrograd side, node for node, with torch's verdict."""
    x = Value(-4.0)
    z = 2 * x + 2 + x
    q = z.relu() + z * x
    h = (z * z).relu()
    y = h + q + q * x
    y.backward()

    xpt = torch.Tensor([-4.0]).double()
    xpt.requires_grad = True
    zpt = 2 * xpt + 2 + xpt
    qpt = zpt.relu() + zpt * xpt
    hpt = (zpt * zpt).relu()
    ypt = hpt + qpt + qpt * xpt
    ypt.backward()
    assert y.data == ypt.data.item(), (y.data, ypt.data.item())
    assert x.grad == xpt.grad.item(), (x.grad, xpt.grad.item())
    return [float(v.data) for v in CREATED], [float(v.grad) for v in CREATED], [float(ypt.data.item()), float(xpt.grad.item())]


def more_case():
    """test_more_ops's micrograd side, node for node, with torch's verdict."""
    a = Value(-4.0)
    b = Value(2.0)
    c = a + b
    d = a * b + b**3
    c += c + 1
    c += 1 + c + (-a)
    d += d * 2 + (b + a).relu()
    d += 3 * d + (b - a).relu()
    e = c - d
    f = e**2
    g = f / 2.0
    g += 10.0 / f
    g.backward()

    apt = torch.Tensor([-4.0]).double()
    bpt = torch.Tensor([2.0]).double()
    apt.requires_grad = True
    bpt.requires_grad = True
    cpt = apt + bpt
    dpt = apt * bpt + bpt**3
    cpt = cpt + cpt + 1
    cpt = cpt + 1 + cpt + (-apt)
    dpt = dpt + dpt * 2 + (bpt + apt).relu()
    dpt = dpt + 3 * dpt + (bpt - apt).relu()
    ept = cpt - dpt
    fpt = ept**2
    gpt = fpt / 2.0
    gpt = gpt + 10.0 / fpt
    gpt.backward()
    tol = 1e-6
    assert abs(g.data - gpt.data.item()) < tol, (g.data, gpt.data.item())
    assert abs(a.grad - apt.grad.item()) < tol, (a.grad, apt.grad.item())
    assert abs(b.grad - bpt.grad.item()) < tol, (b.grad, bpt.grad.item())
    return [float(v.data) for v in CREATED], [float(v.grad) for v in CREATED], [float(gpt.data.item()), float(apt.grad.item()), float(bpt.grad.item())]


# --- Case: the seeded MLP(3, [4, 4, 1]) forward ---

def mlp_forward_case():
    """MLP(3, [4, 4, 1]) seeded with random.seed(1337), forward and backward
    over the README's example input; every created Value's data and grad."""
    random.seed(1337)
    n = MLP(3, [4, 4, 1])
    weights = [float(p.data) for p in n.parameters()]
    ypred = n([2.0, 3.0, -1.0])
    ypred.backward()
    return weights, [float(v.data) for v in CREATED], [float(v.grad) for v in CREATED]


# --- Case: the README demo's SGD training ---

DATASET_X = [[2.0, 3.0, -1.0], [3.0, -1.0, 0.5], [0.5, 1.0, 1.0], [1.0, 1.0, -1.0]]
DATASET_Y = [1.0, -1.0, -1.0, 1.0]


def demo_loss(n, xs, ys):
    """The README demo's loss: sum((yout - ygt)**2 ...)."""
    ypred = [n(xb) for xb in xs]
    return sum((yout - ygt) ** 2 for ygt, yout in zip(ys, ypred)), ypred


def readme_training_case():
    """20 README demo SGD steps: the loss before every update and once after,
    then the final parameters and predictions."""
    random.seed(1337)
    n = MLP(3, [4, 4, 1])
    losses = []
    for _ in range(20):
        loss, _ = demo_loss(n, DATASET_X, DATASET_Y)
        losses.append(float(loss.data))
        n.zero_grad()
        loss.backward()
        for p in n.parameters():
            p.data += -0.05 * p.grad
    loss, ypred = demo_loss(n, DATASET_X, DATASET_Y)
    losses.append(float(loss.data))
    return losses, [float(p.data) for p in n.parameters()], [float(y.data) for y in ypred]


# --- Case: a tiny tanh network, trained ---

TANH_W0 = [0.6, -0.4, 0.1, -0.2, 0.8, -0.3, 0.5, 0.7, 0.2]
TANH_X = [[0.5, -1.0], [-0.5, 0.8], [1.0, 0.3]]
TANH_Y = [1.0, -1.0, 0.5]


def tanh_neuron(ws, b, xs):
    """One tanh neuron: the bias plus each wi*xi, then tanh."""
    act = b
    for w, x in zip(ws, xs):
        act = act + w * x
    return act.tanh()


def tanh_net(W, x):
    """Two tanh neurons over two inputs, then one tanh output neuron."""
    h0 = tanh_neuron([W[0], W[1]], W[2], x)
    h1 = tanh_neuron([W[3], W[4]], W[5], x)
    return tanh_neuron([W[6], W[7]], W[8], [h0, h1])


def tanh_training_case():
    """12 SGD steps of the tiny tanh network: the loss and parameter gradients
    before training, the loss before every update and once after, and the final
    parameters."""
    W = [Value(w) for w in TANH_W0]
    outs = [tanh_net(W, x) for x in TANH_X]
    loss = sum((o - y) ** 2 for y, o in zip(TANH_Y, outs))
    loss.backward()
    init_loss = float(loss.data)
    init_grads = [float(w.grad) for w in W]

    losses = []
    for _ in range(12):
        outs = [tanh_net(W, x) for x in TANH_X]
        loss = sum((o - y) ** 2 for y, o in zip(TANH_Y, outs))
        losses.append(float(loss.data))
        for w in W:
            w.grad = 0
        loss.backward()
        for w in W:
            w.data += -0.1 * w.grad
    outs = [tanh_net(W, x) for x in TANH_X]
    loss = sum((o - y) ** 2 for y, o in zip(TANH_Y, outs))
    losses.append(float(loss.data))
    return init_loss, init_grads, losses, [float(w.data) for w in W]


# --- Case: backward twice without zero_grad ---

def twice_case():
    """y = a*b + a: backward once, then again without zero_grad."""
    a = Value(-4.0)
    b = Value(2.0)
    y = a * b + a
    y.backward()
    once = [float(a.grad), float(b.grad)]
    y.backward()
    return once, [float(a.grad), float(b.grad)]


# --- Emit expected.py ---

def main():
    op_want = [run_case(inputs, build) for _, inputs, build in OP_CASES]

    CREATED.clear()
    sanity_data, sanity_grads, sanity_torch = sanity_case()
    CREATED.clear()
    more_data, more_grads, more_torch = more_case()
    CREATED.clear()
    mlp_weights, mlp_data, mlp_grads = mlp_forward_case()
    CREATED.clear()
    train_losses, train_params, train_preds = readme_training_case()
    CREATED.clear()
    tanh_init_loss, tanh_init_grads, tanh_losses, tanh_w = tanh_training_case()
    CREATED.clear()
    twice_once, twice_twice = twice_case()

    summary = (
        "Values the real micrograd computed: every op's data and gradients, "
        "micrograd's own test_engine.py cases, an MLP(3,[4,4,1]) forward/backward, "
        "20 README-demo SGD steps, a 12-step tanh network and a double backward."
    )
    write("micrograd", summary, {"micrograd": version("micrograd"), "torch": torch.__version__}, {
        "OP_WANT": op_want, "OP_RTOL": OP_RTOL,
        "SANITY_DATA": sanity_data, "SANITY_GRADS": sanity_grads, "SANITY_TORCH": sanity_torch,
        "MORE_DATA": more_data, "MORE_GRADS": more_grads, "MORE_TORCH": more_torch,
        "MLP_WEIGHTS": mlp_weights, "MLP_FORWARD_DATA": mlp_data, "MLP_FORWARD_GRADS": mlp_grads,
        "DATASET_X": DATASET_X, "DATASET_Y": DATASET_Y,
        "TRAIN_LOSSES": train_losses, "TRAIN_PARAMS": train_params, "TRAIN_PREDS": train_preds,
        "TANH_W0": TANH_W0, "TANH_X": TANH_X, "TANH_Y": TANH_Y,
        "TANH_INIT_LOSS": tanh_init_loss, "TANH_INIT_GRADS": tanh_init_grads,
        "TANH_LOSSES": tanh_losses, "TANH_W": tanh_w,
        "TWICE_ONCE": twice_once, "TWICE_TWICE": twice_twice,
    })
    print("wrote packages/micrograd/tests/expected.py")


main()
