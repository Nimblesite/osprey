# osprey: flavor=python
# osprey: namespace=torch
# osprey: module=Autograd
"""torch.autograd as an explicit reverse-mode tape.

Values are immutable, so a differentiable computation appends one `Node` per
operation to a `Tape` and threads it through: each op reads its inputs' values
from the tape, computes the result, and returns the grown tape and the new node
id. `backward` walks the tape in reverse, seeding the loss node with a
cotangent, and accumulates each node's gradient as the sum of its consumers'
contributions, broadcasting every contribution back to the input's shape with
`Tensor.sum_to_size` — exactly torch's accumulation and broadcast-reduction
semantics. `grad` and `vjp` wrap a tape-style function `(tape, id) -> Step`.
"""
from typing import Callable

from numpy import Array, Linalg, Storage
from torch import Tensor

__all__ = [
    "Tape", "Node", "Grad", "Step", "VJP", "empty", "leaf", "value_of", "backward",
    "grad", "vjp", "add", "sub", "mul", "div", "neg", "exp", "log", "sqrt", "square",
    "tanh", "sigmoid", "relu", "leaky_relu", "gelu", "silu", "sum_all", "sum_axis",
    "mean_all", "mean_axis", "matmul", "softmax", "log_softmax",
]

INV_SQRT_TWO = 0.7071067811865476
INV_SQRT_TWO_PI = 0.3989422804014327


class Tape[B]:
    """The reverse-mode tape: one node per operation, in forward order."""
    nodes: list[Node[B]]


class Node[B]:
    """One tape node: its op tag, input ids, tensors its backward needs, the
    output shape and value, and the axis/param/keep extras reductions and
    parameterised activations use."""
    op: str
    inputs: list[int]
    saved: list[Array.NDArray[B]]
    shape: list[int]
    value: Array.NDArray[B]
    axis: int
    param: float
    keep: bool


class Grad[B]:
    """One backward contribution: to input `pos`, the gradient to accumulate."""
    pos: int
    value: Array.NDArray[B]


class Step[B]:
    """An op's result: the grown tape and the new node's id."""
    tape: Tape[B]
    out: int


class VJP[B]:
    """torch.func.vjp's result: the primal output and the pulled-back
    cotangent."""
    primal: Array.NDArray[B]
    cotangent: Array.NDArray[B]


def empty():
    """An empty tape."""
    return Tape(nodes=[])


def node_at(tape, i):
    """The node at id `i`; the fallback is never read by a valid graph."""
    return unwrap_or(listGet(tape.nodes, i), Node(op="", inputs=[], saved=[], shape=[0], value=Array.empty(), axis=0, param=0.0, keep=False))


def value_of(tape, i):
    """The forward value stored at node id `i`."""
    return node_at(tape, i).value


def append(tape, node):
    """Append `node`; its id is its position in the grown tape."""
    return Step(tape=Tape(nodes=listAppend(tape.nodes, node)), out=len(tape.nodes))


def leaf(tape, value):
    """A constant input node: no inputs, no backward contribution."""
    return append(tape, Node(op="leaf", inputs=[], saved=[], shape=value.shape, value=value, axis=0, param=0.0, keep=False))


def in_shape(tape, node, k):
    """The shape of input `k` of `node`."""
    return node_at(tape, unwrap_or(listGet(node.inputs, k), 0)).shape


def saved(node, k):
    """The `k`-th saved tensor of `node`."""
    return unwrap_or(listGet(node.saved, k), Array.empty())


def set_index(xs, i, v):
    """A copy of `xs` with element `i` replaced by `v` (the flavor has no
    list-element update)."""
    return toList(map(range(len(xs)), lambda k: v if k == i else unwrap_or(listGet(xs, k), v)))


def add_grad(grads, c):
    """Accumulate `c` into the gradient of its input: gradients sum over uses."""
    old = unwrap_or(listGet(grads, c.pos), Array.empty())
    return set_index(grads, c.pos, Array.add(old, c.value))


def backward(tape, loss_id, seed):
    """Reverse-mode over `tape`: seed the loss node with `seed` and accumulate
    every node's gradient from its consumers, in reverse order."""
    n = len(tape.nodes)
    init = toList(map(range(n), lambda i: seed if i == loss_id else Array.zeros_like(node_at(tape, i).value)))
    return fold(range(n), init, lambda acc, i: step_backward(tape, acc, n - 1 - i))


def step_backward(tape, grads, i):
    """One node's backward: push its gradient to each input."""
    node = node_at(tape, i)
    g = unwrap_or(listGet(grads, i), Array.empty())
    return foldList(op_backward(tape, node, g), grads, add_grad)


def op_backward(tape, node, g):
    """The per-op backward: a list of input contributions for `node`'s gradient
    `g`, each already broadcast-reduced to its input's shape."""
    match node.op:
        case "leaf":
            return []
        case "add":
            return [Grad(pos=input0(node), value=Tensor.sum_to_size(g, in_shape(tape, node, 0))), Grad(pos=input1(node), value=Tensor.sum_to_size(g, in_shape(tape, node, 1)))]
        case "sub":
            return [Grad(pos=input0(node), value=Tensor.sum_to_size(g, in_shape(tape, node, 0))), Grad(pos=input1(node), value=Tensor.sum_to_size(Array.negative(g), in_shape(tape, node, 1)))]
        case "mul":
            a = saved(node, 0)
            b = saved(node, 1)
            return [Grad(pos=input0(node), value=Tensor.sum_to_size(Tensor.bmul(g, b), in_shape(tape, node, 0))), Grad(pos=input1(node), value=Tensor.sum_to_size(Tensor.bmul(g, a), in_shape(tape, node, 1)))]
        case "div":
            a = saved(node, 0)
            b = saved(node, 1)
            return [Grad(pos=input0(node), value=Tensor.sum_to_size(Tensor.bdiv(g, b), in_shape(tape, node, 0))), Grad(pos=input1(node), value=Tensor.sum_to_size(Array.negative(Tensor.bdiv(Tensor.bmul(g, a), Array.multiply(b, b))), in_shape(tape, node, 1)))]
        case "neg":
            return [Grad(pos=input0(node), value=Array.negative(g))]
        case "exp":
            return [Grad(pos=input0(node), value=Array.multiply(g, node.value))]
        case "log":
            return [Grad(pos=input0(node), value=Array.divide(g, saved(node, 0)))]
        case "sqrt":
            return [Grad(pos=input0(node), value=Array.divide(g, Tensor.scale(node.value, 2.0)))]
        case "square":
            return [Grad(pos=input0(node), value=Array.multiply(Tensor.scale(g, 2.0), saved(node, 0)))]
        case "tanh":
            return [Grad(pos=input0(node), value=Array.multiply(g, Tensor.sub_from(1.0, Array.multiply(node.value, node.value))))]
        case "sigmoid":
            return [Grad(pos=input0(node), value=Array.multiply(g, Array.multiply(node.value, Tensor.sub_from(1.0, node.value))))]
        case "relu":
            return [Grad(pos=input0(node), value=Array.multiply(g, Tensor.gt_zero(node.value)))]
        case "leaky_relu":
            x = saved(node, 0)
            mask = Array.where(Tensor.gt_zero(x), Array.full(x.shape, 1.0), Array.full(x.shape, node.param))
            return [Grad(pos=input0(node), value=Array.multiply(g, mask))]
        case "gelu":
            return [Grad(pos=input0(node), value=Array.multiply(g, gelu_deriv(saved(node, 0))))]
        case "silu":
            return [Grad(pos=input0(node), value=Array.multiply(g, silu_deriv(saved(node, 0))))]
        case "sum_all":
            return [Grad(pos=input0(node), value=Tensor.broadcast_to(g, in_shape(tape, node, 0)))]
        case "sum_axis":
            xshape = in_shape(tape, node, 0)
            gg = Array.expand_dims(g, node.axis) if not node.keep else g
            return [Grad(pos=input0(node), value=Tensor.broadcast_to(gg, xshape))]
        case "mean_all":
            xshape = in_shape(tape, node, 0)
            return [Grad(pos=input0(node), value=Tensor.broadcast_to(Tensor.scale(g, 1.0 / toFloat(Tensor.flat(xshape))), xshape))]
        case "mean_axis":
            xshape = in_shape(tape, node, 0)
            scaled = Tensor.scale(g, 1.0 / toFloat(unwrap_or(listGet(xshape, node.axis), 1)))
            gg = Array.expand_dims(scaled, node.axis) if not node.keep else scaled
            return [Grad(pos=input0(node), value=Tensor.broadcast_to(gg, xshape))]
        case "matmul":
            a = saved(node, 0)
            b = saved(node, 1)
            return [Grad(pos=input0(node), value=Linalg.matmul(g, Array.transpose_all(b))), Grad(pos=input1(node), value=Linalg.matmul(Array.transpose_all(a), g))]
        case "softmax":
            go = Array.multiply(g, node.value)
            s = Array.sum_axis(go, node.axis, True)
            return [Grad(pos=input0(node), value=Array.multiply(node.value, Tensor.bsub(g, s)))]
        case "log_softmax":
            s = Array.sum_axis(g, node.axis, True)
            return [Grad(pos=input0(node), value=Tensor.bsub(g, Array.multiply(Array.exp(node.value), s)))]
        case _:
            return []


def input0(node):
    """Input id 0 of `node`."""
    return unwrap_or(listGet(node.inputs, 0), 0)


def input1(node):
    """Input id 1 of `node`."""
    return unwrap_or(listGet(node.inputs, 1), 0)


def grad(f, x):
    """torch.func.grad: the gradient of a tape-style function `f` at `x`,
    seeded with a ones cotangent."""
    s0 = leaf(empty(), x)
    s1 = f(s0.tape, s0.out)
    grads = backward(s1.tape, s1.out, Array.ones_like(value_of(s1.tape, s1.out)))
    return unwrap_or(listGet(grads, s0.out), Array.zeros_like(x))


def vjp(f, x, v):
    """torch.func.vjp: run `f` at `x` and pull back the cotangent `v`."""
    s0 = leaf(empty(), x)
    s1 = f(s0.tape, s0.out)
    grads = backward(s1.tape, s1.out, v)
    return VJP(primal=value_of(s1.tape, s1.out), cotangent=unwrap_or(listGet(grads, s0.out), Array.zeros_like(x)))


# Forward ops ------------------------------------------------------------

def add(tape, a, b):
    """Tape add (torch's `+`): broadcasts, backward passes the gradient."""
    av = value_of(tape, a)
    bv = value_of(tape, b)
    out = Array.add(av, bv)
    return append(tape, Node(op="add", inputs=[a, b], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def sub(tape, a, b):
    """Tape subtract (torch's `-`)."""
    av = value_of(tape, a)
    bv = value_of(tape, b)
    out = Array.subtract(av, bv)
    return append(tape, Node(op="sub", inputs=[a, b], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def mul(tape, a, b):
    """Tape multiply (torch's `*`): backward is `g * other`."""
    av = value_of(tape, a)
    bv = value_of(tape, b)
    out = Array.multiply(av, bv)
    return append(tape, Node(op="mul", inputs=[a, b], saved=[av, bv], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def div(tape, a, b):
    """Tape divide (torch's `/`): backward `g / b` and `-g * a / b^2`."""
    av = value_of(tape, a)
    bv = value_of(tape, b)
    out = Array.divide(av, bv)
    return append(tape, Node(op="div", inputs=[a, b], saved=[av, bv], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def neg(tape, a):
    """Tape negative."""
    av = value_of(tape, a)
    out = Array.negative(av)
    return append(tape, Node(op="neg", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def exp(tape, a):
    """Tape elementwise exp: backward is `g * out`."""
    av = value_of(tape, a)
    out = Array.exp(av)
    return append(tape, Node(op="exp", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def log(tape, a):
    """Tape elementwise natural log: backward is `g / a`."""
    av = value_of(tape, a)
    out = Array.log(av)
    return append(tape, Node(op="log", inputs=[a], saved=[av], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def sqrt(tape, a):
    """Tape elementwise sqrt: backward is `g / (2 * out)`."""
    av = value_of(tape, a)
    out = Array.sqrt(av)
    return append(tape, Node(op="sqrt", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def square(tape, a):
    """Tape elementwise square: backward is `2 * g * a`."""
    av = value_of(tape, a)
    out = Array.square(av)
    return append(tape, Node(op="square", inputs=[a], saved=[av], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def tanh(tape, a):
    """Tape elementwise tanh: backward is `g * (1 - out^2)`."""
    av = value_of(tape, a)
    out = Array.tanh(av)
    return append(tape, Node(op="tanh", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def sigmoid(tape, a):
    """Tape elementwise sigmoid: backward is `g * out * (1 - out)`."""
    av = value_of(tape, a)
    out = sigmoid_forward(av)
    return append(tape, Node(op="sigmoid", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def relu(tape, a):
    """Tape elementwise ReLU: backward is `g * (out > 0)`."""
    av = value_of(tape, a)
    out = Tensor.max_scalar(av, 0.0)
    return append(tape, Node(op="relu", inputs=[a], saved=[], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def leaky_relu(tape, a, slope):
    """Tape leaky ReLU with negative slope: backward is `g * (x > 0 ? 1 :
    slope)`."""
    av = value_of(tape, a)
    out = Array.where(Tensor.gt_zero(av), av, Tensor.scale(av, slope))
    return append(tape, Node(op="leaky_relu", inputs=[a], saved=[av], shape=out.shape, value=out, axis=0, param=slope, keep=False))


def gelu(tape, a):
    """Tape GELU with torch's default exact forward and backward formulas."""
    av = value_of(tape, a)
    out = gelu_forward(av)
    return append(tape, Node(op="gelu", inputs=[a], saved=[av], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def silu(tape, a):
    """Tape SiLU (swish): `x * sigmoid(x)`, torch's exact backward."""
    av = value_of(tape, a)
    out = Array.multiply(av, sigmoid_forward(av))
    return append(tape, Node(op="silu", inputs=[a], saved=[av], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def sum_all(tape, a):
    """Tape sum over every element: the output is a rank-0 scalar tensor."""
    av = value_of(tape, a)
    out = Array.scalar(Array.sum(av))
    return append(tape, Node(op="sum_all", inputs=[a], saved=[], shape=[], value=out, axis=0, param=0.0, keep=False))


def sum_axis(tape, a, axis, keepdims):
    """Tape sum along `axis`, with torch's keepdims flag."""
    av = value_of(tape, a)
    ax = norm_axis(axis, len(av.shape))
    out = Array.sum_axis(av, ax, keepdims)
    return append(tape, Node(op="sum_axis", inputs=[a], saved=[], shape=out.shape, value=out, axis=ax, param=0.0, keep=keepdims))


def mean_all(tape, a):
    """Tape mean over every element: the output is a rank-0 scalar tensor."""
    av = value_of(tape, a)
    out = Array.scalar(Array.mean(av))
    return append(tape, Node(op="mean_all", inputs=[a], saved=[], shape=[], value=out, axis=0, param=0.0, keep=False))


def mean_axis(tape, a, axis, keepdims):
    """Tape mean along `axis`, with torch's keepdims flag."""
    av = value_of(tape, a)
    ax = norm_axis(axis, len(av.shape))
    out = Array.mean_axis(av, ax, keepdims)
    return append(tape, Node(op="mean_axis", inputs=[a], saved=[], shape=out.shape, value=out, axis=ax, param=0.0, keep=keepdims))


def matmul(tape, a, b):
    """Tape matrix product (torch's `@`): backward is `g @ b^T` and `a^T @ g`
    for 2-D operands."""
    av = value_of(tape, a)
    bv = value_of(tape, b)
    out = Linalg.matmul(av, bv)
    return append(tape, Node(op="matmul", inputs=[a, b], saved=[av, bv], shape=out.shape, value=out, axis=0, param=0.0, keep=False))


def softmax(tape, a, dim):
    """Tape softmax along `dim`: torch's max-subtracted form, backward
    `out * (g - sum(g * out, dim, keepdim))`."""
    av = value_of(tape, a)
    ax = norm_axis(dim, len(av.shape))
    m = Array.max_axis(av, ax, True)
    e = Array.exp(Tensor.bsub(av, m))
    out = Tensor.bdiv(e, Array.sum_axis(e, ax, True))
    return append(tape, Node(op="softmax", inputs=[a], saved=[], shape=out.shape, value=out, axis=ax, param=0.0, keep=False))


def log_softmax(tape, a, dim):
    """Tape log-softmax along `dim`: backward `g - exp(out) * sum(g, dim)`."""
    av = value_of(tape, a)
    ax = norm_axis(dim, len(av.shape))
    m = Array.max_axis(av, ax, True)
    e = Array.exp(Tensor.bsub(av, m))
    out = Tensor.bsub(Tensor.bsub(av, m), Array.log(Array.sum_axis(e, ax, True)))
    return append(tape, Node(op="log_softmax", inputs=[a], saved=[], shape=out.shape, value=out, axis=ax, param=0.0, keep=False))


# Activation math ----------------------------------------------------------

def sigmoid_forward(x):
    """torch's sigmoid: `1 / (1 + exp(-x))`."""
    return Array.reciprocal(Tensor.add_scalar(Array.exp(Array.negative(x)), 1.0))


def gelu_forward(x):
    """torch's default exact GELU: x times the standard normal CDF."""
    return Array.multiply(x, normal_cdf(x))


def normal_cdf(x):
    """Standard normal probabilities through the scalar erf builtin."""
    return Array.NDArray(shape=x.shape, data=perform(Storage.Buffer.build(Tensor.flat(x.shape), lambda i: 0.5 * (1.0 + erf(Storage.at(x.data, i) * INV_SQRT_TWO)))))


def gelu_deriv(x):
    """The exact GELU derivative: normal CDF plus x times normal density."""
    density = Tensor.scale(Array.exp(Tensor.scale(Array.square(x), -0.5)), INV_SQRT_TWO_PI)
    return Array.add(normal_cdf(x), Array.multiply(x, density))


def silu_deriv(x):
    """torch's SiLU backward: `sig * (1 + x * (1 - sig))`."""
    sig = sigmoid_forward(x)
    return Array.multiply(sig, Tensor.add_scalar(Array.multiply(x, Tensor.sub_from(1.0, sig)), 1.0))


def norm_axis(axis, nd):
    """A possibly-negative axis normalised against `nd` dimensions, without
    numpy's error effect (the port's ops assume a valid axis)."""
    return axis if axis >= 0 else axis + nd
