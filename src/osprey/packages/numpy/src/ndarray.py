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
    return full([], value)

def empty():
    """A zero-element rank-1 array, the validated-access fallback every
    `unwrap_or` over a list of arrays needs; never actually read."""
    return full([0], 0.0)

# Creation ----------------------------------------------------------------

def array(values, shape):
    """An array from a flat list and an explicit shape, sized to match."""
    g = Errors.require(len(values) == Layout.flat_size(shape), Errors.reshape_failed(len(values), shape))
    return NDArray(shape=shape, data=Storage.lift(values))

def array1(values):
    """A rank-1 array from a flat list."""
    return array(values, [len(values)])

def uniform_rows(rows, count, width):
    """Whether each of `count` rows holds exactly `width` elements."""
    return fold(range(count), True, lambda ok, i: ok and len(unwrap_or(listGet(rows, i), [])) == width)

def uniform_blocks(blocks, d0, d1, d2):
    """Whether each of `d0` blocks holds `d1` rows of exactly `d2` elements."""
    return fold(range(d0), True, lambda ok, i: ok and uniform_rows(unwrap_or(listGet(blocks, i), []), d1, d2))

def concat_each(rows, count):
    """The flat list of the first `count` lists in `rows`, concatenated."""
    return fold(range(count), [], lambda acc, i: listConcat(acc, unwrap_or(listGet(rows, i), [])))

def array2(rows):
    """A rank-2 array from a list of equal-length rows."""
    n = len(rows)
    m = len(unwrap_or(listGet(rows, 0), []))
    g = Errors.require(uniform_rows(rows, n, m), Errors.inhomogeneous(1, [n]))
    return array(concat_each(rows, n), [n, m])

def array3(blocks):
    """A rank-3 array from a list of equal-shaped rank-2 blocks."""
    d0 = len(blocks)
    first = unwrap_or(listGet(blocks, 0), [])
    d1 = len(first)
    d2 = len(unwrap_or(listGet(first, 0), []))
    g = Errors.require(uniform_rows(blocks, d0, d1), Errors.inhomogeneous(1, [d0]))
    g2 = Errors.require(uniform_blocks(blocks, d0, d1, d2), Errors.inhomogeneous(2, [d0, d1]))
    flats = mapList(blocks, lambda b: concat_each(b, d1))
    return array(concat_each(flats, d0), [d0, d1, d2])

def over(shape, built_result):
    """An array of `shape` over an inline kernel's `Buffer.build` result: the
    one construction path every shaped operation shares. The kernel lambda
    stays at its caller, where the GPU gate can prove it pure."""
    return NDArray(shape=shape, data=Storage.built(built_result))

def full(shape, value):
    """An array of `shape` with every element `value`."""
    return over(shape, perform(Storage.Buffer.build(Layout.flat_size(shape), lambda i: value)))

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
    return full_like(a, 0.0)

def ones_like(a):
    """An array shaped like `a` filled with 1.0."""
    return full_like(a, 1.0)

def ramp(n, start, delta):
    """A rank-1 array of `n` values `start + i*delta`: the element rule arange
    and linspace share."""
    return over([n], perform(Storage.Buffer.build(Layout.flat_size([n]), lambda i: start + toFloat(i) * delta)))

def arange(start, stop, step):
    """np.arange: `start + i*step` for `i` below `ceil((stop-start)/step)`,
    matching numpy's element count and values."""
    g = Errors.require(step != 0.0, "division by zero")
    n = Math.ceil_int((stop - start) / step)
    return ramp(n if n > 0 else 0, start, step)

def linspace(start, stop, num):
    """np.linspace: `num` evenly spaced values including `stop`."""
    g = Errors.require(num >= 0, Errors.linspace_samples(num))
    if num == 0:
        return empty()
    elif num == 1:
        return array([start], [1])
    else:
        return ramp(num, start, (stop - start) / toFloat(num - 1))

def eye(n):
    """The `n` by `n` identity matrix."""
    return eye_m(n, n)

def eye_m(n, m):
    """The `n` by `m` matrix with ones on the main diagonal."""
    g = Errors.require(n >= 0 and m >= 0, Errors.negative_dimensions())
    return over([n, m], perform(Storage.Buffer.build(Layout.flat_size([n, m]), lambda i: 1.0 if i // m == i % m else 0.0)))

def identity(n):
    """The `n` by `n` identity matrix (numpy's `identity`)."""
    return eye(n)

def diag(a):
    """numpy's `diag`: a 1-D array becomes a diagonal matrix; a 2-D array's
    diagonal is extracted."""
    nd = len(a.shape)
    if nd == 1:
        n = unwrap_or(listGet(a.shape, 0), 0)
        return over([n, n], perform(Storage.Buffer.build(Layout.flat_size([n, n]), lambda i: Storage.at(a.data, i // n) if i // n == i % n else 0.0)))
    else:
        n = Layout.second_dim(a.shape)
        k = Layout.diag_extent(a.shape)
        return over([k], perform(Storage.Buffer.build(Layout.flat_size([k]), lambda i: Storage.at(a.data, i * (n + 1)))))

def tri(n):
    """numpy's `tri(n)`: an `n` by `n` matrix with ones on and below the
    main diagonal."""
    return tri_k(n, n, 0)

def tri_k(n, m, k):
    """numpy's `tri(n, m, k)`: an `n` by `m` matrix with ones where
    `column <= row + k`."""
    nn = n if n > 0 else 0
    mm = m if m > 0 else 0
    return over([nn, mm], perform(Storage.Buffer.build(Layout.flat_size([nn, mm]), lambda i: 1.0 if i % mm <= i // mm + k else 0.0)))

# Introspection -----------------------------------------------------------

def tolist(a):
    """The elements as a flat host list of floats."""
    return Storage.lower(a.data)

def flatten(a):
    """The elements as a flat host list of floats (numpy's `flatten`)."""
    return tolist(a)

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

def strided(a, shape, strides):
    """`a` read through `strides` over `shape`: the shared view construction
    of `transpose_perm` and `broadcast_to`."""
    return over(shape, perform(Storage.Buffer.build(Layout.flat_size(shape), lambda i: Storage.at(a.data, Layout.offset_of(i, shape, strides)))))

def transpose_perm(a, axes):
    """A transpose along a validated axis permutation."""
    out_shape = Layout.permuted(a.shape, axes)
    read = Layout.permuted(Layout.strides_of(a.shape), axes)
    return strided(a, out_shape, read)

def transpose(a, axes):
    """numpy's `transpose`: a permutation of the axes."""
    return transpose_perm(a, Layout.normalize_axes(axes, len(a.shape)))

def transpose_all(a):
    """numpy's `a.T`: every axis reversed."""
    return transpose_perm(a, listReverse(Layout.map_indexed(a.shape, lambda i: i)))

def along(a, axis):
    """The axis `axis` names for `a`, normalised (a negative axis counts from
    the last)."""
    return Layout.normalize_axis(axis, len(a.shape), "")


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
    ax = along(a, axis)
    g = Errors.require(unwrap_or(listGet(a.shape, ax), 1) == 1, Errors.squeeze_failed())
    return NDArray(shape=Layout.drop_at(a.shape, ax), data=a.data)

def broadcast_shapes(shapes):
    """np.broadcast_shapes over a list of shapes."""
    return Layout.broadcast_shapes(shapes)

def broadcast_to(a, shape):
    """np.broadcast_to: `a` read through broadcast strides over `shape`."""
    g = Errors.require(len(a.shape) <= len(shape), Errors.broadcast_to_dims())
    g2 = Errors.require(Layout.compatible(a.shape, shape), Errors.broadcast_to_failed(a.shape, shape))
    return strided(a, shape, Layout.broadcast_strides(a.shape, shape))

# Joining, moving and gathering -------------------------------------------

class ArrayInputs[B]:
    """What a joining operation opens with: the per-input shapes and buffers,
    the first input's shape, and the empty fallback buffer for validated
    reads."""
    shapes: list[list[int]]
    datas: list[B]
    first: list[int]
    fallback: B

def checked_inputs(name, arrays):
    """The `ArrayInputs` of the joining operation named `name`, which needs at
    least one input array."""
    g = Errors.require(len(arrays) >= 1, Errors.need_one_array(name))
    shapes = mapList(arrays, lambda a: a.shape)
    return ArrayInputs(shapes=shapes, datas=mapList(arrays, lambda a: a.data), first=unwrap_or(listGet(shapes, 0), []), fallback=empty().data)

def read_at_axis(data, shape, coords, ax, k):
    """The element of `data` at `coords` with the coordinate at `ax` replaced
    by `k`: the shared read of every gather that rewrites one axis."""
    return Storage.at(data, Layout.ravel_index(Layout.set_at(coords, ax, k), shape))

def dim_of(shape, d):
    """The `d`-th dimension of `shape`, 0 when it has none."""
    return unwrap_or(listGet(shape, d), 0)

def input_rank(inputs, t):
    """The number of dimensions input `t` of `inputs` has."""
    return len(unwrap_or(listGet(inputs.shapes, t), []))

def concatenate(arrays, axis):
    """np.concatenate: join arrays along `axis`, which must match elsewhere."""
    inputs = checked_inputs("concatenate", arrays)
    nd = len(inputs.first)
    j = fold(range(len(inputs.shapes)), -1, lambda bad, t: t if bad < 0 and input_rank(inputs, t) != nd else bad)
    g2 = Errors.require(j < 0, Errors.concat_dims_failed(0, nd, j, input_rank(inputs, j)))
    ax = Layout.normalize_axis(axis, nd, "")
    bad_d = fold(range(nd), -1, lambda bad, d: d if bad < 0 and d != ax and any_concat_row_differs(inputs.shapes, inputs.first, d) else bad)
    bad_j = concat_first_row_differs(inputs.shapes, inputs.first, bad_d)
    g3 = Errors.require(bad_d < 0, Errors.concat_size_failed(bad_d, 0, dim_of(inputs.first, bad_d), bad_j, dim_of(unwrap_or(listGet(inputs.shapes, bad_j), []), bad_d)))
    axis_sizes = mapList(inputs.shapes, lambda s: dim_of(s, ax))
    out_shape = Layout.set_at(inputs.first, ax, total_ints(axis_sizes, len(axis_sizes)))
    return over(out_shape, perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: concat_read(i, out_shape, inputs, ax, axis_sizes))))

def any_concat_row_differs(shapes, first, d):
    """Whether any input's size at dimension `d` differs from `first`'s."""
    return concat_first_row_differs(shapes, first, d) >= 0

def concat_first_row_differs(shapes, first, d):
    """The first input (after index 0) whose size at `d` differs."""
    return fold(range(len(shapes)), -1, lambda bad, t: t if bad < 0 and dim_of(unwrap_or(listGet(shapes, t), []), d) != dim_of(first, d) else bad)

def total_ints(xs, count):
    """The sum of the first `count` ints in `xs`."""
    return fold(range(count), 0, lambda acc, t: acc + unwrap_or(listGet(xs, t), 0))

def concat_input_index(k, axis_sizes):
    """The input holding the `k`-th element along the concatenation axis."""
    return fold(range(len(axis_sizes)), -1, lambda found, t: t if found < 0 and k < concat_cum_before(t, axis_sizes) + dim_of(axis_sizes, t) else found)

def concat_cum_before(j, axis_sizes):
    """The element count before input `j` along the concatenation axis."""
    return total_ints(axis_sizes, j)

def concat_read(i, out_shape, inputs, ax, axis_sizes):
    """One concatenate output element: locate the input, then read it."""
    coords = Layout.coords_of(i, out_shape)
    k = axis_coord(coords, ax)
    j = concat_input_index(k, axis_sizes)
    return read_at_axis(unwrap_or(listGet(inputs.datas, j), inputs.fallback), unwrap_or(listGet(inputs.shapes, j), []), coords, ax, k - concat_cum_before(j, axis_sizes))

def stack(arrays, axis):
    """np.stack: join equal-shaped arrays along a new axis of their count."""
    inputs = checked_inputs("stack", arrays)
    g2 = Errors.require(fold(range(len(inputs.shapes)), True, lambda ok, t: ok and structuralEquals(unwrap_or(listGet(inputs.shapes, t), []), inputs.first)), Errors.stack_shape_failed())
    ax = Layout.normalize_axis(axis, len(inputs.first) + 1, "")
    out_shape = Layout.insert_at(inputs.first, ax, len(arrays))
    return over(out_shape, perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: stack_read(i, out_shape, inputs, ax))))

def stack_read(i, out_shape, inputs, ax):
    """One stack output element: which input, then that input's element."""
    coords = Layout.coords_of(i, out_shape)
    j = unwrap_or(listGet(coords, ax), 0)
    src = Layout.ravel_index(Layout.drop_at(coords, ax), inputs.first)
    return Storage.at(unwrap_or(listGet(inputs.datas, j), inputs.fallback), src)

def normalize_index(idx, n, ax):
    """A take index normalised into `[0, n)`, failing with numpy's text."""
    g = Errors.require(idx >= 0 - n and idx < n, Errors.index_out_of_bounds(idx, ax, n))
    return idx if idx >= 0 else idx + n

def take(a, indices, axis):
    """np.take: gather `indices` along `axis` (negative indices wrap)."""
    nd = len(a.shape)
    ax = Layout.normalize_axis(axis, nd, "")
    n = unwrap_or(listGet(a.shape, ax), 0)
    normed = Layout.map_indexed(indices, lambda k: normalize_index(unwrap_or(listGet(indices, k), 0), n, ax))
    out_shape = Layout.set_at(a.shape, ax, len(indices))
    return over(out_shape, perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: take_read(i, out_shape, a.data, a.shape, normed, ax))))

def take_read(i, out_shape, data, shape, normed, ax):
    """One take output element."""
    coords = Layout.coords_of(i, out_shape)
    k = axis_coord(coords, ax)
    idx = unwrap_or(listGet(normed, k), 0)
    return read_at_axis(data, shape, coords, ax, idx)

# Axis-rewriting read tags: a kernel may not call a dynamic function value
# (the GPU backend rejects a closure it cannot prove pure), so flip, roll and
# the cumulative prefix name their read by an int tag that `axis_read`
# dispatches statically; `arg` carries the roll shift or combine tag.
FLIP_OP = 0
ROLL_OP = 1
CUM_OP = 2

def axis_gather(a, axis, op, arg):
    """An array shaped like `a` whose elements come from the axis-rewriting
    read that `op` names, over the axis `axis` names."""
    ax = along(a, axis)
    return over(a.shape, perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: axis_read(i, a, ax, op, arg))))

def axis_read(i, a, ax, op, arg):
    """The axis-rewriting kernel selected by an int tag."""
    match op:
        case 0:
            return moved_read(i, a, ax, FLIP_OP, arg)
        case 1:
            return moved_read(i, a, ax, ROLL_OP, arg)
        case _:
            return cum_read(i, a, ax, arg)

def axis_coord(coords, ax):
    """The coordinate along `ax` of one element's coordinates."""
    return unwrap_or(listGet(coords, ax), 0)

def moved_read(i, a, ax, op, arg):
    """One flip or roll output element: the axis coordinate is mirrored
    (FLIP_OP) or shifted by `arg` modulo the axis size (ROLL_OP)."""
    coords = Layout.coords_of(i, a.shape)
    n = unwrap_or(listGet(a.shape, ax), 0)
    c = axis_coord(coords, ax)
    moved = n - 1 - c if op == FLIP_OP else (((c - arg) % n) + n) % n
    return read_at_axis(a.data, a.shape, coords, ax, moved)

def flip(a, axis):
    """np.flip along one axis."""
    return axis_gather(a, axis, FLIP_OP, 0)

def flip_all(a):
    """np.flip with no axis: every dimension reversed."""
    return over(a.shape, perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: flip_all_read(i, a))))

def flip_all_read(i, a):
    """One flip-all output element: every coordinate is mirrored."""
    coords = Layout.coords_of(i, a.shape)
    mirrored = Layout.map_indexed(a.shape, lambda d: unwrap_or(listGet(a.shape, d), 0) - 1 - unwrap_or(listGet(coords, d), 0))
    return Storage.at(a.data, Layout.ravel_index(mirrored, a.shape))

def roll(a, shift, axis):
    """np.roll along one axis."""
    return axis_gather(a, axis, ROLL_OP, shift)

def repeat(a, repeats, axis):
    """np.repeat with a scalar count along one axis."""
    ax = along(a, axis)
    n = unwrap_or(listGet(a.shape, ax), 0)
    out_shape = Layout.set_at(a.shape, ax, n * repeats)
    return over(out_shape, perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: repeat_read(i, out_shape, a, ax, repeats))))

def repeat_read(i, out_shape, a, ax, repeats):
    """One repeat output element: the axis coordinate is its source's
    `coord // repeats`."""
    coords = Layout.coords_of(i, out_shape)
    return read_at_axis(a.data, a.shape, coords, ax, axis_coord(coords, ax) // repeats)

def tile(a, reps):
    """np.tile: repeat `a` `reps[d]` times along each dimension."""
    g = Errors.require(len(reps) >= len(a.shape), "tile reps must have at least as many dimensions as the array")
    gap = len(reps) - len(a.shape)
    pad = listConcat(toList(map(range(gap), lambda _: 1)), a.shape)
    out_shape = Layout.map_indexed(reps, lambda d: unwrap_or(listGet(pad, d), 1) * unwrap_or(listGet(reps, d), 1))
    return over(out_shape, perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: tile_read(i, out_shape, a, gap))))

def tile_read(i, out_shape, a, gap):
    """One tile output element: each coordinate is its source's coordinate
    modulo the source dimension."""
    coords = Layout.coords_of(i, out_shape)
    src_coords = Layout.map_indexed(a.shape, lambda d: unwrap_or(listGet(coords, d + gap), 0) % unwrap_or(listGet(a.shape, d), 1))
    return Storage.at(a.data, Layout.ravel_index(src_coords, a.shape))

# Elementwise -------------------------------------------------------------

class BinaryLayout:
    shape: list[int]
    left: list[int]
    right: list[int]

def binary_layout(a, b):
    """Share the output shape and strides of an elementwise binary operation."""
    shape = Layout.broadcast_two(a.shape, b.shape)
    return BinaryLayout(shape=shape, left=Layout.broadcast_strides(a.shape, shape), right=Layout.broadcast_strides(b.shape, shape))

# Elementwise kernel tags. A kernel may not call a dynamic function value (the
# GPU backend rejects a closure it cannot prove pure), so each operation names
# its kernel by an int tag that `pair_op` and `unary_op` dispatch statically.
ADD_OP = 0
SUBTRACT_OP = 1
MULTIPLY_OP = 2
DIVIDE_OP = 3
MAXIMUM_OP = 4
MINIMUM_OP = 5
EQUAL_OP = 6
NOT_EQUAL_OP = 7
LESS_OP = 8
LESS_EQUAL_OP = 9
GREATER_OP = 10
GREATER_EQUAL_OP = 11
COMPARE_OPS = EQUAL_OP
NEGATE_OP = 12
ABSOLUTE_OP = 13
SQUARE_OP = 14
RECIPROCAL_OP = 15
SIGN_OP = 16
EXP_OP = 17
LOG_OP = 18
SQRT_OP = 19
TANH_OP = 20
TRANSCENDENTAL_OPS = EXP_OP

def pair_op(x, y, op):
    """The elementwise binary kernel selected by an int tag."""
    return compare_op(x, y, op) if op >= COMPARE_OPS else arith_op(x, y, op)

def arith_op(x, y, op):
    """The elementwise arithmetic kernel selected by an int tag."""
    match op:
        case 0:
            return Storage.add_f(x, y)
        case 1:
            return x - y
        case 2:
            return Storage.mul_f(x, y)
        case 3:
            return x / y
        case 4:
            return Storage.max_f(x, y)
        case _:
            return Storage.min_f(x, y)

def compare_op(x, y, op):
    """The elementwise 0.0/1.0 comparison kernel selected by an int tag."""
    match op:
        case 6:
            return 1.0 if x == y else 0.0
        case 7:
            return 0.0 if x == y else 1.0
        case 8:
            return 1.0 if x < y else 0.0
        case 9:
            return 1.0 if x <= y else 0.0
        case 10:
            return 1.0 if x > y else 0.0
        case _:
            return 1.0 if x >= y else 0.0

def element_at(a, shape, strides, i):
    """The element of `a` at output index `i`, read through `strides` over
    `shape`: 0 strides repeat a broadcast element."""
    return Storage.at(a.data, Layout.offset_of(i, shape, strides))

def pair(a, b, op):
    """An elementwise binary result over the broadcast shape, under the kernel
    `op` names."""
    plan = binary_layout(a, b)
    return over(plan.shape, perform(Storage.Buffer.build(Layout.flat_size(plan.shape), lambda i: pair_op(element_at(a, plan.shape, plan.left, i), element_at(b, plan.shape, plan.right, i), op))))

def add(a, b):
    """np.add, broadcast."""
    return pair(a, b, ADD_OP)

def subtract(a, b):
    """np.subtract, broadcast."""
    return pair(a, b, SUBTRACT_OP)

def multiply(a, b):
    """np.multiply, broadcast."""
    return pair(a, b, MULTIPLY_OP)

def divide(a, b):
    """np.divide, broadcast. A zero divisor performs Arith.divideByZero."""
    return pair(a, b, DIVIDE_OP)

def maximum(a, b):
    """np.maximum, broadcast: NaN wins, else the larger."""
    return pair(a, b, MAXIMUM_OP)

def minimum(a, b):
    """np.minimum, broadcast: NaN wins, else the smaller."""
    return pair(a, b, MINIMUM_OP)

def where(cond, a, b):
    """np.where(cond, a, b): `a` where cond is nonzero, else `b`, all
    broadcast together."""
    out = Layout.broadcast_shapes([cond.shape, a.shape, b.shape])
    sc = Layout.broadcast_strides(cond.shape, out)
    sa = Layout.broadcast_strides(a.shape, out)
    sb = Layout.broadcast_strides(b.shape, out)
    return over(out, perform(Storage.Buffer.build(Layout.flat_size(out), lambda i: element_at(a, out, sa, i) if element_at(cond, out, sc, i) != 0.0 else element_at(b, out, sb, i))))

def equal(a, b):
    """np.equal as 0.0/1.0 floats, broadcast."""
    return pair(a, b, EQUAL_OP)

def not_equal(a, b):
    """np.not_equal as 0.0/1.0 floats, broadcast."""
    return pair(a, b, NOT_EQUAL_OP)

def less(a, b):
    """np.less as 0.0/1.0 floats, broadcast."""
    return pair(a, b, LESS_OP)

def less_equal(a, b):
    """np.less_equal as 0.0/1.0 floats, broadcast."""
    return pair(a, b, LESS_EQUAL_OP)

def greater(a, b):
    """np.greater as 0.0/1.0 floats, broadcast."""
    return pair(a, b, GREATER_OP)

def greater_equal(a, b):
    """np.greater_equal as 0.0/1.0 floats, broadcast."""
    return pair(a, b, GREATER_EQUAL_OP)

def clip(a, lo, hi):
    """np.clip with scalar bounds."""
    return over(a.shape, perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Storage.min_f(Storage.max_f(Storage.at(a.data, i), lo), hi))))

def transcendental_op(x, op):
    """The elementwise transcendental kernel selected by an int tag."""
    match op:
        case 17:
            return Math.exp_of(x)
        case 18:
            return Math.ln_of(x)
        case 19:
            return Math.sqrt_of(x)
        case _:
            return Math.tanh_of(x)

def unary_op(x, op):
    """The elementwise unary kernel selected by an int tag."""
    return transcendental_op(x, op) if op >= TRANSCENDENTAL_OPS else algebraic_op(x, op)

def algebraic_op(x, op):
    """The elementwise algebraic kernel selected by an int tag."""
    match op:
        case 12:
            return 0.0 - x
        case 13:
            return abs(x)
        case 14:
            return x * x
        case 15:
            return 1.0 / x
        case _:
            return 0.0 if x == 0.0 else (1.0 if x > 0.0 else (0.0 - 1.0 if x < 0.0 else x))

def unary(a, op):
    """An elementwise unary result shaped like `a`, under the kernel `op`
    names."""
    return over(a.shape, perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: unary_op(Storage.at(a.data, i), op))))

def negative(a):
    """np.negative."""
    return unary(a, NEGATE_OP)

def absolute(a):
    """np.absolute."""
    return unary(a, ABSOLUTE_OP)

def exp(a):
    """np.exp (elementwise natural exponential)."""
    return unary(a, EXP_OP)

def log(a):
    """np.log (elementwise natural logarithm)."""
    return unary(a, LOG_OP)

def sqrt(a):
    """np.sqrt."""
    return unary(a, SQRT_OP)

def tanh(a):
    """np.tanh."""
    return unary(a, TANH_OP)

def square(a):
    """np.square."""
    return unary(a, SQUARE_OP)

def reciprocal(a):
    """np.reciprocal. A zero element performs Arith.divideByZero."""
    return unary(a, RECIPROCAL_OP)

def sign(a):
    """np.sign: -1.0, 0.0, 1.0, or NaN for NaN."""
    return unary(a, SIGN_OP)

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
    """Reduce every element in row-major order under the combine `op`, or leave
    the checked region with the runtime's reason."""
    match op:
        case 0:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, Storage.add_f)))
        case 1:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, Storage.mul_f)))
        case 2:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, Storage.max_f)))
        case 3:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, Storage.min_f)))
        case 4:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, and_kernel)))
        case _:
            return Storage.reduced(perform(Storage.Buffer.reduce(a.data, seed, or_kernel)))

def reduced_coords(out_coords, ax, k, keepdims):
    """The source coordinates of reduction element `k`, given the output
    coordinates."""
    return Layout.set_at(out_coords, ax, k) if keepdims else Layout.insert_at(out_coords, ax, k)

class AxisPlan:
    """A reduction's normalised axis, its size, and the shape its result takes."""
    ax: int
    n: int
    out_shape: list[int]

def axis_plan(shape, axis, keepdims):
    """The normalised axis `axis` names over `shape`, its size, and the reduced
    shape (the axis set to 1 when kept)."""
    ax = Layout.normalize_axis(axis, len(shape), "")
    out_shape = Layout.set_at(shape, ax, 1) if keepdims else Layout.drop_at(shape, ax)
    return AxisPlan(ax=ax, n=unwrap_or(listGet(shape, ax), 1), out_shape=out_shape)

def reduce_axis(a, axis, keepdims, seed, op):
    """Reduce along one axis into an NDArray, optionally keeping the axis."""
    shape = a.shape
    if len(shape) == 0:
        total = reduce_all(a, seed, op)
        return over([], perform(Storage.Buffer.build(Layout.flat_size([]), lambda i: total)))
    else:
        plan = axis_plan(shape, axis, keepdims)
        return over(plan.out_shape, perform(Storage.Buffer.build(Layout.flat_size(plan.out_shape), lambda i: reduce_axis_element(i, plan.out_shape, a, shape, plan.ax, plan.n, keepdims, seed, op))))

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
    return over(a.shape, perform(Storage.Buffer.build(Layout.flat_size(a.shape), lambda i: Storage.at(a.data, i) * factor)))

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
    return over(s.shape, perform(Storage.Buffer.build(Layout.flat_size(s.shape), lambda i: Math.variance(Storage.at(s.data, i), n, ddof))))

def std(a, ddof):
    """np.std over all elements with `ddof` degrees of freedom."""
    return Math.sqrt_of(var(a, ddof))

def std_axis(a, axis, keepdims, ddof):
    """np.std along `axis` with `ddof` degrees of freedom."""
    return sqrt(var_axis(a, axis, keepdims, ddof))

def better_of(x, y, want_max):
    """Whether `x` beats `y` as the extremum: NaN is extreme and ties keep
    the earlier index."""
    return x != y and (x != x or (x > y if want_max else x < y))

def argmax(a):
    """np.argmax over all elements: the flat index of the first maximum."""
    return arg_reduce(a, True)

def argmin(a):
    """np.argmin over all elements: the flat index of the first minimum."""
    return arg_reduce(a, False)

def arg_reduce(a, want_max):
    """The flat index of the first extremum over every element."""
    return fold(range(Layout.flat_size(a.shape)), 0, lambda best, i: i if better_of(Storage.at(a.data, i), Storage.at(a.data, best), want_max) else best)

def argmax_axis(a, axis):
    """np.argmax along `axis`: the per-position index, as a flat int list over
    the reduced shape."""
    return arg_reduce_axis(a, axis, True)

def argmin_axis(a, axis):
    """np.argmin along `axis`: the per-position index, as a flat int list over
    the reduced shape."""
    return arg_reduce_axis(a, axis, False)

def arg_reduce_axis(a, axis, want_max):
    """The per-position extremum index along `axis`, as a flat int list over
    the reduced shape."""
    shape = a.shape
    if len(shape) == 0:
        return [0]
    else:
        plan = axis_plan(shape, axis, False)
        return toList(map(range(Layout.flat_size(plan.out_shape)), lambda i: argreduce_at(i, plan.out_shape, a, shape, plan.ax, plan.n, want_max)))

def argreduce_at(i, out_shape, a, shape, ax, n, want_max):
    """One argmax/argmin output: the index along `ax` of the extremum."""
    coords = Layout.coords_of(i, out_shape)
    return fold(range(n), 0, lambda best, k: k if arg_better(k, best, coords, a, shape, ax, want_max) else best)

def arg_better(k, best, coords, a, shape, ax, want_max):
    """Whether reduction index `k` beats `best` at this position."""
    return better_of(axis_element(a, coords, shape, ax, k), axis_element(a, coords, shape, ax, best), want_max)

def axis_element(a, coords, shape, ax, k):
    """The element at `coords` with the coordinate at `ax` replaced by `k`."""
    return Storage.at(a.data, Layout.ravel_index(Layout.insert_at(coords, ax, k), shape))

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

def cumulative(a, axis, op):
    """np.cumsum or np.cumprod along `axis`, under combine tag `op`."""
    return axis_gather(a, axis, CUM_OP, op)

def cumsum(a, axis):
    """np.cumsum along `axis`."""
    return cumulative(a, axis, SUM_OP)

def cumprod(a, axis):
    """np.cumprod along `axis`."""
    return cumulative(a, axis, PROD_OP)

def cum_read(i, a, ax, op):
    """One cumsum/cumprod output: the prefix over the axis up to this
    coordinate."""
    coords = Layout.coords_of(i, a.shape)
    k = axis_coord(coords, ax)
    seed = 0.0 if op == SUM_OP else 1.0
    return fold(range(k + 1), seed, lambda acc, t: combine_op(acc, read_at_axis(a.data, a.shape, coords, ax, t), op))
