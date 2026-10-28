# osprey: flavor=python
# osprey: namespace=numpy
# osprey: module=Errors
"""numpy's ValueError as an Osprey effect, with numpy's exact message text.

CPython raises `ValueError` for shape and value errors; the port performs
`ValueError.fail` instead, and a caller's `checked` computation sees the same
message text in an `Error` that `str(exception)` printed in CPython. The
`checked` functions each take a computation returning one concrete type: a
`with` suite is a closure whose result type must be concrete to compile, and
the compiler does not yet accept type parameters on functions (see the port's
README). Implements [PORT-SCOPE]'s Result-instead-of-exception rule.
"""
from typing import Callable

__all__ = [
    "ValueError", "raising", "fail", "require", "checked", "checked_float",
    "checked_int", "checked_bool", "checked_values", "checked_ints", "shape_text",
    "tuple_text", "broadcast_failed", "axis_out_of_bounds", "reshape_failed",
    "reshape_unknown_text", "broadcast_to_failed", "concat_dims_failed",
    "concat_size_failed", "squeeze_failed", "stack_shape_failed",
    "matmul_mismatch", "index_out_of_bounds", "negative_dimensions",
    "linspace_samples", "moveaxis_len", "need_one_array", "inhomogeneous",
    "broadcast_to_dims", "aligned_failed", "product_lengths",
]


@effect
class ValueError:
    """The shape and value error numpy raises, performed instead of raised:
    its control operation abandons the failing computation and answers the
    surrounding `checked` region with the message."""
    @control
    def fail(message: str) -> int: ...


@handler(ValueError)
class raising:
    """The ValueError policy: answer `fail` with an Error carrying numpy's
    message text, which a `checked` region turns into a Result."""
    def fail(message):
        return Error(message=message)


def checked(work: Callable[[], None]) -> Result[None, str]:
    """Runs a Unit computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def checked_float(work: Callable[[], float]) -> Result[float, str]:
    """Runs a float computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def checked_int(work: Callable[[], int]) -> Result[int, str]:
    """Runs an int computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def checked_bool(work: Callable[[], bool]) -> Result[bool, str]:
    """Runs a bool computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def checked_values(work: Callable[[], list[float]]) -> Result[list[float], str]:
    """Runs a float-list computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def checked_ints(work: Callable[[], list[int]]) -> Result[list[int], str]:
    """Runs an int-list computation under the ValueError policy."""
    with raising:
        return Success(value=work())


def fail(message):
    """Performs the ValueError carrying `message`. Its int result is the
    abandon's declared type; the computation never resumes past it."""
    return perform(ValueError.fail(message))


def require(cond, message):
    """The guard every validation shares: 0 when `cond` holds, else the
    ValueError carrying `message`. Bind the result to `_` at the call site —
    a failed guard abandons the computation before the binding happens."""
    return 0 if cond else fail(message)


def product_lengths(side, has, needs):
    """The error a row-major product gives when the `side` operand ("left" or
    "right") holds `has` elements but its stated shape needs `needs`. The text
    is the GPU runtime's own `gpuMatMul` error, so both interpretations fail
    alike."""
    return f"gpuMatMul: {side} operand: has {has} elements, the stated shape needs {needs}"


def shape_text(shape):
    """numpy's compact shape text: "(2,3)", "(4,)" for one dimension, "()" for
    none — the form the broadcast and reshape messages print."""
    inner = join(mapList(shape, lambda d: toString(d)), ",")
    return "(" + inner + ("," if len(shape) == 1 else "") + ")"


def tuple_text(shape):
    """Python's tuple repr of a shape: "(2, 3)", "(4,)", "()" — the form
    np.broadcast_shapes' message prints."""
    inner = join(mapList(shape, lambda d: toString(d)), ", ")
    return "(" + inner + ("," if len(shape) == 1 else "") + ")"


def broadcast_failed(a, b):
    """numpy's two-operand broadcast error text (note the trailing space)."""
    return "operands could not be broadcast together with shapes " + shape_text(a) + " " + shape_text(b) + " "


def axis_out_of_bounds(axis, span, prefix):
    """numpy's AxisError text for `axis` against `span` dimensions, with
    numpy's role prefix ("source: ", "axis1: ") when one applies."""
    return (prefix + ": " if prefix != "" else "") + f"axis {axis} is out of bounds for array of dimension {span}"


def reshape_failed(size, shape):
    """numpy's reshape error text."""
    return f"cannot reshape array of size {size} into shape {shape_text(shape)}"


def reshape_unknown_text(size, requested, unknown):
    """numpy's reshape error text when the unknown dimension cannot resolve:
    a leading unknown disappears from the text, any other prints as
    `newaxis`, and a lone dimension prints without a trailing comma."""
    dims = requested if unknown != 0 else drop_first(requested)
    inner = join(toList(map(range(len(dims)), lambda d: "newaxis" if unwrap_or(listGet(dims, d), 0) < 0 else toString(unwrap_or(listGet(dims, d), 0)))), ",")
    return f"cannot reshape array of size {size} into shape ({inner})"


def drop_first(xs):
    """`xs` without its first element."""
    return toList(map(range(1, len(xs)), lambda d: unwrap_or(listGet(xs, d), 0)))


def broadcast_to_failed(original, requested):
    """np.broadcast_to's error text (note the two spaces before `and`)."""
    gap = len(requested) - len(original)
    remapped = listConcat(toList(map(range(gap), lambda _: 1)), original)
    return "operands could not be broadcast together with remapped shapes [original->remapped]: " + shape_text(remapped) + "  and requested shape " + shape_text(requested)


def concat_dims_failed(i, nd_i, j, nd_j):
    """np.concatenate's ndim mismatch error text."""
    return f"all the input arrays must have same number of dimensions, but the array at index {i} has {nd_i} dimension(s) and the array at index {j} has {nd_j} dimension(s)"


def concat_size_failed(d, i, s_i, j, s_j):
    """np.concatenate's non-axis size mismatch error text."""
    return f"all the input array dimensions except for the concatenation axis must match exactly, but along dimension {d}, the array at index {i} has size {s_i} and the array at index {j} has size {s_j}"


def squeeze_failed():
    """np.squeeze's error text for a non-singleton selected axis."""
    return "cannot select an axis to squeeze out which has size not equal to one"


def stack_shape_failed():
    """np.stack's error text for mismatched input shapes."""
    return "all input arrays must have the same shape"


def matmul_mismatch(k_b, k_a):
    """np.matmul's core-dimension mismatch error text."""
    return f"matmul: Input operand 1 has a mismatch in its core dimension 0, with gufunc signature (n?,k),(k,m?)->(n?,m?) (size {k_b} is different from {k_a})"


def index_out_of_bounds(index, axis, size):
    """np.take's IndexError text (folded into the ValueError effect)."""
    return f"index {index} is out of bounds for axis {axis} with size {size}"


def negative_dimensions():
    """np.eye's error text for a negative dimension."""
    return "negative dimensions are not allowed"


def linspace_samples(num):
    """np.linspace's error text for a negative sample count."""
    return f"Number of samples, {num}, must be non-negative."


def moveaxis_len():
    """np.moveaxis's error text for mismatched source/destination counts."""
    return "`source` and `destination` arguments must have the same number of elements"


def need_one_array(op):
    """The error np.concatenate/stack print for an empty input tuple."""
    return f"need at least one array to {op}"


def inhomogeneous(level, shape):
    """np.asarray's error text for a ragged nested list."""
    return f"setting an array element with a sequence. The requested array has an inhomogeneous shape after {level} dimensions. The detected shape was {tuple_text(shape)} + inhomogeneous part."


def broadcast_to_dims():
    """np.broadcast_to's error text when the input has more dimensions than
    the requested shape."""
    return "input operand has more dimensions than allowed by the axis remapping"


def aligned_failed(a_shape, b_shape, ka, a_axis, kb, b_axis):
    """np.dot/np.inner's contraction mismatch error text."""
    return f"shapes {shape_text(a_shape)} and {shape_text(b_shape)} not aligned: {ka} (dim {a_axis}) != {kb} (dim {b_axis})"
