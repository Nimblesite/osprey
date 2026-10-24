# osprey: flavor=python
# osprey: namespace=numpy
# osprey: module=Array
"""Row-major float64 ndarray: creation, shape, broadcasting, reductions.

`NDArray[B]` holds its elements in a `Storage.Buffer` interpretation `B`, so a
program installs `Host.on_host` (host lists) or `Device.on_gpu` (GPU surface)
and every operation here computes under that handler. Elements are float64
only; integer results (`argmax`, `size`, shapes) are `int`/`list[int]`.
Reductions accumulate in element order from the seed exactly as the GPU
interpretation does, so both handlers agree bit for bit. Implements
[PORT-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from typing import Callable

from numpy import Errors, Layout, Math, Storage

__all__ = [
    "NDArray", "scalar", "empty", "array", "array1", "array2", "array3",
    "zeros", "ones", "full", "zeros_like", "ones_like", "full_like", "arange",
    "linspace", "eye", "eye_m", "identity", "diag", "tri", "tri_k",
    "tolist", "flatten", "size", "ndim",
    "reshape", "ravel", "transpose", "transpose_all", "swapaxes", "moveaxis",
    "expand_dims", "squeeze", "squeeze_axis", "broadcast_shapes", "broadcast_to",
    "concatenate", "stack", "take", "flip", "flip_all", "roll", "repeat", "tile",
    "add", "subtract", "multiply", "divide", "maximum", "minimum", "where",
    "equal", "not_equal", "less", "less_equal", "greater", "greater_equal",
    "clip", "negative", "absolute", "exp", "log", "sqrt", "tanh", "square",
    "reciprocal", "sign",
    "sum", "sum_axis", "mean", "mean_axis", "max", "max_axis", "min", "min_axis",
    "prod", "prod_axis", "var", "var_axis", "std", "std_axis", "argmax",
    "argmax_axis", "argmin", "argmin_axis", "all", "all_axis", "any", "any_axis",
    "cumsum", "cumprod",
]


class NDArray[B]:
    """A row-major float64 array: its shape and the buffer holding its
    elements. `B` is the buffer representation the installed handler picked."""
    shape: list[int]
    data: B


def scalar(value):
    """A rank-0 array holding one element, which broadcasting spreads to any
    operand: `add(a, scalar(2.0))`."""
    return NDArray(shape=[], data=perform(Storage.Buffer.build(1, lambda i: value)))


def empty():
    """A zero-element rank-1 array, the validated-access fallback every
    `unwrap_or` over a list of arrays needs; never actually read."""
    return NDArray(shape=[0], data=perform(Storage.Buffer.build(0, lambda i: 0.0)))


# Creation ----------------------------------------------------------------

def array(values, shape):
    """An array from a flat list and an explicit shape, sized to match."""
    g = Errors.require(len(values) == Layout.flat_size(shape), Errors.reshape_failed(len(values), shape))
    return NDArray(shape=shape, data=Storage.lift(values))


def array1(values):
    """A rank-1 array from a flat list."""
    return array(values, [len(values)])


def array2(rows):
    """A rank-2 array from a list of equal-length rows."""
    n = len(rows)
    m = len(unwrap_or(listGet(rows, 0), []))
    g = Errors.require(fold(range(n), True, lambda ok, i: ok and len(unwrap_or(listGet(rows, i), [])) == m), Errors.inhomogeneous(1, [n]))
    flat = fold(range(n), [], lambda acc, i: listConcat(acc, unwrap_or(listGet(rows, i), [])))
    return array(flat, [n, m])


def array3(blocks):
    """A rank-3 array from a list of equal-shaped rank-2 blocks."""
    d0 = len(blocks)
    first = unwrap_or(listGet(blocks, 0), [])
    d1 = len(first)
    d2 = len(unwrap_or(listGet(first, 0), []))
    g = Errors.require(fold(range(d0), True, lambda ok, i: ok and len(unwrap_or(listGet(blocks, i), [])) == d1), Errors.inhomogeneous(1, [d0]))
    g2 = Errors.require(fold(range(d0), True, lambda ok, i: ok and fold(range(d1), True, lambda ok2, r: ok2 and len(unwrap_or(listGet(unwrap_or(listGet(blocks, i), []), r), [])) == d2)), Errors.inhomogeneous(2, [d0, d1]))
    flat = fold(range(d0), [], lambda acc, i: listConcat(acc, fold(range(d1), [], lambda acc2, r: listConcat(acc2, unwrap_or(listGet(unwrap_or(listGet(blocks, i), []), r), [])))))
    return array(flat, [d0, d1, d2])


def full(shape, value):
    """An array of `shape` with every element `value`."""
    return NDArray(shape=shape, data=perform(Storage.Buffer.build(Layout.flat_size(shape), lambda i: value)))


def zeros(shape):
    """An array of `shape` filled with 0.0."""
    return full(shape, 0.0)


def ones(shape):
    """An array of `shape` filled with 1.0."""
    return full(shape, 1.0)


def full_like(a, value):
    """An array shaped like `a` with every element `value`."""
    return full(a.shape, value)


def zeros_like(a):
    """An array shaped like `a` filled with 0.0."""
    return full(a.shape, 0.0)


def ones_like(a):
    """An array shaped like `a` filled with 1.0."""
    return full(a.shape, 1.0)


def arange(start, stop, step):
    """np.arange: `start + i*step` for `i` below `ceil((stop-start)/step)`,
    matching numpy's element count and values."""
    g = Errors.require(step != 0.0, "division by zero")
    n = Math.ceil_int((stop - start) / step)
    nn = n if n > 0 else 0
    return NDArray(shape=[nn], data=perform(Storage.Buffer.build(nn, lambda i: start + toFloat(i) * step)))


def linspace(start, stop, num):
    """np.linspace: `num` evenly spaced values including `stop`."""
    g = Errors.require(num >= 0, Errors.linspace_samples(num))
    if num == 0:
        return empty()
    elif num == 1:
        return array([start], [1])
    else:
        delta = (stop - start) / toFloat(num - 1)
        return NDArray(shape=[num], data=perform(Storage.Buffer.build(num, lambda i: start + delta * toFloat(i))))


def eye(n):
    """The `n` by `n` identity matrix."""
    g = Errors.require(n >= 0, Errors.negative_dimensions())
    return NDArray(shape=[n, n], data=perform(Storage.Buffer.build(n * n, lambda i: 1.0 if i // n == i % n else 0.0)))


def eye_m(n, m):
    """The `n` by `m` matrix with ones on the main diagonal."""
    g = Errors.require(n >= 0 and m >= 0, Errors.negative_dimensions())
    return NDArray(shape=[n, m], data=perform(Storage.Buffer.build(n * m, lambda i: 1.0 if i // m == i % m else 0.0)))


def identity(n):
    """The `n` by `n` identity matrix (numpy's `identity`)."""
    return eye(n)


def diag(a):
    """numpy's `diag`: a 1-D array becomes a diagonal matrix; a 2-D array's
    diagonal is extracted."""
    nd = len(a.shape)
    if nd == 1:
        n = unwrap_or(listGet(a.shape, 0), 0)
        return NDArray(shape=[n, n], data=perform(Storage.Buffer.build(n * n, lambda i: Storage.at(a.data, i // n) if i // n == i % n else 0.0)))
    else:
        m = unwrap_or(listGet(a.shape, 0), 0)
        n = unwrap_or(listGet(a.shape, 1), 0)
        k = m if m < n else n
        return NDArray(shape=[k], data=perform(Storage.Buffer.build(k, lambda i: Storage.at(a.data, i * (n + 1)))))


def tri(n):
    """numpy's `tri(n)`: an `n` by `n` matrix with ones on and below the
    main diagonal."""
    return tri_k(n, n, 0)


def tri_k(n, m, k):
    """numpy's `tri(n, m, k)`: an `n` by `m` matrix with ones where
    `column <= row + k`."""
    nn = n if n > 0 else 0
    mm = m if m > 0 else 0
    return NDArray(shape=[nn, mm], data=perform(Storage.Buffer.build(nn * mm, lambda i: 1.0 if i % mm <= i // mm + k else 0.0)))


# Introspection -----------------------------------------------------------

def tolist(a):
    """The elements as a flat host list of floats."""
    return Storage.lower(a.data)


def flatten(a):
    """The elements as a flat host list of floats (numpy's `flatten`)."""
    return Storage.lower(a.data)


def size(a):
    """The element count."""
    return Layout.flat_size(a.shape)


def ndim(a):
    """The number of dimensions."""
    return len(a.shape)


# Shape operations --------------------------------------------------------

def reshape(a, shape):
    """numpy's `reshape`, with one `-1` dimension allowed."""
    resolved = Layout.resolve_reshape(Layout.flat_size(a.shape), shape)
    return NDArray(shape=resolved, data=a.data)


def ravel(a):
    """A rank-1 view of `a`'s elements in row-major order."""
    return NDArray(shape=[Layout.flat_size(a.shape)], data=a.data)


def transpose_perm(a, axes):
    """A transpose along a validated axis permutation."""
    out_shape = Layout.permuted(a.shape, axes)
    read = Layout.permuted(Layout.strides_of(a.shape), axes)
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: Storage.at(a.data, Layout.offset_of(i, out_shape, read)))))


def transpose(a, axes):
    """numpy's `transpose`: a permutation of the axes."""
    return transpose_perm(a, Layout.normalize_axes(axes, len(a.shape)))


def transpose_all(a):
    """numpy's `a.T`: every axis reversed."""
    return transpose_perm(a, listReverse(toList(map(range(len(a.shape)), lambda i: i))))


def swapaxes(a, axis1, axis2):
    """numpy's `swapaxes`."""
    ax1 = Layout.normalize_axis(axis1, len(a.shape), "axis1")
    ax2 = Layout.normalize_axis(axis2, len(a.shape), "axis2")
    return transpose_perm(a, Layout.swap_perm(len(a.shape), ax1, ax2))


def moveaxis(a, source, dest):
    """numpy's `moveaxis`: move `source` axes to `dest` positions."""
    return transpose_perm(a, Layout.moveaxis_perm(len(a.shape), source, dest))


def expand_dims(a, axis):
    """numpy's `expand_dims`: a size-1 dimension inserted at `axis`."""
    ax = Layout.normalize_axis(axis, len(a.shape) + 1, "")
    return NDArray(shape=Layout.insert_at(a.shape, ax, 1), data=a.data)


def squeeze(a):
    """numpy's `squeeze` with no axis: every size-1 dimension removed."""
    return NDArray(shape=filterList(a.shape, lambda d: d != 1), data=a.data)


def squeeze_axis(a, axis):
    """numpy's `squeeze` of one axis, which must have size 1."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    g = Errors.require(unwrap_or(listGet(a.shape, ax), 1) == 1, Errors.squeeze_failed())
    return NDArray(shape=Layout.drop_at(a.shape, ax), data=a.data)


def broadcast_shapes(shapes):
    """np.broadcast_shapes over a list of shapes."""
    return Layout.broadcast_shapes(shapes)


def broadcast_to(a, shape):
    """np.broadcast_to: `a` read through broadcast strides over `shape`."""
    g = Errors.require(len(a.shape) <= len(shape), Errors.broadcast_to_dims())
    g2 = Errors.require(Layout.compatible(a.shape, shape), Errors.broadcast_to_failed(a.shape, shape))
    strides = Layout.broadcast_strides(a.shape, shape)
    return NDArray(shape=shape, data=perform(Storage.Buffer.build(Layout.flat_size(shape), lambda i: Storage.at(a.data, Layout.offset_of(i, shape, strides)))))


# Joining, moving and gathering -------------------------------------------

def concatenate(arrays, axis):
    """np.concatenate: join arrays along `axis`, which must match elsewhere."""
    g = Errors.require(len(arrays) >= 1, Errors.need_one_array("concatenate"))
    shapes = mapList(arrays, lambda a: a.shape)
    datas = mapList(arrays, lambda a: a.data)
    fb = empty().data
    first = unwrap_or(listGet(shapes, 0), [])
    nd = len(first)
    j = fold(range(len(shapes)), -1, lambda bad, t: t if bad < 0 and len(unwrap_or(listGet(shapes, t), [])) != nd else bad)
    g2 = Errors.require(j < 0, Errors.concat_dims_failed(0, nd, j, len(unwrap_or(listGet(shapes, j), []))))
    ax = Layout.normalize_axis(axis, nd, "")
    bad_d = fold(range(nd), -1, lambda bad, d: d if bad < 0 and d != ax and any_concat_row_differs(shapes, first, d) else bad)
    bad_j = concat_first_row_differs(shapes, first, bad_d)
    g3 = Errors.require(bad_d < 0, Errors.concat_size_failed(bad_d, 0, unwrap_or(listGet(first, bad_d), 0), bad_j, unwrap_or(listGet(unwrap_or(listGet(shapes, bad_j), []), bad_d), 0)))
    axis_sizes = mapList(shapes, lambda s: unwrap_or(listGet(s, ax), 0))
    out_shape = Layout.set_at(first, ax, fold(range(len(axis_sizes)), 0, lambda acc, t: acc + unwrap_or(listGet(axis_sizes, t), 0)))
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: concat_read(i, out_shape, datas, shapes, ax, axis_sizes, fb))))


def any_concat_row_differs(shapes, first, d):
    """Whether any input's size at dimension `d` differs from `first`'s."""
    return fold(range(len(shapes)), False, lambda any_bad, t: any_bad or unwrap_or(listGet(unwrap_or(listGet(shapes, t), []), d), 0) != unwrap_or(listGet(first, d), 0))


def concat_first_row_differs(shapes, first, d):
    """The first input (after index 0) whose size at `d` differs."""
    return fold(range(len(shapes)), -1, lambda bad, t: t if bad < 0 and unwrap_or(listGet(unwrap_or(listGet(shapes, t), []), d), 0) != unwrap_or(listGet(first, d), 0) else bad)


def concat_input_index(k, axis_sizes):
    """The input holding the `k`-th element along the concatenation axis."""
    return fold(range(len(axis_sizes)), -1, lambda found, t: t if found < 0 and k < concat_cum_before(t, axis_sizes) + unwrap_or(listGet(axis_sizes, t), 0) else found)


def concat_cum_before(j, axis_sizes):
    """The element count before input `j` along the concatenation axis."""
    return fold(range(j), 0, lambda acc, t: acc + unwrap_or(listGet(axis_sizes, t), 0))


def concat_read(i, out_shape, datas, shapes, ax, axis_sizes, fb):
    """One concatenate output element: locate the input, then read it."""
    coords = Layout.coords_of(i, out_shape)
    k = unwrap_or(listGet(coords, ax), 0)
    j = concat_input_index(k, axis_sizes)
    in_coords = Layout.set_at(coords, ax, k - concat_cum_before(j, axis_sizes))
    src = Layout.ravel_index(in_coords, unwrap_or(listGet(shapes, j), []))
    return Storage.at(unwrap_or(listGet(datas, j), fb), src)


def stack(arrays, axis):
    """np.stack: join equal-shaped arrays along a new axis of their count."""
    g = Errors.require(len(arrays) >= 1, Errors.need_one_array("stack"))
    shapes = mapList(arrays, lambda a: a.shape)
    datas = mapList(arrays, lambda a: a.data)
    fb = empty().data
    first = unwrap_or(listGet(shapes, 0), [])
    g2 = Errors.require(fold(range(len(shapes)), True, lambda ok, t: ok and structuralEquals(unwrap_or(listGet(shapes, t), []), first)), Errors.stack_shape_failed())
    ax = Layout.normalize_axis(axis, len(first) + 1, "")
    out_shape = Layout.insert_at(first, ax, len(arrays))
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: stack_read(i, out_shape, datas, first, ax, fb))))


def stack_read(i, out_shape, datas, first, ax, fb):
    """One stack output element: which input, then that input's element."""
    coords = Layout.coords_of(i, out_shape)
    j = unwrap_or(listGet(coords, ax), 0)
    src = Layout.ravel_index(Layout.drop_at(coords, ax), first)
    return Storage.at(unwrap_or(listGet(datas, j), fb), src)


def normalize_index(idx, n, ax):
    """A take index normalised into `[0, n)`, failing with numpy's text."""
    g = Errors.require(idx >= 0 - n and idx < n, Errors.index_out_of_bounds(idx, ax, n))
    return idx if idx >= 0 else idx + n


def take(a, indices, axis):
    """np.take: gather `indices` along `axis` (negative indices wrap)."""
    nd = len(a.shape)
    ax = Layout.normalize_axis(axis, nd, "")
    n = unwrap_or(listGet(a.shape, ax), 0)
    normed = toList(map(range(len(indices)), lambda k: normalize_index(unwrap_or(listGet(indices, k), 0), n, ax)))
    out_shape = Layout.set_at(a.shape, ax, len(indices))
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: take_read(i, out_shape, a.data, a.shape, normed, ax))))


def take_read(i, out_shape, data, shape, normed, ax):
    """One take output element."""
    coords = Layout.coords_of(i, out_shape)
    k = unwrap_or(listGet(coords, ax), 0)
    idx = unwrap_or(listGet(normed, k), 0)
    return Storage.at(data, Layout.ravel_index(Layout.set_at(coords, ax, idx), shape))


def flip(a, axis):
    """np.flip along one axis."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: flip_read(i, a, ax))))


def flip_read(i, a, ax):
    """One flip output element: the axis coordinate is mirrored."""
    coords = Layout.coords_of(i, a.shape)
    n = unwrap_or(listGet(a.shape, ax), 0)
    return Storage.at(a.data, Layout.ravel_index(Layout.set_at(coords, ax, n - 1 - unwrap_or(listGet(coords, ax), 0)), a.shape))


def flip_all(a):
    """np.flip with no axis: every dimension reversed."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: flip_all_read(i, a))))


def flip_all_read(i, a):
    """One flip-all output element: every coordinate is mirrored."""
    coords = Layout.coords_of(i, a.shape)
    mirrored = toList(map(range(len(a.shape)), lambda d: unwrap_or(listGet(a.shape, d), 0) - 1 - unwrap_or(listGet(coords, d), 0)))
    return Storage.at(a.data, Layout.ravel_index(mirrored, a.shape))


def roll(a, shift, axis):
    """np.roll along one axis."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: roll_read(i, a, ax, shift))))


def roll_read(i, a, ax, shift):
    """One roll output element: the axis coordinate is shifted modulo its
    size."""
    coords = Layout.coords_of(i, a.shape)
    n = unwrap_or(listGet(a.shape, ax), 0)
    moved = (((unwrap_or(listGet(coords, ax), 0) - shift) % n) + n) % n
    return Storage.at(a.data, Layout.ravel_index(Layout.set_at(coords, ax, moved), a.shape))


def repeat(a, repeats, axis):
    """np.repeat with a scalar count along one axis."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    n = unwrap_or(listGet(a.shape, ax), 0)
    out_shape = Layout.set_at(a.shape, ax, n * repeats)
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: repeat_read(i, out_shape, a, ax, repeats))))


def repeat_read(i, out_shape, a, ax, repeats):
    """One repeat output element: the axis coordinate is its source's
    `coord // repeats`."""
    coords = Layout.coords_of(i, out_shape)
    src = Layout.set_at(coords, ax, unwrap_or(listGet(coords, ax), 0) // repeats)
    return Storage.at(a.data, Layout.ravel_index(src, a.shape))


def tile(a, reps):
    """np.tile: repeat `a` `reps[d]` times along each dimension."""
    g = Errors.require(len(reps) >= len(a.shape), "tile reps must have at least as many dimensions as the array")
    gap = len(reps) - len(a.shape)
    pad = listConcat(toList(map(range(gap), lambda _: 1)), a.shape)
    out_shape = toList(map(range(len(reps)), lambda d: unwrap_or(listGet(pad, d), 1) * unwrap_or(listGet(reps, d), 1)))
    return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: tile_read(i, out_shape, a, gap))))


def tile_read(i, out_shape, a, gap):
    """One tile output element: each coordinate is its source's coordinate
    modulo the source dimension."""
    coords = Layout.coords_of(i, out_shape)
    src_coords = toList(map(range(len(a.shape)), lambda d: unwrap_or(listGet(coords, d + gap), 0) % unwrap_or(listGet(a.shape, d), 1)))
    return Storage.at(a.data, Layout.ravel_index(src_coords, a.shape))


# Elementwise -------------------------------------------------------------

def _arithmetic(op, x, y):
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
        case _:
            return Storage.min_f(x, y)


def _comparison(op, x, y):
    match op:
        case 6:
            return x == y
        case 7:
            return x != y
        case 8:
            return x < y
        case 9:
            return x <= y
        case 10:
            return x > y
        case _:
            return x >= y


def _binary_value(op, x, y):
    return _arithmetic(op, x, y) if op < 6 else (1.0 if _comparison(op, x, y) else 0.0)


def _binary(a, b, op):
    out = Layout.broadcast_two(a.shape, b.shape)
    sa = Layout.broadcast_strides(a.shape, out)
    sb = Layout.broadcast_strides(b.shape, out)
    return NDArray(shape=out, data=perform(Storage.Buffer.build(Layout.flat_size(out), lambda i: _binary_value(op, Storage.at(a.data, Layout.offset_of(i, out, sa)), Storage.at(b.data, Layout.offset_of(i, out, sb))))))


def add(a, b):
    """np.add, broadcast."""
    return _binary(a, b, 0)


def subtract(a, b):
    """np.subtract, broadcast."""
    return _binary(a, b, 1)


def multiply(a, b):
    """np.multiply, broadcast."""
    return _binary(a, b, 2)


def divide(a, b):
    """np.divide, broadcast. A zero divisor performs Arith.divideByZero."""
    return _binary(a, b, 3)


def maximum(a, b):
    """np.maximum, broadcast: NaN wins, else the larger."""
    return _binary(a, b, 4)


def minimum(a, b):
    """np.minimum, broadcast: NaN wins, else the smaller."""
    return _binary(a, b, 5)


def where(cond, a, b):
    """np.where(cond, a, b): `a` where cond is nonzero, else `b`, all
    broadcast together."""
    out = Layout.broadcast_shapes([cond.shape, a.shape, b.shape])
    sc = Layout.broadcast_strides(cond.shape, out)
    sa = Layout.broadcast_strides(a.shape, out)
    sb = Layout.broadcast_strides(b.shape, out)
    return NDArray(shape=out, data=perform(Storage.Buffer.build(Layout.flat_size(out), lambda i: Storage.at(a.data, Layout.offset_of(i, out, sa)) if Storage.at(cond.data, Layout.offset_of(i, out, sc)) != 0.0 else Storage.at(b.data, Layout.offset_of(i, out, sb)))))


def equal(a, b):
    """np.equal as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 6)


def not_equal(a, b):
    """np.not_equal as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 7)


def less(a, b):
    """np.less as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 8)


def less_equal(a, b):
    """np.less_equal as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 9)


def greater(a, b):
    """np.greater as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 10)


def greater_equal(a, b):
    """np.greater_equal as 0.0/1.0 floats, broadcast."""
    return _binary(a, b, 11)


def clip(a, lo, hi):
    """np.clip with scalar bounds."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Storage.min_f(Storage.max_f(Storage.at(a.data, i), lo), hi))))


def negative(a):
    """np.negative."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: 0.0 - Storage.at(a.data, i))))


def absolute(a):
    """np.absolute."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: abs(Storage.at(a.data, i)))))


def exp(a):
    """np.exp (elementwise natural exponential)."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Math.exp_of(Storage.at(a.data, i)))))


def log(a):
    """np.log (elementwise natural logarithm)."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Math.ln_of(Storage.at(a.data, i)))))


def sqrt(a):
    """np.sqrt."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Math.sqrt_of(Storage.at(a.data, i)))))


def tanh(a):
    """np.tanh."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Math.tanh_of(Storage.at(a.data, i)))))


def square(a):
    """np.square."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Storage.at(a.data, i) * Storage.at(a.data, i))))


def reciprocal(a):
    """np.reciprocal. A zero element performs Arith.divideByZero."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: 1.0 / Storage.at(a.data, i))))


def sign(a):
    """np.sign: -1.0, 0.0, 1.0, or NaN for NaN."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: 0.0 if Storage.at(a.data, i) == 0.0 else (1.0 if Storage.at(a.data, i) > 0.0 else (0.0 - 1.0 if Storage.at(a.data, i) < 0.0 else Storage.at(a.data, i))))))



# Reductions --------------------------------------------------------------

# Reduction combine tags: kernels may not call a dynamic function value (the
# GPU backend rejects a closure it cannot prove pure), so each reduction names
# its combine by an int tag that `combine_op` dispatches statically.
SUM_OP = 0
PROD_OP = 1
MAX_OP = 2
MIN_OP = 3
AND_OP = 4
OR_OP = 5


def combine_op(acc, x, op):
    """The reduction combine selected by an int tag."""
    match op:
        case 0:
            return Storage.add_f(acc, x)
        case 1:
            return Storage.mul_f(acc, x)
        case 2:
            return Storage.max_f(acc, x)
        case 3:
            return Storage.min_f(acc, x)
        case 4:
            return and_kernel(acc, x)
        case _:
            return or_kernel(acc, x)


def reduce_all(a, seed, op):
    """Reduce every element in row-major order under the combine `op`."""
    match op:
        case 0:
            return perform(Storage.Buffer.reduce(a.data, seed, Storage.add_f))
        case 1:
            return perform(Storage.Buffer.reduce(a.data, seed, Storage.mul_f))
        case 2:
            return perform(Storage.Buffer.reduce(a.data, seed, Storage.max_f))
        case 3:
            return perform(Storage.Buffer.reduce(a.data, seed, Storage.min_f))
        case 4:
            return perform(Storage.Buffer.reduce(a.data, seed, and_kernel))
        case _:
            return perform(Storage.Buffer.reduce(a.data, seed, or_kernel))


def reduced_coords(out_coords, ax, k, keepdims):
    """The source coordinates of reduction element `k`, given the output
    coordinates."""
    return Layout.set_at(out_coords, ax, k) if keepdims else Layout.insert_at(out_coords, ax, k)


def reduce_axis(a, axis, keepdims, seed, op):
    """Reduce along one axis into an NDArray, optionally keeping the axis."""
    shape = a.shape
    nd = len(shape)
    if nd == 0:
        return NDArray(shape=[], data=perform(Storage.Buffer.build(1, lambda i: reduce_all(a, seed, op))))
    else:
        ax = Layout.normalize_axis(axis, nd, "")
        n = unwrap_or(listGet(shape, ax), 1)
        out_shape = Layout.drop_at(shape, ax) if not keepdims else Layout.set_at(shape, ax, 1)
        return NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: reduce_axis_element(i, out_shape, a, shape, ax, n, keepdims, seed, op))))


def reduce_axis_element(i, out_shape, a, shape, ax, n, keepdims, seed, op):
    """One reduce_axis output element: fold over the reduced axis."""
    coords = Layout.coords_of(i, out_shape)
    return fold(range(n), seed, lambda acc, k: combine_op(acc, Storage.at(a.data, Layout.ravel_index(reduced_coords(coords, ax, k, keepdims), shape)), op))


def axis_size(a, axis):
    """The size of a normalised axis of `a`."""
    return unwrap_or(listGet(a.shape, Layout.axis_of(a.shape, axis)), 1)


def sum(a):
    """np.sum over all elements."""
    return reduce_all(a, 0.0, SUM_OP)


def sum_axis(a, axis, keepdims):
    """np.sum along `axis`."""
    return reduce_axis(a, axis, keepdims, 0.0, SUM_OP)


def mean(a):
    """np.mean over all elements."""
    return sum(a) / toFloat(Layout.flat_size(a.shape))


def mean_axis(a, axis, keepdims):
    """np.mean along `axis`."""
    s = reduce_axis(a, axis, keepdims, 0.0, SUM_OP)
    n = axis_size(a, axis)
    return scale(s, 1.0 / toFloat(n))


def scale(a, factor):
    """An elementwise scale of `a` by a scalar."""
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Storage.at(a.data, i) * factor)))


def max(a):
    """np.max over all elements (NaN wins)."""
    return reduce_all(a, 0.0 - Storage.infinity(), MAX_OP)


def max_axis(a, axis, keepdims):
    """np.max along `axis` (NaN wins)."""
    return reduce_axis(a, axis, keepdims, 0.0 - Storage.infinity(), MAX_OP)


def min(a):
    """np.min over all elements (NaN wins)."""
    return reduce_all(a, Storage.infinity(), MIN_OP)


def min_axis(a, axis, keepdims):
    """np.min along `axis` (NaN wins)."""
    return reduce_axis(a, axis, keepdims, Storage.infinity(), MIN_OP)


def prod(a):
    """np.prod over all elements."""
    return reduce_all(a, 1.0, PROD_OP)


def prod_axis(a, axis, keepdims):
    """np.prod along `axis`."""
    return reduce_axis(a, axis, keepdims, 1.0, PROD_OP)


def var(a, ddof):
    """np.var over all elements with `ddof` degrees of freedom."""
    n = Layout.flat_size(a.shape)
    m = mean(a)
    return Math.variance(sum(square(subtract(a, scalar(m)))), n, ddof)


def var_axis(a, axis, keepdims, ddof):
    """np.var along `axis` with `ddof` degrees of freedom."""
    m = mean_axis(a, axis, True)
    s = reduce_axis(square(subtract(a, m)), axis, keepdims, 0.0, SUM_OP)
    n = axis_size(a, axis)
    return NDArray(shape=s.shape, data=perform(Storage.Buffer.build(Layout.flat_size(s.shape), lambda i: Math.variance(Storage.at(s.data, i), n, ddof))))


def std(a, ddof):
    """np.std over all elements with `ddof` degrees of freedom."""
    return Math.sqrt_of(var(a, ddof))


def std_axis(a, axis, keepdims, ddof):
    """np.std along `axis` with `ddof` degrees of freedom."""
    return sqrt(var_axis(a, axis, keepdims, ddof))


def better_max(x, y):
    """Whether `x` should replace `y` as argmax: NaN counts as largest, and
    ties keep the earlier index."""
    return x != y and (x != x or x > y)


def better_min(x, y):
    """Whether `x` should replace `y` as argmin: NaN counts as smallest, and
    ties keep the earlier index."""
    return x != y and (x != x or x < y)


def argmax(a):
    """np.argmax over all elements: the flat index of the first maximum."""
    return fold(range(Layout.flat_size(a.shape)), 0, lambda best, i: i if better_max(Storage.at(a.data, i), Storage.at(a.data, best)) else best)


def argmin(a):
    """np.argmin over all elements: the flat index of the first minimum."""
    return fold(range(Layout.flat_size(a.shape)), 0, lambda best, i: i if better_min(Storage.at(a.data, i), Storage.at(a.data, best)) else best)


def argmax_axis(a, axis):
    """np.argmax along `axis`: the per-position index, as a flat int list over
    the reduced shape."""
    shape = a.shape
    nd = len(shape)
    if nd == 0:
        return [0]
    else:
        ax = Layout.normalize_axis(axis, nd, "")
        n = unwrap_or(listGet(shape, ax), 1)
        out_shape = Layout.drop_at(shape, ax)
        return toList(map(range(Layout.flat_size(out_shape)), lambda i: argreduce_at(i, out_shape, a, shape, ax, n, True)))


def argmin_axis(a, axis):
    """np.argmin along `axis`: the per-position index, as a flat int list over
    the reduced shape."""
    shape = a.shape
    nd = len(shape)
    if nd == 0:
        return [0]
    else:
        ax = Layout.normalize_axis(axis, nd, "")
        n = unwrap_or(listGet(shape, ax), 1)
        out_shape = Layout.drop_at(shape, ax)
        return toList(map(range(Layout.flat_size(out_shape)), lambda i: argreduce_at(i, out_shape, a, shape, ax, n, False)))


def argreduce_at(i, out_shape, a, shape, ax, n, want_max):
    """One argmax/argmin output: the index along `ax` of the extremum."""
    coords = Layout.coords_of(i, out_shape)
    return fold(range(n), 0, lambda best, k: k if arg_better(k, best, coords, a, shape, ax, want_max) else best)


def arg_better(k, best, coords, a, shape, ax, want_max):
    """Whether reduction index `k` beats `best` at this position."""
    x = Storage.at(a.data, Layout.ravel_index(Layout.insert_at(coords, ax, k), shape))
    y = Storage.at(a.data, Layout.ravel_index(Layout.insert_at(coords, ax, best), shape))
    return better_max(x, y) if want_max else better_min(x, y)


def and_kernel(acc, x):
    """Logical AND as 0.0/1.0 (nonzero is true, NaN is true)."""
    return 1.0 if acc != 0.0 and x != 0.0 else 0.0


def or_kernel(acc, x):
    """Logical OR as 0.0/1.0 (nonzero is true, NaN is true)."""
    return 1.0 if acc != 0.0 or x != 0.0 else 0.0


def all(a):
    """np.all over all elements (bool)."""
    return reduce_all(a, 1.0, AND_OP) != 0.0


def all_axis(a, axis, keepdims):
    """np.all along `axis`, as 0.0/1.0 floats."""
    return reduce_axis(a, axis, keepdims, 1.0, AND_OP)


def any(a):
    """np.any over all elements (bool)."""
    return reduce_all(a, 0.0, OR_OP) != 0.0


def any_axis(a, axis, keepdims):
    """np.any along `axis`, as 0.0/1.0 floats."""
    return reduce_axis(a, axis, keepdims, 0.0, OR_OP)


def cumsum(a, axis):
    """np.cumsum along `axis`."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: cum_read(i, a, ax, SUM_OP))))


def cumprod(a, axis):
    """np.cumprod along `axis`."""
    ax = Layout.normalize_axis(axis, len(a.shape), "")
    return NDArray(shape=a.shape, data=perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: cum_read(i, a, ax, PROD_OP))))


def cum_read(i, a, ax, op):
    """One cumsum/cumprod output: the prefix over the axis up to this
    coordinate."""
    coords = Layout.coords_of(i, a.shape)
    k = unwrap_or(listGet(coords, ax), 0)
    seed = 0.0 if op == SUM_OP else 1.0
    return fold(range(k + 1), seed, lambda acc, t: combine_op(acc, Storage.at(a.data, Layout.ravel_index(Layout.set_at(coords, ax, t), a.shape)), op))
