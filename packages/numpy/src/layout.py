# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=numpy
# osprey: module=Layout
"""The pure shape and index engine every module of the port shares.

Sizes, row-major strides, broadcasting, axis normalisation and the arithmetic
that maps an output index to a source offset, plus the three gather engines
that build one buffer over an output index space while reading their operands
at broadcast or permuted offsets — the single construction path the port's
performance rule requires. Validation guards perform `Errors.ValueError` with
numpy's message text; everything else is pure list arithmetic.
"""
from typing import Callable

from numpy import Errors, Storage

__all__ = [
    "flat_size", "strides_of", "offset_of", "coords_of", "ravel_index",
    "broadcast_two", "broadcast_shapes", "broadcast_strides", "compatible",
    "normalize_axis", "normalize_axes", "resolve_reshape", "drop_at",
    "insert_at", "set_at", "permuted", "swap_perm", "moveaxis_perm",
    "axis_of", "contains",
]


def flat_size(shape):
    """The number of elements a shape holds: the product of its dimensions,
    1 for rank 0."""
    return fold(range(len(shape)), 1, lambda acc, d: acc * unwrap_or(listGet(shape, d), 1))


def strides_of(shape):
    """Row-major strides: the element count one step along each dimension
    moves. The stride of the last dimension is 1; a rank-0 shape has none."""
    return toList(map(range(len(shape)), lambda d: fold(range(d + 1, len(shape)), 1, lambda acc, k: acc * unwrap_or(listGet(shape, k), 1))))


def offset_of(i, shape, strides):
    """The flat offset of output index `i` over `shape`, given strides that
    may be permuted or zero (broadcast): the sum of each dimension's
    coordinate times its stride."""
    coordinates = coords_of(i, shape)
    return fold(range(len(shape)), 0, lambda acc, d: acc + unwrap_or(listGet(coordinates, d), 0) * unwrap_or(listGet(strides, d), 0))


def coords_of(i, shape):
    """The coordinates of flat index `i` over `shape` (unravel)."""
    return toList(map(range(len(shape)), lambda d: (i // unwrap_or(listGet(strides_of(shape), d), 1)) % unwrap_or(listGet(shape, d), 1)))


def ravel_index(coords, shape):
    """The flat index of `coords` over `shape` (ravel)."""
    return fold(range(len(shape)), 0, lambda acc, d: acc + unwrap_or(listGet(coords, d), 0) * unwrap_or(listGet(strides_of(shape), d), 0))


def dim_from_right(shape, k):
    """The k-th dimension of a shape counted from the right, 1 past its rank
    (the missing dimensions of a broadcast operand)."""
    return unwrap_or(listGet(shape, len(shape) - 1 - k), 1)


def mergeable(x, y):
    """One broadcast dimension pair: equal sizes, or one of them is 1."""
    return x == y or x == 1 or y == 1


def merged_dim(x, y, a, b):
    """One broadcast dimension after its guard: a 1 stretches to the other,
    and equal sizes pass through."""
    g = Errors.require(mergeable(x, y), Errors.broadcast_failed(a, b))
    return y if x == 1 else x


def broadcast_two(a, b):
    """numpy's broadcast of two shapes, failing with the two-operand message
    binary operations print."""
    rank = len(a) if len(a) > len(b) else len(b)
    return toList(map(range(rank), lambda d: merged_dim(dim_from_right(a, rank - 1 - d), dim_from_right(b, rank - 1 - d), a, b)))


def compatible(a, b):
    """Whether two shapes broadcast (numpy's rule), without numpy's error."""
    rank = len(a) if len(a) > len(b) else len(b)
    return fold(range(rank), True, lambda ok, k: ok and mergeable(dim_from_right(a, k), dim_from_right(b, k)))


def clash_at(running, shape, k):
    """Whether dimension `k` counted from the right clashes between shapes."""
    x = dim_from_right(running, k)
    y = dim_from_right(shape, k)
    return x != y and x != 1 and y != 1


def first_setter(shapes, j, k):
    """The first shape before `j` holding a size other than 1 at dimension
    `k` from the right: the shape numpy's broadcast message names."""
    return fold(range(j), j, lambda first, t: t if first == j and dim_from_right(unwrap_or(listGet(shapes, t), []), k) != 1 else first)


def mismatch_text(running, shape, shapes, j):
    """np.broadcast_shapes' error text: the most significant clashing
    dimension, the first earlier shape that set it, and the offender."""
    rank = len(running) if len(running) > len(shape) else len(shape)
    k = fold(range(rank), -1, lambda worst, t: t if clash_at(running, shape, t) else worst)
    i = first_setter(shapes, j, k)
    return "shape mismatch: objects cannot be broadcast to a single shape.  Mismatch is between arg " + toString(i) + " with shape " + Errors.tuple_text(unwrap_or(listGet(shapes, i), [])) + " and arg " + toString(j) + " with shape " + Errors.tuple_text(shape)


def merge_into(running, shape, shapes, j):
    """One np.broadcast_shapes fold step: merge `shape` into `running`, or
    perform numpy's multi-operand message."""
    g = Errors.require(compatible(running, shape), mismatch_text(running, shape, shapes, j))
    return broadcast_two(running, shape)


def broadcast_shapes(shapes):
    """np.broadcast_shapes over a list of shapes: a left fold that stops at
    the first shape that cannot join, with numpy's multi-operand error."""
    first = unwrap_or(listGet(shapes, 0), [])
    return fold(range(1, len(shapes)), first, lambda running, j: merge_into(running, unwrap_or(listGet(shapes, j), []), shapes, j))


def broadcast_strides(shape, out_shape):
    """The strides of `shape` right-aligned inside `out_shape`: zero along the
    dimensions it broadcasts, so a gather repeats its elements there."""
    inner = strides_of(shape)
    gap = len(out_shape) - len(shape)
    return toList(map(range(len(out_shape)), lambda d: 0 if d < gap or unwrap_or(listGet(shape, d - gap), 1) == 1 else unwrap_or(listGet(inner, d - gap), 0)))


def normalize_axis(axis, span, prefix):
    """Normalises a possibly-negative axis against `span` valid dimensions
    (`span` is ndim for existing axes, ndim + 1 where an axis may be
    inserted), failing with numpy's AxisError text."""
    g = Errors.require(axis >= 0 - span and axis < span, Errors.axis_out_of_bounds(axis, span, prefix))
    return axis if axis >= 0 else axis + span


def axis_of(a, axis):
    """A reduction's normalised axis, with numpy's 0-d exception: axis 0 or
    -1 of a rank-0 array is the array itself, not an error."""
    if len(a) == 0:
        g = Errors.require(axis == 0 or axis == -1, Errors.axis_out_of_bounds(axis, 0, ""))
        return 0
    else:
        return normalize_axis(axis, len(a), "")


def contains(xs, x):
    """Whether `x` appears in `xs`."""
    return fold(range(len(xs)), False, lambda found, i: found or unwrap_or(listGet(xs, i), 0) == x)


def has_repeats(xs):
    """Whether any value appears twice."""
    return fold(range(len(xs)), False, lambda bad, i: bad or contains(drop_at(xs, i), unwrap_or(listGet(xs, i), 0)))


def normalize_axes(axes, ndim):
    """Normalises a transpose axis list: every axis in range, none repeated,
    and one per dimension, with numpy's transpose messages."""
    g = Errors.require(len(axes) == ndim, "axes don't match array")
    normed = toList(map(range(len(axes)), lambda i: normalize_axis(unwrap_or(listGet(axes, i), 0), ndim, "")))
    g2 = Errors.require(not has_repeats(normed), "repeated axis in transpose")
    return normed


def drop_at(xs, i):
    """`xs` without its `i`-th element."""
    return fold(range(len(xs)), [], lambda acc, d: acc if d == i else listAppend(acc, unwrap_or(listGet(xs, d), 0)))


def insert_at(xs, i, x):
    """`xs` with `x` inserted at position `i`."""
    return fold(range(len(xs) + 1), [], lambda acc, d: listAppend(acc, x) if d == i else listAppend(acc, unwrap_or(listGet(xs, d if d < i else d - 1), 0)))


def set_at(xs, i, x):
    """`xs` with its `i`-th element replaced by `x`."""
    return toList(map(range(len(xs)), lambda d: x if d == i else unwrap_or(listGet(xs, d), 0)))


def permuted(shape, axes):
    """The output shape of a transpose: shape[axes[0]], shape[axes[1]], ..."""
    return mapList(axes, lambda a: unwrap_or(listGet(shape, a), 0))


def swap_perm(ndim, a, b):
    """The axis permutation swapaxes applies."""
    return toList(map(range(ndim), lambda d: a if d == b else (b if d == a else d)))


def moved_axis(position, sources, dests, free_axes, free_positions):
    """The source axis landing at `position` under moveaxis: a destination's
    moved source, else the next unlisted axis in order."""
    d = fold(range(len(dests)), -1, lambda found, t: t if found < 0 and unwrap_or(listGet(dests, t), 0) == position else found)
    if d >= 0:
        return unwrap_or(listGet(sources, d), 0)
    else:
        r = fold(range(len(free_positions)), -1, lambda found, t: t if found < 0 and unwrap_or(listGet(free_positions, t), 0) == position else found)
        return unwrap_or(listGet(free_axes, r), 0)


def moveaxis_perm(ndim, sources, dests):
    """The axis permutation moveaxis applies, validated: moved axes land on
    their destinations and the rest keep their relative order, with numpy's
    AxisError and repeat messages."""
    g = Errors.require(len(sources) == len(dests), "`source` and `destination` arguments must have the same number of elements")
    normed_sources = toList(map(range(len(sources)), lambda i: normalize_axis(unwrap_or(listGet(sources, i), 0), ndim, "source")))
    normed_dests = toList(map(range(len(dests)), lambda i: normalize_axis(unwrap_or(listGet(dests, i), 0), ndim, "destination")))
    g2 = Errors.require(not has_repeats(normed_sources), "repeated axis in `source` argument")
    g3 = Errors.require(not has_repeats(normed_dests), "repeated axis in `destination` argument")
    return moveaxis_target(ndim, normed_sources, normed_dests)


def moveaxis_target(ndim, sources, dests):
    """The validated moveaxis permutation over normalised axes."""
    free_positions = filterList(toList(map(range(ndim), lambda p: p)), lambda p: not contains(dests, p))
    free_axes = filterList(toList(map(range(ndim), lambda a: a)), lambda a: not contains(sources, a))
    return toList(map(range(ndim), lambda p: moved_axis(p, sources, dests, free_axes, free_positions)))


def first_negative(xs):
    """The index of the first negative element, -1 when there is none."""
    return fold(range(len(xs)), -1, lambda found, i: i if found < 0 and unwrap_or(listGet(xs, i), 0) < 0 else found)


def negative_count(xs):
    """How many elements are negative."""
    return fold(range(len(xs)), 0, lambda count, i: count + (1 if unwrap_or(listGet(xs, i), 0) < 0 else 0))


def known_product(requested):
    """The product of a requested shape's non-negative dimensions."""
    return fold(range(len(requested)), 1, lambda acc, d: acc * (1 if unwrap_or(listGet(requested, d), 0) < 0 else unwrap_or(listGet(requested, d), 1)))


def resolve_reshape(size, requested):
    """Resolves numpy's reshape of `size` elements into `requested`: at most
    one negative dimension is the unknown, and the rest must divide `size`
    exactly. Fails with numpy's three reshape messages."""
    unknown = first_negative(requested)
    known = known_product(requested)
    g = Errors.require(negative_count(requested) <= 1, "can only specify one unknown dimension")
    if unknown < 0:
        g2 = Errors.require(known == size, Errors.reshape_failed(size, requested))
        return requested
    else:
        g3 = Errors.require(known != 0 and size % known == 0, Errors.reshape_unknown_text(size, requested, unknown))
        return set_at(requested, unknown, size // known)
