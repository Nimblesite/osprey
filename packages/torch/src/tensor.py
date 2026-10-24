# osprey: flavor=python
# osprey: namespace=torch
# osprey: module=Tensor
"""Shared float64 tensor helpers the autograd tape and the optimizers build on.

A torch tensor is a `numpy.Array.NDArray[B]` — a float64 array over the buffer
representation `B` the installed numpy handler picks — so one body computes on
host lists or the GPU by handler [PORT-DEVICE]. This module adds the pieces
numpy does not provide: float powers (the flavor has no pow builtin yet), a
scalar scale, correct broadcasting, and `sum_to_size`, the broadcast reduction
torch's autograd applies to every incoming gradient.

The `b*` elementwise helpers and `broadcast_to` reimplement broadcasting here
because numpy's `Layout.offset_of` divides by zero on a zero (broadcast)
stride; see the report's `## Needs from numpy`.
"""
from numpy import Array, Storage

__all__ = [
    "pow_f", "scale", "add_scalar", "sub_scalar", "sub_from", "div_scalar", "max_scalar",
    "one_minus", "flat", "strides", "src_offset", "bcast_shapes2", "combine_op",
    "broadcast_to", "badd", "bsub", "bmul", "bdiv", "bmax", "bmin", "gt_zero",
    "sum_axes", "sum_to_size",
]

ADD_TAG = 0
SUB_TAG = 1
MUL_TAG = 2
DIV_TAG = 3
MAX_TAG = 4
MIN_TAG = 5


def pow_f(base, n):
    """`base` raised to a non-negative integer power, by repeated
    multiplication (the flavor has no pow builtin; a compiler pi is adding
    one). Matches CPython's `base ** n` to within a few ulps, well inside the
    1e-12 rtol the optimizers compare at."""
    return fold(range(n), 1.0, lambda acc, _: acc * base)


def flat(shape):
    """The number of elements a shape holds (1 for rank 0), the product of
    its dimensions."""
    return fold(range(len(shape)), 1, lambda acc, d: acc * unwrap_or(listGet(shape, d), 1))


def strides(shape):
    """Row-major strides of a shape."""
    return toList(map(range(len(shape)), lambda d: fold(range(d + 1, len(shape)), 1, lambda acc, k: acc * unwrap_or(listGet(shape, k), 1))))


def src_offset(i, out_shape, src_shape):
    """The flat source offset for output index `i`, with `src_shape`
    right-aligned and broadcast inside `out_shape` (a broadcast dimension
    contributes 0, so there is no division by a zero stride)."""
    gap = len(out_shape) - len(src_shape)
    out_strides = strides(out_shape)
    src_strides = strides(src_shape)
    return fold(range(len(out_shape)), 0, lambda acc, d: acc + broadcast_step(d, i, gap, out_shape, out_strides, src_shape, src_strides))


def broadcast_step(d, i, gap, out_shape, out_strides, src_shape, src_strides):
    """One dimension's contribution to a broadcast source offset."""
    if d < gap or unwrap_or(listGet(src_shape, d - gap), 1) == 1:
        return 0
    else:
        coord = (i // unwrap_or(listGet(out_strides, d), 1)) % unwrap_or(listGet(out_shape, d), 1)
        return coord * unwrap_or(listGet(src_strides, d - gap), 0)


def bcast_shapes2(a, b):
    """The broadcast of two shapes (right-aligned; assumes they are
    compatible, as the port's ops keep them)."""
    rank = len(a) if len(a) > len(b) else len(b)
    return toList(map(range(rank), lambda d: bcast_dim(d, rank, a, b)))


def bcast_dim(d, rank, a, b):
    """One dimension of a two-shape broadcast, right-aligned."""
    x = dim_right(a, rank - 1 - d)
    y = dim_right(b, rank - 1 - d)
    return y if x == 1 else x


def dim_right(shape, k):
    """The k-th dimension from the right, 1 past the rank (a broadcast
    operand's missing dimensions)."""
    return unwrap_or(listGet(shape, len(shape) - 1 - k), 1)


def combine_op(op, x, y):
    """The elementwise combine selected by an int tag."""
    match op:
        case 0:
            return x + y
        case 1:
            return x - y
        case 2:
            return x * y
        case 3:
            return x / y
        case 4:
            return Storage.max_f(x, y)
        case 5:
            return Storage.min_f(x, y)
        case _:
            return x + y


def bcast(a, b, op):
    """One elementwise binary operation with broadcasting, computed through
    the buffer effect with a correct source offset."""
    out_shape = bcast_shapes2(a.shape, b.shape)
    return Array.NDArray(shape=out_shape, data=perform(Storage.Buffer.build(flat(out_shape), lambda i: combine_op(op, Storage.at(a.data, src_offset(i, out_shape, a.shape)), Storage.at(b.data, src_offset(i, out_shape, b.shape))))))


def broadcast_to(a, shape):
    """`a` read through broadcast offsets over `shape` (the port's own, since
    numpy's divides by a zero stride)."""
    return Array.NDArray(shape=shape, data=perform(Storage.Buffer.build(flat(shape), lambda i: Storage.at(a.data, src_offset(i, shape, a.shape)))))


def badd(a, b):
    """Broadcast add."""
    return bcast(a, b, ADD_TAG)


def bsub(a, b):
    """Broadcast subtract."""
    return bcast(a, b, SUB_TAG)


def bmul(a, b):
    """Broadcast multiply."""
    return bcast(a, b, MUL_TAG)


def bdiv(a, b):
    """Broadcast divide."""
    return bcast(a, b, DIV_TAG)


def bmax(a, b):
    """Broadcast maximum (NaN wins, as numpy's maximum)."""
    return bcast(a, b, MAX_TAG)


def bmin(a, b):
    """Broadcast minimum (NaN wins, as numpy's minimum)."""
    return bcast(a, b, MIN_TAG)


def scale(a, c):
    """`a * c`, scaling a tensor by a scalar, without broadcasting a rank-0
    operand (a same-shape fill, which numpy's offset path handles)."""
    return Array.multiply(a, Array.full(a.shape, c))


def add_scalar(a, c):
    """`a + c`, elementwise with a scalar, same-shape."""
    return Array.add(a, Array.full(a.shape, c))


def sub_scalar(a, c):
    """`a - c`, elementwise with a scalar, same-shape."""
    return Array.subtract(a, Array.full(a.shape, c))


def sub_from(c, a):
    """`c - a`, elementwise with a scalar, same-shape."""
    return Array.subtract(Array.full(a.shape, c), a)


def max_scalar(a, c):
    """`max(a, c)`, elementwise with a scalar, same-shape."""
    return Array.maximum(a, Array.full(a.shape, c))


def div_scalar(a, c):
    """`a / c`, elementwise with a scalar, same-shape (torch's element order
    for Adam's `sqrt(m) / sqrt(bc)`)."""
    return Array.divide(a, Array.full(a.shape, c))


def gt_zero(a):
    """`a > 0` as 0.0/1.0 floats, same-shape."""
    return Array.greater(a, Array.full(a.shape, 0.0))


def one_minus(x):
    """`1.0 - x`, the scalar helper torch's update formulas spell inline."""
    return 1.0 - x


def sum_axes(a, axes):
    """Sum over each axis in `axes`, keeping each axis at size 1, in the
    order given, so the positions of later axes are unchanged."""
    return foldList(axes, a, lambda acc, ax: Array.sum_axis(acc, ax, True))


def sum_to_size(g, shape):
    """torch's broadcast reduction: sum `g` down to `shape` exactly as
    `torch.autograd.grad` does over broadcast axes. Extra leading gradient
    dimensions are reduced, every axis where `shape` has size 1 but `g` does not is summed
    (keeping the axis), and the result is reshaped to `shape`."""
    gs = g.shape
    if structuralEquals(gs, shape):
        return g
    else:
        extra = len(gs) - len(shape)
        reduced = fold(range(extra), g, lambda acc, _: Array.sum_axis(acc, 0, False))
        gap = len(shape) - len(reduced.shape)
        padded = fold(range(gap), reduced, lambda acc, _: Array.expand_dims(acc, 0))
        ps = padded.shape
        axes = filterList(toList(map(range(len(shape)), lambda i: i)), lambda i: unwrap_or(listGet(shape, i), 1) == 1 and unwrap_or(listGet(ps, i), 1) != 1)
        return Array.reshape(sum_axes(padded, axes), shape)
