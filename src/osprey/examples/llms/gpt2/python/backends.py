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

__all__ = ["Kernel", "Combine", "Tensor", "hit", "min_i", "stop", "product_lengths", "strictly", "under", "reported"]


# The kernels a model hands its handler: an index to its element, and two
# elements to one.
type Kernel = Callable[[int], float32]
"""A kernel a model hands its handler: from an element's index to the
element it builds."""
type Combine = Callable[[float32, float32], float32]
"""Two elements to one: the step of a `zip_with`, a `reduce` or a product's
accumulation."""


@static_effect
class Tensor[B]:
    """The operations a model needs: build a buffer from an index function, read
    and measure one, combine two, reduce one, find the first index holding a
    value, multiply two matrices, lift a host list, load a tensor from a
    safetensors file, and say which interpretation is answering. `B` is the
    representation the handler picks. A build, zip, reduction, index search,
    product, upload or load that cannot be done answers `Error` with the reason
    ([FAILURE-EXPLICIT])."""
    def build(size: int, kernel: Kernel) -> Result[B, str]:
        """Builds a buffer of `size` elements, element `i` from `kernel(i)`; a negative size answers `Error`.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        def stepped(i):
            return toFloat32(i * 2)
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.build(2, stepped)), Model.empty))))), "built")
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.build(-1, stepped)), Model.empty))))), "refused")
        ```
        ```output
        [0.0, 2.0]
        refused stopped: Tensor.build: a negative element count
        ```
        """
        ...
    def get(buffer: B, index: int) -> float32:
        """Reads element `index` of `buffer`; an index past the end reads the zero under `on_lists`.

        # Examples
        ```osprey
        from gpt2 import Cpu
        reported(strictly(lambda: Cpu.on_lists(lambda: print(f"second {perform(Tensor.get([toFloat32(1.0), toFloat32(4.0)], 1))}, past the end {perform(Tensor.get([toFloat32(1.0)], 3))}"))), "get")
        ```
        ```output
        second 4.0, past the end 0.0
        ```
        """
        ...
    def length(buffer: B) -> int:
        """The element count of `buffer`.

        # Examples
        ```osprey
        from gpt2 import Cpu
        reported(strictly(lambda: Cpu.on_lists(lambda: print(perform(Tensor.length([toFloat32(1.0), toFloat32(4.0)]))))), "length")
        ```
        ```output
        2
        ```
        """
        ...
    def zip_with(a: B, b: B, combine: Combine) -> Result[B, str]:
        """Combines `a` and `b` element by element through `combine`, or answers `Error` with the reason.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        def times(a, x):
            return a * x
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.zip_with([toFloat32(1.0), toFloat32(2.0)], [toFloat32(10.0), toFloat32(20.0)], times)), Model.empty))))), "zipped")
        ```
        ```output
        [10.0, 40.0]
        ```
        """
        ...
    def reduce(buffer: B, seed: float32, combine: Combine) -> Result[float32, str]:
        """Folds `combine` over the elements of `buffer` from `seed`, or answers `Error` with the reason.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        def added(a, x):
            return a + x
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.reduce([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0)], toFloat32(0.0), added)), lambda: toFloat32(0.0)))))), "summed")
        ```
        ```output
        6.0
        ```
        """
        ...
    def index_of(buffer: B, value: float32) -> Result[int, str]:
        """The first index holding `value`, or `Error` with the reason.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.index_of([toFloat32(3.0), toFloat32(1.0), toFloat32(4.0)], toFloat32(4.0))), lambda: -1))))), "found")
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.index_of([toFloat32(3.0)], toFloat32(4.0))), lambda: -1))))), "absent")
        ```
        ```output
        2
        1
        ```
        """
        ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> Result[B, str]:
        """a [rows, inner] times b [inner, cols]; operands that disagree with the stated shape answer `Error`.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.mat_mul([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0), toFloat32(4.0)], [toFloat32(5.0), toFloat32(6.0), toFloat32(7.0), toFloat32(8.0)], 2, 2, 2)), Model.empty))))), "product")
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.mat_mul([toFloat32(1.0), toFloat32(2.0)], [toFloat32(1.0)], 1, 1, 1)), Model.empty))))), "misfit")
        ```
        ```output
        [19.0, 22.0, 43.0, 50.0]
        misfit stopped: gpuMatMul: left operand: has 2 elements, the stated shape needs 1
        ```
        """
        ...
    def from_list(values: list[float32]) -> Result[B, str]:
        """Lifts host values into a buffer, or answers `Error` with the reason.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.from_list([toFloat32(1.0), toFloat32(2.0)])), Model.empty))))), "lifted")
        ```
        ```output
        [1.0, 2.0]
        ```
        """
        ...
    def load(path: str, name: str) -> Result[B, str]:
        """The tensor `name` of the safetensors file at `path`, or `Error` with the loader's reason.

        # Examples
        ```osprey
        from gpt2 import Cpu, Model
        reported(strictly(lambda: Cpu.on_lists(lambda: print(toString(Model.or_stop(perform(Tensor.load("no-such-weights.safetensors", "wte.weight")), Model.empty))))), "loaded")
        ```
        ```output
        loaded stopped: loadTensor: no-such-weights.safetensors: No such file or directory
        ```
        """
        ...
    def device() -> str:
        """The handler's own name for where the arithmetic runs.

        # Examples
        ```osprey
        from gpt2 import Cpu, Gpu
        reported(strictly(lambda: Cpu.on_lists(lambda: print(perform(Tensor.device())))), "lists")
        reported(strictly(lambda: Gpu.on_gpu(lambda: print(perform(Tensor.device())))), "host")
        ```
        ```output
        lists
        host
        ```
        """
        ...


def hit(v, x, i, n):
    """`index_of`'s first step under both handlers: element i names itself when it
    holds x and the length n otherwise.

    # Examples
    ```osprey
    print(f"{hit(toFloat32(2.0), toFloat32(2.0), 1, 3)} {hit(toFloat32(1.0), toFloat32(2.0), 1, 3)}")
    ```
    ```output
    1 3
    ```
    """
    return i if v == x else n


def min_i(a, i):
    """`index_of`'s second step: the smallest name wins, so the answer is the first
    index holding x or n when none does.

    # Examples
    ```osprey
    print(f"{min_i(2, 1)} {min_i(1, 2)}")
    ```
    ```output
    1 1
    ```
    """
    return i if i < a else a


@effect
class Fault:
    """A fault ends the run: its arm answers the whole region. Arithmetic
    faults, failed products and failed weight loads all stop here."""
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    def halt(fault):
        return Error(message=fault)


def stop(reason):
    """Stops the run with `reason`: the surrounding `strictly` answers Error
    naming it. The operation never resumes.

    # Examples
    ```osprey
    reported(strictly(lambda: print(f"{stop('stopped on purpose')}")), "run")
    ```
    ```output
    run stopped: stopped on purpose
    ```
    """
    return perform(Fault.halt(reason))


@handler(Arith)
class arithmetic:
    def overflow(op, l, r, _):
        return stop(f"integer overflow: {l} {op} {r}")

    def divideByZero(op, l):
        match stop(f"division by zero: {l} {op} 0"):
            case _:
                return l

    def remainderByZero(l):
        return stop(f"remainder by zero: {l} % 0")


def product_lengths(side, has, needs):
    """The reason a product refuses operands whose element count disagrees with
    the stated shape: the GPU runtime's own `gpuMatMul` text, so both
    interpretations stop alike.

    # Examples
    ```osprey
    print(product_lengths("left", 3, 4))
    ```
    ```output
    gpuMatMul: left operand: has 3 elements, the stated shape needs 4
    ```
    """
    return f"gpuMatMul: {side} operand: has {has} elements, the stated shape needs {needs}"


def strictly(work) -> Result[None, str]:
    """Runs `work` under GPT-2's arithmetic policy: Success with its value, or Error
    naming the first fault. A fault is a bug in the model, never a value to
    substitute (a fabricated 0.0 is indistinguishable from a real logit), so it
    stops the run, as a device kernel's fault does [GPU-DEVICE-FAULT]. The
    annotation pins what inference leaves open: each `with` suite is a closure
    whose Result type must be concrete to compile.

    # Examples
    ```osprey
    reported(strictly(lambda: print(2 + 2)), "sum")
    reported(strictly(lambda: print(toFloat32(1.0) / toFloat32(0.0))), "quotient")
    ```
    ```output
    4
    quotient stopped: division by zero: 1.0 / 0
    ```
    """
    with halting:
        with arithmetic:
            return Success(value=work())


def under(name, work):
    """Runs `work` under the handler `name` selects. An unknown name runs nothing,
    so a mistyped backend never silently lands on the GPU surface.

    # Examples
    ```osprey
    reported(strictly(lambda: under("cpu", lambda: print("ran"))), "under")
    ```
    ```output
    unknown backend `cpu`: choose gpu or lists
    ```
    """
    match name:
        case "gpu":
            return Gpu.on_gpu(work)
        case "lists":
            return Cpu.on_lists(work)
        case _:
            return print(f"unknown backend `{name}`: choose gpu or lists")


def reported(run, label):
    """A program's answer, or its last line: `label stopped: <the fault>` when the
    run failed, so a fault ends the output instead of leaving half of it.

    # Examples
    ```osprey
    reported(strictly(lambda: print("ran")), "example")
    reported(strictly(lambda: print(f"{stop('stopped on purpose')}")), "example")
    ```
    ```output
    ran
    example stopped: stopped on purpose
    ```
    """
    match run:
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"{label} stopped: {message}")
