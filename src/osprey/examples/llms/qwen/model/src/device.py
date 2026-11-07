# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=Qwen3_5Device
"""What every `qwen3_5` module shares on the device: weights, faults, grids, index arithmetic.

A `Weight` is one `nn.Linear`'s matrix as the checkpoint stores it: GGUF blocks in a
quantized format `gpuMatMulQuant` reads (rows of `out`, each `in` wide, as `nn.Linear`
lays `[out, in]` out), or dense `float32` values transposed to `[in, out]` for
`gpuMatMul`. `linear` is the product either way, so no module knows which it holds.

A failed GPU step ends the whole forward pass through `Fault`: the `strictly` handler
turns it into the `Error` the caller's `Result` carries, so a model never computes on
from a missing buffer ([FAILURE-EXPLICIT]). Index arithmetic inside kernels uses the
operations that cannot fault (`wrapAdd`, `wrapMul`, shifts), and a division by a
run-time width goes through `Div`, a multiply-shift reciprocal exact for every index
below 2^32 (Granlund and Montgomery, "Division by invariant integers using
multiplication", 1994): a kernel with no fault site stays in the open command buffer
([LLM-DISPATCH-BATCH]). Implements [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION].
"""

__all__ = ["Weight", "Fault", "strictly", "wrapping", "must", "must_ints", "must_values", "grid", "at", "zeros", "constant", "inverse", "Div", "divider", "quot", "rem", "linear", "added", "sigmoid", "rsqrt", "softplus", "squares", "slice_of"]


@union
class Weight:
    """One linear layer's matrix: quantized GGUF blocks, or dense `float32` values."""
    class Quant:
        blocks: GpuBuffer[uint8]
        type: str
    class Dense:
        values: GpuBuffer[float32]


@effect
class Fault:
    """A failed GPU step ends the forward pass: its arm answers the whole region."""
    @control
    def halt(message: str) -> GpuBuffer[float32]: ...
    @control
    def halt_ints(message: str) -> GpuBuffer[int]: ...
    @control
    def halt_values(message: str) -> list[float32]: ...


@handler(Fault)
class strictly:
    """The fault becomes the `Error` of the region's `Result`."""

    def halt(message):
        return Error(message=message)

    def halt_ints(message):
        return Error(message=message)

    def halt_values(message):
        return Error(message=message)


@handler(Arith)
class wrapping:
    """Every index a kernel forms is in range by construction, so these arms never run: a
    kernel's own faults report through the device's fault word, not through `Arith`."""

    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, l):
        return toFloat(l)

    def remainderByZero(l):
        return l


def must(result) -> GpuBuffer[float32]:
    """The buffer a step produced, or the fault that ends the pass."""
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Fault.halt(toString(message)))


def must_ints(result) -> GpuBuffer[int]:
    """The int buffer a step produced, or the fault that ends the pass."""
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Fault.halt_ints(toString(message)))


def must_values(result) -> list[float32]:
    """A host vector a loader found, or the fault that ends the load."""
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Fault.halt_values(toString(message)))


def grid(n) -> GpuBuffer[int]:
    """The index buffer `[0, n)`: every kernel maps over one."""
    return must_ints(gpuIota(n))


def at(b, i) -> float32:
    """Element `i` of a `float32` buffer; every index a kernel forms is in range by construction."""
    return unwrap_or(gpuGet(b, i), toFloat32(0.0))


def zeros(g) -> GpuBuffer[float32]:
    """A zero for every index of `g`."""
    return must(gpuMap(g, lambda _: toFloat32(0.0)))


def constant(values) -> GpuBuffer[float32]:
    """A host list of `float32` moved to the device."""
    return must(toGpu(values))


def inverse(width) -> float32:
    """`1 / width` as the `float32` a mean multiplies by."""
    return toFloat32(1.0 / toFloat(width))


class Div:
    """A reciprocal for `i // d`: `(i * mul) >> shift`, exact for `0 <= i < 2^32`."""
    mul: int
    shift: int


def log2_ceil(d):
    return fold(range(0, 40), 0, lambda bits, _b: bits if shiftLeft(1, bits) >= d else bits + 1)


def divider(d) -> Div:
    """`Div` for the positive divisor `d`: `shift = 32 + ceil(log2 d)`, `mul = 2^shift // d + 1`."""
    k = 32 + log2_ceil(d)
    return Div(mul=shiftLeft(1, k) // d + 1, shift=k)


def quot(i, mul, shift):
    """`i // d` for the `Div(mul, shift)` of `d`, with no fault site."""
    return shiftRightLogical(wrapMul(i, mul), shift)


def rem(i, mul, shift, d):
    """`i % d` for the `Div(mul, shift)` of `d`."""
    return wrapSub(i, wrapMul(quot(i, mul, shift), d))


def linear(x, w, rows, inner, cols) -> GpuBuffer[float32]:
    """`x @ W^T` for `rows` rows of `inner` values against a `[cols, inner]` weight."""
    match w:
        case Quant(blocks=blocks, type=type):
            return must(gpuMatMulQuant(x, blocks, type, rows, inner, cols))
        case Dense(values=values):
            return must(gpuMatMul(x, values, rows, inner, cols))


def added(a, b, g) -> GpuBuffer[float32]:
    """`a + b` over the indices of `g`."""
    return must(gpuMap(g, lambda i: at(a, i) + at(b, i)))


def sigmoid(x) -> GpuBuffer[float32]:
    """`torch.sigmoid` of every value of the buffer `x`."""
    return must(gpuSigmoid(x))


def rsqrt(x: float32) -> float32:
    """`torch.rsqrt` as the device library's `exp` and `ln` form it."""
    return exp(toFloat32(-0.5) * ln(x))


def softplus(x: float32) -> float32:
    """`F.softplus(x)`: `ln(1 + exp(x))`."""
    return ln(toFloat32(1.0) + exp(x))


def squares(x, start, n) -> float32:
    """The sum of squares of `n` values of `x` from `start`, left to right."""
    return fold(range(0, n), toFloat32(0.0), lambda a, i: a + at(x, wrapAdd(start, i)) * at(x, wrapAdd(start, i)))


def slice_of(x, start, g) -> GpuBuffer[float32]:
    """The values of `x` from `start`, one per index of `g`."""
    return must(gpuMap(g, lambda i: at(x, wrapAdd(start, i))))
