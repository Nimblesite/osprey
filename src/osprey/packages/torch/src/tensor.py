# osprey: flavor=python
# osprey: namespace=torch
# osprey: module=Tensor
"""Shared float64 tensor helpers the autograd tape and the optimizers build on.

A torch tensor is a `numpy.Array.NDArray[B]` — a float64 array over the buffer
representation `B` the installed numpy handler picks — so one body computes on
host lists or the GPU by handler [PORT-DEVICE]. This module adds the pieces
numpy does not provide: integer powers, scalar transforms, and `sum_to_size`,
the broadcast reduction torch's autograd applies to every incoming gradient.
Shape, stride and broadcast operations use the shared NumPy implementation.
"""
from numpy import Array, Layout, Storage

__all__ = [
    "pow_f", "scale", "add_scalar", "sub_scalar", "sub_from", "div_scalar", "max_scalar",
    "one_minus", "flat", "strides", "src_offset", "bcast_shapes2", "combine_op",
    "broadcast_to", "badd", "bsub", "bmul", "bdiv", "bmax", "bmin", "gt_zero",
    "sum_axes", "sum_to_size",
]


def pow_f(base, n):
    """`base` raised to a non-negative integer power, by repeated
    multiplication. Matches CPython's `base ** n` to within a few ulps, well inside the
    1e-12 rtol the optimizers compare at."""
    return fold(range(n), 1.0, lambda acc, _: acc * base)


flat = Layout.flat_size
"""The number of elements a shape holds (1 for rank 0), the product of
its dimensions."""


strides = Layout.strides_of
"""Row-major strides of a shape."""


def src_offset(i, out_shape, src_shape):
    """A broadcast element's source offset using the shared NumPy layout."""
    return Layout.offset_of(i, out_shape, Layout.broadcast_strides(src_shape, out_shape))


bcast_shapes2 = Layout.broadcast_two
"""The broadcast of two shapes (right-aligned; assumes they are
compatible, as the port's ops keep them)."""


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


broadcast_to = Array.broadcast_to
"""Broadcast an array through the shared NumPy shape and stride rules."""


badd = Array.add
"""Broadcast add."""


bsub = Array.subtract
"""Broadcast subtract."""


bmul = Array.multiply
"""Broadcast multiply."""


bdiv = Array.divide
"""Broadcast divide."""


bmax = Array.maximum
"""Broadcast maximum (NaN wins, as numpy's maximum)."""


bmin = Array.minimum
"""Broadcast minimum (NaN wins, as numpy's minimum)."""


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
