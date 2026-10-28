# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Cpu
"""The CPU interpretation of `gpt2.Backends.Tensor`: every operation answered with
host list code, accumulating in the same k order from 0.0 as
examples/gpt2/reference.py. Nothing here names the GPU surface: weights load
through the host `loadTensor`, and a failed load or product answers its reason.
Implements [STAGE-BACKEND], [FLAVOR-PYTHON-GPU], [BUILTIN-FILE-TENSOR].
"""
from gpt2.Backends import Tensor, hit, min_i, product_lengths

__all__ = ["on_lists"]


def product_of(a, b, rows, inner, cols):
    """The rows x cols product of well-shaped lists, accumulated in k order from 0.0."""
    return Success(value=toList(map(range(rows * cols), lambda i: fold(range(inner), 0.0, lambda acc, k: acc + unwrap_or(listGet(a, i // cols * inner + k), 0.0) * unwrap_or(listGet(b, k * cols + i % cols), 0.0)))))


def misfit(a, b, rows, inner, cols):
    """The first operand whose length disagrees with its stated shape, named as the GPU runtime names it."""
    return Error(message=product_lengths("left", len(a), rows * inner)) if len(a) != rows * inner else Error(message=product_lengths("right", len(b), inner * cols))


def loaded(result):
    """A loaded tensor, or the loader's reason as text."""
    match result:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=toString(message))


@static_handler(Tensor[list[float]])
class on_lists:
    """Host lists, loaded straight into lists from the safetensors file."""
    def build(n, f):
        return toList(map(range(n), f))

    def get(b, i):
        return unwrap_or(listGet(b, i), 0.0)

    def length(b):
        return len(b)

    def zip_with(a, b, f):
        return toList(map(range(len(a)), lambda i: f(unwrap_or(listGet(a, i), 0.0), unwrap_or(listGet(b, i), 0.0))))

    def reduce(b, seed, f):
        return fold(range(len(b)), seed, lambda a, i: f(a, unwrap_or(listGet(b, i), 0.0)))

    def index_of(b, x):
        return fold(map(range(len(b)), lambda i: hit(unwrap_or(listGet(b, i), 0.0), x, i, len(b))), len(b), min_i)

    def mat_mul(a, b, rows, inner, cols):
        return product_of(a, b, rows, inner, cols) if len(a) == rows * inner and len(b) == inner * cols else misfit(a, b, rows, inner, cols)

    def from_list(xs):
        return xs

    def load(path, name):
        return loaded(loadTensor(path, name))

    def device():
        return "lists"
