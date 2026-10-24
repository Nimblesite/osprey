# osprey: flavor=python
# osprey: namespace=numpy
# osprey: module=Linalg
"""numpy.linalg core: matrix products, dot products and the trace.

`matmul` follows numpy's promotion and batched-broadcast semantics: a 1-D
operand is promoted to a matrix and the row or column squeezed back; 2-D
products go through `Storage.mat_mul` (the GPU product); batched products
broadcast their leading dimensions and build one buffer over the output index
space. All results are float64.
"""
from numpy import Array, Errors, Layout, Storage

__all__ = ["matmul", "dot", "outer", "inner", "vdot", "trace"]


def drop_last(shape):
    """A shape without its last dimension."""
    return toList(map(range(len(shape) - 1), lambda d: unwrap_or(listGet(shape, d), 1)))


def drop_last_two(shape):
    """A shape without its last two dimensions."""
    return toList(map(range(len(shape) - 2), lambda d: unwrap_or(listGet(shape, d), 1)))


def take_first(xs, n):
    """The first `n` elements of a list."""
    return toList(map(range(n), lambda d: unwrap_or(listGet(xs, d), 0)))


def slice_coords(xs, lo, hi):
    """The elements of a list in `[lo, hi)`."""
    return toList(map(range(lo, hi), lambda d: unwrap_or(listGet(xs, d), 0)))


def matvec_inner(a, b):
    """The 1-D x 1-D matmul/dot: the inner product as a rank-0 array."""
    k = unwrap_or(listGet(a.shape, 0), 0)
    g = Errors.require(k == unwrap_or(listGet(b.shape, 0), 0), Errors.matmul_mismatch(unwrap_or(listGet(b.shape, 0), 0), k))
    return Array.NDArray(shape=[], data=perform(Storage.Buffer.build(1, lambda i: fold(range(k), 0.0, lambda acc, t: acc + Storage.at(a.data, t) * Storage.at(b.data, t)))))


def batch_base(coords, batch_len, strides, mat_size):
    """The flat offset of a batch element, given batch strides in elements."""
    return fold(range(batch_len), 0, lambda acc, d: acc + unwrap_or(listGet(coords, d), 0) * unwrap_or(listGet(strides, d), 0) * mat_size)


def batched_element(i, out_shape, a, b, batch_len, sa, sb, m, k, n):
    """One batched matmul element: the dot of a's row and b's column."""
    coords = Layout.coords_of(i, out_shape)
    r = unwrap_or(listGet(coords, batch_len), 0)
    col = unwrap_or(listGet(coords, batch_len + 1), 0)
    base_a = batch_base(coords, batch_len, sa, m * k)
    base_b = batch_base(coords, batch_len, sb, k * n)
    return fold(range(k), 0.0, lambda acc, t: acc + Storage.at(a.data, base_a + r * k + t) * Storage.at(b.data, base_b + t * n + col))


def matmul_core(a, b):
    """The 2-D-or-higher matmul: 2-D goes through the GPU product, batched
    products broadcast their leading dimensions."""
    m = unwrap_or(listGet(a.shape, len(a.shape) - 2), 0)
    k = unwrap_or(listGet(a.shape, len(a.shape) - 1), 0)
    k2 = unwrap_or(listGet(b.shape, len(b.shape) - 2), 0)
    n = unwrap_or(listGet(b.shape, len(b.shape) - 1), 0)
    g = Errors.require(k == k2, Errors.matmul_mismatch(k2, k))
    batch_a = drop_last_two(a.shape)
    batch_b = drop_last_two(b.shape)
    out_batch = Layout.broadcast_shapes([batch_a, batch_b])
    if len(out_batch) == 0:
        return Array.NDArray(shape=[m, n], data=perform(Storage.Buffer.mat_mul(a.data, b.data, m, k, n)))
    else:
        out_shape = listConcat(out_batch, [m, n])
        sa = Layout.broadcast_strides(batch_a, out_batch)
        sb = Layout.broadcast_strides(batch_b, out_batch)
        blen = len(out_batch)
        return Array.NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: batched_element(i, out_shape, a, b, blen, sa, sb, m, k, n))))


def matmul(a, b):
    """np.matmul: 1-D promotion, 2-D product, batched broadcast."""
    na = len(a.shape)
    nb = len(b.shape)
    if na == 1 and nb == 1:
        return matvec_inner(a, b)
    elif na == 1:
        a2 = Array.NDArray(shape=[1, unwrap_or(listGet(a.shape, 0), 0)], data=a.data)
        r = matmul_core(a2, b)
        return Array.squeeze_axis(r, len(r.shape) - 2)
    elif nb == 1:
        b2 = Array.NDArray(shape=[unwrap_or(listGet(b.shape, 0), 0), 1], data=b.data)
        r = matmul_core(a, b2)
        return Array.squeeze_axis(r, len(r.shape) - 1)
    else:
        return matmul_core(a, b)


def dot_gather(a, b, na, nb, ka, out_shape):
    """The general dot gather: contract a's last axis with b's second-to-last
    (or only) axis."""
    return Array.NDArray(shape=out_shape, data=perform(Storage.Buffer.build(Layout.flat_size(out_shape), lambda i: dot_element(i, a, b, na, nb, ka, out_shape))))


def dot_element(i, a, b, na, nb, ka, out_shape):
    """One dot output element: fold the contraction over the shared axis."""
    coords = Layout.coords_of(i, out_shape)
    a_prefix = take_first(coords, na - 1)
    return fold(range(ka), 0.0, lambda acc, t: acc + Storage.at(a.data, Layout.ravel_index(listAppend(a_prefix, t), a.shape)) * Storage.at(b.data, dot_b_index(coords, t, na, nb, b)))


def dot_b_index(coords, t, na, nb, b):
    """The flat offset into b of contraction index `t` for one output."""
    if nb == 1:
        return t
    else:
        b_mid = slice_coords(coords, na - 1, na - 1 + nb - 2)
        last = unwrap_or(listGet(coords, len(coords) - 1), 0)
        return Layout.ravel_index(listConcat(listConcat(b_mid, [t]), [last]), b.shape)


def dot(a, b):
    """np.dot: contraction over a's last axis and b's second-to-last (or only)
    axis."""
    na = len(a.shape)
    nb = len(b.shape)
    ka = unwrap_or(listGet(a.shape, na - 1), 1) if na > 0 else 1
    kb = unwrap_or(listGet(b.shape, 0), 1) if nb == 1 else unwrap_or(listGet(b.shape, nb - 2), 1)
    b_axis = 0 if nb == 1 else nb - 2
    out_shape = drop_last(a.shape) if nb == 1 else listConcat(drop_last(a.shape), listConcat(drop_last_two(b.shape), [unwrap_or(listGet(b.shape, nb - 1), 1)]))
    g = Errors.require(ka == kb, Errors.aligned_failed(a.shape, b.shape, ka, na - 1, kb, b_axis))
    if na == 1 and nb == 1:
        return Array.NDArray(shape=[], data=perform(Storage.Buffer.build(1, lambda i: fold(range(ka), 0.0, lambda acc, t: acc + Storage.at(a.data, t) * Storage.at(b.data, t)))))
    else:
        return dot_gather(a, b, na, nb, ka, out_shape)


def outer(a, b):
    """np.outer: the outer product of two flattened operands."""
    na = Layout.flat_size(a.shape)
    nb = Layout.flat_size(b.shape)
    return Array.NDArray(shape=[na, nb], data=perform(Storage.Buffer.build(na * nb, lambda i: Storage.at(a.data, i // nb) * Storage.at(b.data, i % nb))))


def inner(a, b):
    """np.inner contracts the last axes and combines the remaining dimensions."""
    return Array.multiply(a, b) if len(a.shape) == 0 or len(b.shape) == 0 else inner_vectors(a, b)


def inner_vectors(a, b):
    na = len(a.shape)
    nb = len(b.shape)
    ka = unwrap_or(listGet(a.shape, na - 1), 1)
    kb = unwrap_or(listGet(b.shape, nb - 1), 1)
    g = Errors.require(ka == kb, Errors.aligned_failed(a.shape, b.shape, ka, na - 1, kb, nb - 1))
    out = listConcat(drop_last(a.shape), drop_last(b.shape))
    b_rows = Layout.flat_size(drop_last(b.shape))
    return Array.NDArray(shape=out, data=perform(Storage.Buffer.build(Layout.flat_size(out), lambda i: inner_element(i, a, b, ka, b_rows))))


def inner_element(i, a, b, ka, b_rows):
    """One inner output element."""
    base_a = (i // b_rows) * ka
    base_b = (i % b_rows) * ka
    return fold(range(ka), 0.0, lambda acc, t: acc + Storage.at(a.data, base_a + t) * Storage.at(b.data, base_b + t))


def vdot(a, b):
    """np.vdot: the flattened dot product (conjugation is identity for real
    float64)."""
    na = Layout.flat_size(a.shape)
    nb = Layout.flat_size(b.shape)
    g = Errors.require(na == nb, Errors.reshape_failed(nb, [na]))
    return Array.NDArray(shape=[], data=perform(Storage.Buffer.build(1, lambda i: fold(range(na), 0.0, lambda acc, t: acc + Storage.at(a.data, t) * Storage.at(b.data, t)))))


def trace(a):
    """np.trace: the sum of the diagonal over the first two axes (a rank-0
    array for 2-D input, numpy's scalar)."""
    m = unwrap_or(listGet(a.shape, 0), 0)
    n = unwrap_or(listGet(a.shape, 1), 0)
    k = m if m < n else n
    head = slice_coords(a.shape, 2, len(a.shape))
    return Array.NDArray(shape=head, data=perform(Storage.Buffer.build(Layout.flat_size(head), lambda i: trace_element(i, head, a, n, k))))


def trace_element(i, head, a, n, k):
    """One trace output element over the head dimensions."""
    coords = Layout.coords_of(i, head)
    base = Layout.ravel_index(listConcat([0, 0], coords), a.shape)
    stride = (n + 1) * Layout.flat_size(head)
    return fold(range(k), 0.0, lambda acc, t: acc + Storage.at(a.data, base + t * stride))
