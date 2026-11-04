# osprey: flavor=python
# osprey: namespace=micrograd
# osprey: module=Engine
"""micrograd's autograd engine as an immutable graph arena.

micrograd's `Value` is a mutable node with `grad` fields and `_backward`
closures; Osprey values are immutable, so the same semantics live in one
`Graph` record holding every node ever built, in creation order: an op string
(micrograd's `_op`), the operand ids, the forward data, and the integer
exponent of a `**` node. Each operation appends its nodes exactly as
micrograd's operators construct `Value`s — including the leaf a bare Python
number is wrapped into — and returns the extended graph; `top(g)` is the new
node's id. `backward` returns the gradient of every node, computed by the same
depth-first topological order and the same chain-rule accumulation as
micrograd's `Value.backward`, starting from zero gradients (micrograd's
`zero_grad` folded in); `backward_into` accumulates into gradients you
already hold, which is micrograd's call-backward-again behaviour.
"""

from micrograd.Libm import exp_of, tanh_of

__all__ = [
    "Node", "Graph", "Cursor", "empty", "size", "top", "node", "data", "value", "add",
    "add_const", "mul", "mul_const", "pow_const", "relu", "tanh", "exp", "neg", "sub",
    "sub_const", "rsub_const", "truediv", "truediv_const", "rtruediv_const", "ipow",
    "zero_grad", "backward", "backward_into",
]


class Node:
    """One node of the arena: micrograd's `_op` string (`""`, `+`, `*`, `**`,
    `ReLU`, `tanh`, `exp`), the operand ids (-1 when unused), the forward data,
    and the integer exponent a `**` node raises to."""
    op: str
    a: int
    b: int
    data: float
    exponent: int


class Graph:
    """Every node ever built, in creation order; a node's id is its index."""
    nodes: list[Node]


class Cursor:
    """A result under construction: the graph and the id of its current node."""
    graph: Graph
    id: int


def empty():
    """A fresh, empty graph arena."""
    return Graph(nodes=[])


def size(g):
    """The number of nodes in the graph."""
    return len(g.nodes)


def top(g):
    """The id of the most recently appended node: every operation's result."""
    return len(g.nodes) - 1


def node(g, i):
    """Node `i`, or a neutral leaf when `i` is out of range."""
    return unwrap_or(listGet(g.nodes, i), Node(op="", a=-1, b=-1, data=0.0, exponent=0))


def data(g, i):
    """The forward data of node `i` (micrograd's `Value.data`)."""
    return node(g, i).data


def append(g, n):
    """The graph with one more node."""
    return Graph(nodes=listAppend(g.nodes, n))


def value(g, x):
    """micrograd's `Value(data)`: a leaf node wrapping a constant or an input."""
    return append(g, Node(op="", a=-1, b=-1, data=x, exponent=0))


def add(g, a, b):
    """micrograd's `__add__`: a `+` node over both operand ids."""
    return append(g, Node(op="+", a=a, b=b, data=data(g, a) + data(g, b), exponent=0))


def add_const(g, a, k):
    """`a + k` for a bare number k: micrograd wraps k in a leaf, then adds."""
    g2 = value(g, k)
    return add(g2, a, top(g2))


def mul(g, a, b):
    """micrograd's `__mul__`: a `*` node over both operand ids."""
    return append(g, Node(op="*", a=a, b=b, data=data(g, a) * data(g, b), exponent=0))


def mul_const(g, a, k):
    """`a * k` for a bare number k: micrograd wraps k in a leaf, then multiplies."""
    g2 = value(g, k)
    return mul(g2, a, top(g2))


def pow_const(g, a, k):
    """micrograd's `__pow__` with a constant integer exponent: one `**` node."""
    return append(g, Node(op="**", a=a, b=-1, data=ipow(data(g, a), k), exponent=k))


def relu(g, a):
    """micrograd's `relu`: 0 below zero, else the value, as a `ReLU` node."""
    return append(g, Node(op="ReLU", a=a, b=-1, data=0.0 if data(g, a) < 0.0 else data(g, a), exponent=0))


def tanh(g, a):
    """micrograd's lecture `tanh`: `(e^2x - 1) / (e^2x + 1)` as a `tanh` node."""
    return append(g, Node(op="tanh", a=a, b=-1, data=tanh_of(data(g, a)), exponent=0))


def exp(g, a):
    """micrograd's lecture `exp`: e raised to a, as an `exp` node."""
    return append(g, Node(op="exp", a=a, b=-1, data=exp_of(data(g, a)), exponent=0))


def neg(g, a):
    """micrograd's `__neg__`: `a * -1`, so a leaf and a `*` node."""
    return mul_const(g, a, -1.0)


def sub(g, a, b):
    """micrograd's `__sub__` over a Value: `a + (-b)`, the neg's two nodes then a `+`."""
    g2 = neg(g, b)
    return add(g2, a, top(g2))


def sub_const(g, a, k: float):
    """`a - k` for a bare number k: micrograd negates the number itself, wraps
    it in a leaf, then adds — two nodes, no `*` node. The annotation is needed:
    unary minus on an unannotated parameter infers int."""
    g2 = value(g, -k)
    return add(g2, a, top(g2))


def rsub_const(g, a, k):
    """micrograd's `__rsub__` with a bare number: `k - a` as `k + (-a)`."""
    g2 = neg(g, a)
    return add_const(g2, top(g2), k)


def truediv(g, a, b):
    """micrograd's `__truediv__` over a Value: `a * b**-1`, a `**` node then a `*`."""
    g2 = pow_const(g, b, -1)
    return mul(g2, a, top(g2))


def truediv_const(g, a, k):
    """`a / k` for a bare number k: micrograd computes `k**-1` on the number
    itself, wraps the reciprocal in a leaf, then multiplies."""
    g2 = value(g, 1.0 / k)
    return mul(g2, a, top(g2))


def rtruediv_const(g, a, k):
    """micrograd's `__rtruediv__` with a bare number: `k * a**-1`, a `**` node,
    a leaf for k, then a `*` node."""
    g2 = pow_const(g, a, -1)
    g3 = value(g2, k)
    return mul(g3, top(g2), top(g3))


def ipow(x, n):
    """`x ** n` for an integer exponent, by repeated multiplication. Osprey has
    no `pow` builtin yet, so the port computes integer powers itself: positive
    exponents by folding `*`, negative ones through the reciprocal. Float
    exponents wait for the builtin (the `**` op supports integers and -1)."""
    if n < 0:
        return 1.0 / ipow(x, -n)
    else:
        return fold(range(n), 1.0, lambda acc, _: acc * x)


def zero_grad(g):
    """micrograd's `Module.zero_grad`, functionally: a fresh zero gradient per
    node, which is the gradient state `backward` starts from."""
    return toList(map(range(size(g)), lambda _: 0.0))


def backward(g, root):
    """micrograd's `Value.backward` after `zero_grad`: fresh zero gradients,
    the root's gradient set to 1.0, then every node's `_backward` in reversed
    topological order."""
    return backward_into(g, root, zero_grad(g))


def backward_into(g, root, grads):
    """micrograd's `Value.backward` accumulating into `grads`: the root's
    gradient is set to 1.0, then every node's `_backward` runs in reversed
    topological order, so calling it with an earlier result doubles gradients,
    exactly as micrograd without `zero_grad`."""
    indexed = fold(range(len(grads)), {}, lambda acc, i: mapSet(acc, toString(i), unwrap_or(listGet(grads, i), 0.0)))
    started = set_at(indexed, root, 1.0)
    result = foldList(listReverse(build_topo(g, root)), started, lambda acc, i: step(g, i, acc))
    return toList(map(range(len(grads)), lambda i: unwrap_or(mapGet(result, toString(i)), 0.0)))


def set_at(xs, i, v):
    """The persistent gradient table with node i replaced by v."""
    return mapSet(xs, toString(i), v)


def add_at(xs, i, d):
    """micrograd's `grad +=`, path-copying the persistent node table."""
    return mapSet(xs, toString(i), unwrap_or(mapGet(xs, toString(i)), 0.0) + d)


def operands(n):
    """micrograd's `_prev` set: the node's operand ids, deduplicated, in
    operand order — `(self, other)` before the set removed the duplicate."""
    if n.a < 0:
        return []
    elif n.b < 0 or n.b == n.a:
        return [n.a]
    else:
        return [n.a, n.b]


def set_flag(flags, i):
    """The persistent visited set with node i marked True."""
    return mapSet(flags, toString(i), True)


class Walk:
    """`build_topo`'s state: the visited flags and the postorder ids so far."""
    visited: dict[str, bool]
    order: list[int]


def build_topo(g, root):
    """micrograd's `build_topo`: depth-first postorder from the root over the
    deduplicated operands. micrograd iterates `_prev`, a set whose order
    follows object hashes, so its own traversal order varies run to run; this
    port and the oracle both fix the operand order, which makes every gradient
    reproducible while keeping micrograd's algorithm."""
    state = visit(g, root, Walk(visited={}, order=[]))
    return state.order


def visit(g, i, state):
    """One `build_topo` step: mark i visited, visit its operands, then append
    i to the postorder."""
    if unwrap_or(mapGet(state.visited, toString(i)), False):
        return state
    else:
        marked = Walk(visited=set_flag(state.visited, i), order=state.order)
        walked = foldList(operands(node(g, i)), marked, lambda acc, k: visit(g, k, acc))
        return Walk(visited=walked.visited, order=listAppend(walked.order, i))


def step(g, i, grads):
    """One node's `_backward`: its chain-rule contributions to its operands'
    gradients, in micrograd's order (self before other, so a node used on both
    sides of an op accumulates twice)."""
    n = node(g, i)
    og = unwrap_or(mapGet(grads, toString(i)), 0.0)
    if n.op == "":
        return grads
    elif n.op == "+":
        return add_at(add_at(grads, n.a, og), n.b, og)
    elif n.op == "*":
        return add_at(add_at(grads, n.a, data(g, n.b) * og), n.b, data(g, n.a) * og)
    elif n.op == "**":
        return add_at(grads, n.a, (n.exponent * ipow(data(g, n.a), n.exponent - 1)) * og)
    elif n.op == "ReLU":
        return add_at(grads, n.a, (1.0 if n.data > 0.0 else 0.0) * og)
    elif n.op == "tanh":
        return add_at(grads, n.a, (1.0 - n.data * n.data) * og)
    else:
        return add_at(grads, n.a, n.data * og)
