# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Backends
"""The tensor dialect GPT-2 is written in.

`gpt2.Model` performs `Tensor` operations and never names a buffer type;
`Cpu.on_lists` (cpu.py) answers every operation with host list code and
`Gpu.on_gpu` (gpu.py) with the GPU surface. The handler a program installs decides
where the arithmetic runs, and both must agree with src/osprey/examples/llms/gpt2/reference.py bit
for bit, because every arm accumulates in the same k order from 0.0.
Implements [STAGE-BACKEND], [GPU-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from typing import Callable

from gpt2 import Cpu, Gpu

__all__ = ["Kernel", "Combine", "Tensor", "hit", "min_i", "stop", "product_lengths", "strictly", "under"]


# The kernels a model hands its handler: an index to its element, and two
# elements to one.
type Kernel = Callable[[int], float32]
type Combine = Callable[[float32, float32], float32]


@static_effect
class Tensor[B]:
    """The operations a model needs, `B` being the representation the handler
    picks: build, read, measure, combine, reduce, find, multiply, lift, load and
    name the device. One that cannot be done answers `Error` with the reason
    ([FAILURE-EXPLICIT])."""
    def build(size: int, kernel: Kernel) -> Result[B, str]: ...
    def get(buffer: B, index: int) -> float32: ...
    def length(buffer: B) -> int: ...
    def zip_with(a: B, b: B, combine: Combine) -> Result[B, str]: ...
    def reduce(buffer: B, seed: float32, combine: Combine) -> Result[float32, str]: ...
    def index_of(buffer: B, value: float32) -> Result[int, str]: ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> Result[B, str]: ...
    def from_list(values: list[float32]) -> Result[B, str]: ...
    def load(path: str, name: str) -> Result[B, str]: ...
    def device() -> str: ...


def hit(v, x, i, n):
    """`index_of`'s first step: element i names itself when it holds x, else n."""
    return i if v == x else n


def min_i(a, i):
    """`index_of`'s second step: the smallest name wins, so the first holder wins."""
    return i if i < a else a


@effect
class Fault:
    """A fault ends the run: its arm answers the whole region for an arithmetic,
    product or load fault."""
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    def halt(fault):
        return Error(message=fault)


def stop(reason):
    """Stops the run with `reason`: the surrounding `strictly` answers Error
    naming it. Every arithmetic arm is one call here, so they all stop alike."""
    return perform(Fault.halt(reason))


@handler(Arith)
class arithmetic:
    def overflow(op, l, r, _):
        return stop(f"integer overflow: {l} {op} {r}")

    def divideByZero(op, l):
        return stop(f"division by zero: {l} {op} 0")

    def remainderByZero(l):
        return stop(f"remainder by zero: {l} % 0")


def product_lengths(side, has, needs):
    """The reason a product refuses operands whose element count disagrees with the
    stated shape: the GPU runtime's own `gpuMatMul` text, so both agree."""
    return f"gpuMatMul: {side} operand: has {has} elements, the stated shape needs {needs}"


def strictly(work) -> Result[None, str]:
    """Runs `work` under GPT-2's arithmetic policy: Success with its value, or Error
    naming the first fault, since a fault is a model bug and never a value to
    substitute [GPU-DEVICE-FAULT]. The annotation pins each `with` closure's
    Result type, which inference leaves open."""
    with halting:
        with arithmetic:
            return Success(value=work())


def under(name, work):
    """Runs `work` under the handler `name` selects; an unknown name runs nothing,
    so a mistyped backend never silently lands on the GPU surface."""
    match name:
        case "gpu":
            return Gpu.on_gpu(work)
        case "lists":
            return Cpu.on_lists(work)
        case _:
            return print(f"unknown backend `{name}`: choose gpu or lists")
