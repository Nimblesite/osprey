# osprey: flavor=python
# osprey: namespace=micrograd_tests
# osprey: module=Ops
"""The 147 op cases: every micrograd operator, its node ids and its builder.
The first 37 are the original battery; the remaining 110 are independent
permutations over 11 shapes and 10 input variants (ReLU signs, powers, reused
nodes, branching, tanh/exp and long mixed graphs). Each case builds the
expression over fresh leaves, runs `backward`, and the main suite compares the
output data and input gradients with the oracle's."""

from micrograd import Engine
from micrograd_expected import Expected
from testing import Check

__all__ = ["OP_NAMES", "OP_COUNT", "op_case", "list_cases", "nested_cases", "repeat_cases"]

OP_NAMES = ["add", "add_const", "radd", "mul", "mul_const", "rmul", "pow2", "pow3", "pow1", "pow0", "pow_neg1", "pow_neg2", "relu_neg", "relu_pos", "relu_zero", "relu_negzero", "neg", "sub", "rsub", "truediv", "truediv_const", "rtruediv", "self_add", "self_mul", "diamond", "reuse", "chain", "large", "small", "tanh_neg", "tanh_pos", "tanh_zero", "exp_neg", "exp_pos", "exp_zero", "tanh_mul", "exp_add"]
"""The 37 original op case names, in the oracle's order; `op.<name>` is the assertion label."""

OP_INPUTS = [[-4.0, 2.0], [-4.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0], [-4.0], [2.0], [-4.0], [-4.0], [2.0], [-4.0], [-4.0], [2.0], [0.0], [-0.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0, 2.0], [-4.0], [-4.0], [3.0], [3.0], [-4.0, 2.0], [-4.0, 2.0], [-4.0, 2.0], [1e150, 1e10], [1e-150, 1e-10], [-4.0], [2.0], [0.0], [-4.0], [2.0], [0.0], [-4.0, 2.0], [-4.0, 2.0]]
"""The 37 original op case inputs, in the oracle's order."""

EXTRA_SHAPES = ["relu", "relu_mix", "branch", "powers", "powmix", "reuse_mul", "reuse_add", "reuse_tanh", "exp_mix", "long_mixed", "signs"]
"""The 110 extra cases' 11 shape families, in oracle order."""

EXTRA_VARIANTS = [[3.0, 2.0], [-3.0, 2.0], [3.0, -2.0], [-3.0, -2.0], [0.0, 2.0], [-0.0, 2.0], [1.0, -1.0], [-1.0, 0.5], [2.5, 0.5], [-2.5, -0.5]]
"""The 10 input variants every extra shape runs over."""

OP_COUNT = 147
"""Every op case: the 37 originals plus 110 extra permutations."""


def op_name(i):
    """Case i's assertion label: `op.<original>` or `op.<shape>.<variant>`."""
    if i < len(OP_NAMES):
        return "op." + unwrap_or(listGet(OP_NAMES, i), "?")
    else:
        k = i - len(OP_NAMES)
        return "op." + unwrap_or(listGet(EXTRA_SHAPES, k // 10), "?") + "." + toString(k % 10)


def op_inputs(i):
    """Case i's input list: the original table or the matching extra variant."""
    if i < len(OP_INPUTS):
        return unwrap_or(listGet(OP_INPUTS, i), [])
    else:
        return unwrap_or(listGet(EXTRA_VARIANTS, (i - len(OP_INPUTS)) % 10), [])


def list_cases(name, got, want, rtol):
    """One `close` per element plus a length check, so a wrong node is named."""
    elems = fold(range(len(want)), [], lambda acc, i: listAppend(acc, Check.close(name + "." + toString(i), unwrap_or(listGet(got, i), 0.0), unwrap_or(listGet(want, i), 0.0), rtol, 0.0)))
    return listAppend(elems, Check.truth(name + ".length", len(got) == len(want), "length mismatch"))


def nested_cases(name, got, want, rtol):
    """One `list_cases` per row and a row-count check."""
    elems = fold(range(len(want)), [], lambda acc, i: listConcat(acc, list_cases(name + "." + toString(i), unwrap_or(listGet(got, i), []), unwrap_or(listGet(want, i), []), rtol)))
    return listAppend(elems, Check.truth(name + ".rows", len(got) == len(want), "row count mismatch"))


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


def chain(z, a, b):
    """The three-step graph the mul and add shapes share, from its first node."""
    zz = Engine.mul(z, Engine.top(z), Engine.top(z))
    za = Engine.mul(zz, Engine.top(z), a)
    zb = Engine.mul(za, Engine.top(z), b)
    s = Engine.add(zb, Engine.top(zz), Engine.top(za))
    return Engine.add(s, Engine.top(s), Engine.top(zb))


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


def build_extra(k, g, a, b):
    """Extra case k's expression over node ids a and b, mirrored shape for shape
    from the oracle's `extra_ref`."""
    shape = k // 10
    if shape == 0:
        r1 = Engine.relu(g, a)
        r2 = Engine.relu(r1, b)
        return Engine.add(r2, Engine.top(r1), Engine.top(r2))
    elif shape == 1:
        r1 = Engine.relu(g, a)
        r2 = Engine.relu(r1, b)
        m = Engine.mul(r2, Engine.top(r1), Engine.top(r2))
        s = Engine.add(m, Engine.top(m), a)
        return Engine.sub(s, Engine.top(s), b)
    elif shape == 2:
        s = Engine.add(g, a, b)
        r1 = Engine.relu(s, Engine.top(s))
        d = Engine.sub(r1, a, b)
        r2 = Engine.relu(d, Engine.top(d))
        return Engine.mul(r2, Engine.top(r1), Engine.top(r2))
    elif shape == 3:
        a3 = Engine.pow_const(g, a, 3)
        a2 = Engine.pow_const(a3, a, 2)
        a2b = Engine.mul(a2, Engine.top(a2), b)
        s1 = Engine.add(a2b, Engine.top(a3), Engine.top(a2b))
        b2 = Engine.pow_const(s1, b, 2)
        ab2 = Engine.mul(b2, a, Engine.top(b2))
        s2 = Engine.sub(ab2, Engine.top(s1), Engine.top(ab2))
        b3 = Engine.pow_const(s2, b, 3)
        return Engine.add(b3, Engine.top(s2), Engine.top(b3))
    elif shape == 4:
        s = Engine.add(g, a, b)
        p = Engine.pow_const(s, Engine.top(s), 2)
        d = Engine.sub(p, a, b)
        m = Engine.mul(d, Engine.top(p), Engine.top(d))
        prod = Engine.mul(m, a, b)
        pp = Engine.pow_const(prod, Engine.top(prod), 2)
        return Engine.add(pp, Engine.top(m), Engine.top(pp))
    elif shape == 5:
        return chain(Engine.mul(g, a, b), a, b)
    elif shape == 6:
        return chain(Engine.add(g, a, b), a, b)
    elif shape == 7:
        t = Engine.tanh(g, a)
        tt = Engine.mul(t, Engine.top(t), Engine.top(t))
        tb = Engine.mul(tt, Engine.top(t), b)
        return Engine.add(tb, Engine.top(tt), Engine.top(tb))
    elif shape == 8:
        e = Engine.exp(g, b)
        eb = Engine.mul(e, Engine.top(e), b)
        ea = Engine.mul(eb, Engine.top(e), a)
        return Engine.add(ea, Engine.top(eb), Engine.top(ea))
    elif shape == 9:
        m0 = Engine.mul(g, a, b)
        s0 = Engine.add(m0, Engine.top(m0), a)
        d0 = Engine.sub(s0, a, b)
        m1 = Engine.mul(d0, Engine.top(s0), Engine.top(d0))
        sb = Engine.add(m1, a, b)
        r = Engine.relu(sb, Engine.top(sb))
        s1 = Engine.add(r, Engine.top(m1), Engine.top(r))
        a2 = Engine.pow_const(s1, a, 2)
        s2 = Engine.add(a2, Engine.top(a2), b)
        m2 = Engine.mul(s2, Engine.top(s1), Engine.top(s2))
        t = Engine.tanh(m2, a)
        s3 = Engine.sub(t, Engine.top(m2), Engine.top(t))
        b2 = Engine.pow_const(s3, b, 2)
        den = Engine.add_const(b2, Engine.top(b2), 2.0)
        return Engine.truediv(den, Engine.top(s3), Engine.top(den))
    else:
        d = Engine.sub(g, a, b)
        s = Engine.add(d, a, b)
        m = Engine.mul(s, Engine.top(d), Engine.top(s))
        n = Engine.neg(m, Engine.top(m))
        p = Engine.mul(n, a, b)
        ab = Engine.add(p, Engine.top(n), Engine.top(p))
        q = Engine.truediv(ab, a, b)
        return Engine.sub(q, Engine.top(ab), Engine.top(q))


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
    elif i < 37:
        return build_math(i, g, a, b)
    else:
        return build_extra(i - 37, g, a, b)


def op_case(i):
    """One op case: build the expression, backward, compare data and input grads."""
    xs = op_inputs(i)
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
    return Check.close_all(op_name(i), got, want, rtol, 0.0)


class RepeatState:
    """A repeated-backward case: the live gradients and the rows recorded so far."""
    grads: list[float]
    rows: list[list[float]]


class RepeatPlan:
    """A repeated-backward graph: its roots and the root index each call uses."""
    graph: Engine.Graph
    roots: list[int]
    calls: list[int]


def repeat_plan(i, g, a, b):
    """Repeated-backward case i's graph and the roots each call descends from."""
    if i == 0:
        m = Engine.mul(g, a, b)
        y = Engine.add(m, Engine.top(m), a)
        return RepeatPlan(graph=y, roots=[Engine.top(y)], calls=[0, 0, 0])
    elif i == 1:
        s = Engine.add(g, a, b)
        y = Engine.mul(s, Engine.top(s), Engine.top(s))
        return RepeatPlan(graph=y, roots=[Engine.top(y)], calls=[0, 0])
    elif i == 2:
        s = Engine.add(g, a, b)
        r = Engine.relu(s, Engine.top(s))
        d = Engine.sub(r, a, b)
        y = Engine.mul(d, Engine.top(r), Engine.top(d))
        return RepeatPlan(graph=y, roots=[Engine.top(y)], calls=[0, 0])
    elif i == 3:
        t = Engine.tanh(g, a)
        tb = Engine.mul(t, Engine.top(t), b)
        y = Engine.add(tb, Engine.top(tb), Engine.top(t))
        return RepeatPlan(graph=y, roots=[Engine.top(y)], calls=[0, 0])
    elif i == 4:
        u = Engine.mul(g, a, b)
        v = Engine.add(u, Engine.top(u), a)
        w = Engine.mul(v, Engine.top(u), b)
        return RepeatPlan(graph=w, roots=[Engine.top(v), Engine.top(w)], calls=[0, 1, 0])
    else:
        m = Engine.mul(g, a, b)
        z = Engine.add(m, Engine.top(m), a)
        r = Engine.relu(z, Engine.top(z))
        p = Engine.mul(r, Engine.top(r), Engine.top(z))
        t = Engine.tanh(p, Engine.top(z))
        q = Engine.mul(t, Engine.top(t), a)
        y = Engine.add(q, Engine.top(p), Engine.top(q))
        return RepeatPlan(graph=y, roots=[Engine.top(y), Engine.top(z)], calls=[0, 0, 1])


def repeat_next(st, j, plan, aid, bid):
    """One more backward call, accumulating into the live gradients."""
    root = unwrap_or(listGet(plan.roots, unwrap_or(listGet(plan.calls, j), 0)), 0)
    grads = Engine.backward_into(plan.graph, root, st.grads)
    row = listAppend(listAppend([], unwrap_or(listGet(grads, aid), 0.0)), unwrap_or(listGet(grads, bid), 0.0))
    return RepeatState(grads=grads, rows=listAppend(st.rows, row))


def repeat_case(i):
    """One repeated-backward permutation: every call's input gradients."""
    xs = unwrap_or(listGet(Expected.REPEAT_INPUTS, i), [])
    g = Engine.empty()
    a = Engine.value(g, unwrap_or(listGet(xs, 0), 0.0))
    aid = Engine.top(a)
    b = Engine.value(a, unwrap_or(listGet(xs, 1), 0.0))
    bid = Engine.top(b)
    plan = repeat_plan(i, b, aid, bid)
    first = unwrap_or(listGet(plan.calls, 0), 0)
    grads = Engine.backward(plan.graph, unwrap_or(listGet(plan.roots, first), 0))
    first_row = listAppend(listAppend([], unwrap_or(listGet(grads, aid), 0.0)), unwrap_or(listGet(grads, bid), 0.0))
    st = fold(range(1, len(plan.calls)), RepeatState(grads=grads, rows=listAppend([], first_row)), lambda s, j: repeat_next(s, j, plan, aid, bid))
    rtol = 1e-9 if i == 3 or i == 5 else 1e-12
    return nested_cases("repeat." + toString(i), st.rows, unwrap_or(listGet(Expected.REPEAT_WANT, i), []), rtol)


def repeat_cases():
    """Six repeated-backward permutations over reused and branching graphs."""
    return fold(range(6), [], lambda acc, i: listConcat(acc, repeat_case(i)))
