# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=numpy
# osprey: module=Storage
"""The buffer dialect every numpy array computes through.

`Array.NDArray` holds its elements in a `B` and never names the buffer type:
`Host.on_host` answers every `Buffer` operation with host list code and
`Device.on_gpu` with the GPU surface, so the handler a program installs decides
where the arithmetic runs. Both interpretations accumulate reductions in
element order from the seed and products in `k` order from 0.0, so they agree
bit for bit. Implements [PORT-DEVICE], [FLAVOR-PYTHON-GPU], [GPU-DEVICE].
"""
from typing import Callable

__all__ = [
    "Buffer", "at", "length_of", "lift", "lower", "add_f", "mul_f",
    "max_f", "min_f", "infinity", "not_a_number",
]


@static_effect
class Buffer[B]:
    """The buffer operations the port needs: build a buffer of `size` elements
    from an index function, read and measure one, combine two elementwise,
    reduce one to a scalar, multiply two row-major matrices, lift and lower
    host lists, and say which interpretation is answering. `B` is the
    representation the installed handler picks."""
    def build(size: int, kernel: Callable[[int], float]) -> B: ...
    def get(buffer: B, index: int) -> float: ...
    def length(buffer: B) -> int: ...
    def zip_with(a: B, b: B, combine: Callable[[float, float], float]) -> B: ...
    def reduce(buffer: B, seed: float, combine: Callable[[float, float], float]) -> float: ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> B: ...
    def from_list(values: list[float]) -> B: ...
    def to_list(buffer: B) -> list[float]: ...
    def device() -> str: ...


def at(buffer, index):
    """Reads one element of a buffer."""
    return perform(Buffer.get(buffer, index))


def length_of(buffer):
    """The element count of a buffer."""
    return perform(Buffer.length(buffer))


def lift(values):
    """Lifts a host list of doubles into a buffer."""
    return perform(Buffer.from_list(values))


def lower(buffer):
    """Lowers a buffer into a host list of doubles."""
    return perform(Buffer.to_list(buffer))


def add_f(a, b):
    """Addition as a combine kernel."""
    return a + b


def mul_f(a, b):
    """Multiplication as a combine kernel."""
    return a * b


def max_f(a, b):
    """np.maximum's kernel: NaN wins, else the larger."""
    return a if a != a else (b if b != b else (b if b > a else a))


def min_f(a, b):
    """np.minimum's kernel: NaN wins, else the smaller."""
    return a if a != a else (b if b != b else (b if b < a else a))


def infinity():
    """IEEE positive infinity (the flavor has no infinity literal)."""
    return exp(1000.0)


def not_a_number():
    """IEEE NaN (the flavor has no NaN literal)."""
    return infinity() - infinity()
