# osprey: flavor=python
# osprey: namespace=micrograd_tests
# osprey: module=Ops
"""The 37 op cases: every micrograd operator, its node ids and its builder.
Each case builds the expression over fresh leaves, runs `backward`, and the
main suite compares the output data and input gradients with the oracle's."""

from micrograd import Engine
from micrograd_expected import Expected
from testing import Check

__all__ = ["OP_NAMES", "op_case"]

OP_NAMES = ["add", "add_const", "radd", "mul", "mul_const", "rmul", "pow2", "pow3", "pow1", "pow0", "pow_neg1", "pow_neg2", "relu_neg", "relu_pos", "relu_zero", "relu_negzero", "neg", "sub", "rsub", "truediv", "truediv_const", "rtruediv", "self_add", "self_mul", "diamond", "reuse", "chain", "large", "small", "tanh_neg", "tanh_pos", "tanh_zero", "exp_neg", "exp_pos", "exp_zero", "tanh_mul", "exp_add"]
"""The 37 op case names, in the oracle's order; `op.<name>` is the assertion label."""

OP_INPUTS = [[-4.0, 2.0], [-4.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0], [-4.0], [2.0], [-4.0], [-4.0], [2.0], [-4.0], [-4.0], [2.0], [0.0], [-0.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0], [3.0], [3.0], [-4.0, 2.0], [-4.0, 2.0], [-4.0, 2.0], [1e150, 1e10], [1e-150, 1e-10], [-4.0], [2.0], [0.0], [-4.0], [2.0], [0.0], [-4.0, 2.0], [-4.0, 2.0]]


def build_add_mul(i, g, a, b):
    """Op cases 0..5: add/mul over two ids or a constant."""
    if i == 0:
        return Engine.add(g, a, b)
    elif i == 1 or i == 2:
        return Engine.add_const(g, a, 2.0)
    elif i == 3:
        return Engine.mul(g, a, b)
    else:
        return Engine.mul_const(g, a, 2.0)


def build_pow(i, g, a):
    """Op cases 6..11: integer powers."""
    if i == 6:
        return Engine.pow_const(g, a, 2)
    elif i == 7:
        return Engine.pow_const(g, a, 3)
    elif i == 8:
        return Engine.pow_const(g, a, 1)
    elif i == 9:
        return Engine.pow_const(g, a, 0)
    elif i == 10:
        return Engine.pow_const(g, a, -1)
    else:
        return Engine.pow_const(g, a, -2)


def build_relu_neg(i, g, a):
    """Op cases 12..16: relu, or negation."""
    return Engine.neg(g, a) if i == 16 else Engine.relu(g, a)


def build_div(i, g, a, b):
    """Op cases 17..23: subtraction, division and self-ops."""
    if i == 17:
        return Engine.sub(g, a, b)
    elif i == 18:
        return Engine.rsub_const(g, a, 2.0)
    elif i == 19:
        return Engine.truediv(g, a, b)
    elif i == 20:
        return Engine.truediv_const(g, a, 2.0)
    elif i == 21:
        return Engine.rtruediv_const(g, a, 10.0)
    elif i == 22:
        return Engine.add(g, a, a)
    else:
        return Engine.mul(g, a, a)


def build_combo(i, g, a, b):
    """Op cases 24..28: diamonds, reuse, chains and magnitude products."""
    if i == 24:
        z = Engine.add(g, a, b)
        return Engine.mul(z, Engine.top(z), Engine.top(z))
    elif i == 25:
        m = Engine.mul(g, a, b)
        return Engine.add(m, Engine.top(m), a)
    elif i == 26:
        m1 = Engine.mul_const(g, a, 2.0)
        m2 = Engine.mul_const(m1, b, 3.0)
        return Engine.add(m2, Engine.top(m1), Engine.top(m2))
    else:
        return Engine.mul(g, a, b)


def build_math(i, g, a, b):
    """Op cases 29..36: tanh and exp, bare and composed."""
    if i == 29 or i == 30 or i == 31:
        return Engine.tanh(g, a)
    elif i == 32 or i == 33 or i == 34:
        return Engine.exp(g, a)
    elif i == 35:
        t = Engine.tanh(g, a)
        return Engine.mul(t, Engine.top(t), b)
    else:
        e = Engine.exp(g, b)
        return Engine.add(e, a, Engine.top(e))


def build_op(i, g, a, b):
    """Case i's micrograd expression over node ids a and b."""
    if i < 6:
        return build_add_mul(i, g, a, b)
    elif i < 12:
        return build_pow(i, g, a)
    elif i < 17:
        return build_relu_neg(i, g, a)
    elif i < 24:
        return build_div(i, g, a, b)
    elif i < 29:
        return build_combo(i, g, a, b)
    else:
        return build_math(i, g, a, b)


def op_case(i):
    """One op case: build the expression, backward, compare data and input grads."""
    xs = unwrap_or(listGet(OP_INPUTS, i), [])
    g = Engine.empty()
    g1 = Engine.value(g, unwrap_or(listGet(xs, 0), 0.0))
    a = Engine.top(g1)
    g2 = Engine.value(g1, unwrap_or(listGet(xs, 1), 0.0))
    b = Engine.top(g2)
    g3 = build_op(i, g2, a, b)
    root = Engine.top(g3)
    grads = Engine.backward(g3, root)
    want = unwrap_or(listGet(Expected.OP_WANT, i), [])
    rtol = unwrap_or(listGet(Expected.OP_RTOL, i), 1e-12)
    base = [Engine.data(g3, root), unwrap_or(listGet(grads, a), 0.0)]
    got = listAppend(base, unwrap_or(listGet(grads, b), 0.0)) if len(xs) > 1 else base
    return Check.close_all("op." + unwrap_or(listGet(OP_NAMES, i), "?"), got, want, rtol, 0.0)
