# osprey: flavor=python
# osprey: namespace=numpy
# osprey: module=Host
"""The host interpretation of `Storage.Buffer`: every operation answered with
host list code, accumulating reductions in element order and products in `k`
order from 0.0 exactly as `Device.on_gpu` does, so the two interpretations
agree bit for bit. Implements [PORT-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from numpy import Errors, Storage

__all__ = ["on_host"]


def product_of(a, b, rows, inner, cols):
    """The rows x cols product of well-shaped operands, accumulated in k order from 0.0."""
    return Success(value=toList(map(range(rows * cols), lambda i: fold(range(inner), 0.0, lambda acc, k: acc + unwrap_or(listGet(a, i // cols * inner + k), 0.0) * unwrap_or(listGet(b, k * cols + i % cols), 0.0)))))


def misfit(a, b, rows, inner, cols):
    """The first operand whose length disagrees with its stated shape, as the GPU runtime names it."""
    return Error(message=Errors.product_lengths("left", len(a), rows * inner)) if len(a) != rows * inner else Error(message=Errors.product_lengths("right", len(b), inner * cols))


@static_handler(Storage.Buffer[list[float]])
class on_host:
    """Host lists: a buffer is a plain list of doubles."""
    def build(n, f):
        return toList(map(range(n), f))

    def get(b, i):
        return unwrap_or(listGet(b, i), 0.0)

    def length(b):
        return len(b)

    def zip_with(a, b, f):
        return toList(map(range(len(a)), lambda i: f(unwrap_or(listGet(a, i), 0.0), unwrap_or(listGet(b, i), 0.0))))

    def reduce(b, seed, f):
        return fold(range(len(b)), seed, lambda acc, i: f(acc, unwrap_or(listGet(b, i), 0.0)))

    def mat_mul(a, b, rows, inner, cols):
        return product_of(a, b, rows, inner, cols) if len(a) == rows * inner and len(b) == inner * cols else misfit(a, b, rows, inner, cols)

    def from_list(xs):
        return xs

    def to_list(b):
        return b

    def device():
        return "host"
