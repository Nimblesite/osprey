# osprey: flavor=python
# osprey: namespace=numpy_tests
# osprey: module=Contracts
"""Public helper contracts and errors recorded by the CPython oracle."""
from numpy import Array, Errors, Layout, Linalg, Math, Storage
from numpy_expected import Expected
from numpy_tests import Support as S
from testing import Check

__all__ = ["run"]


def error_text(key):
    return foldList(Expected.ERRORS, "missing oracle case", lambda found, pair: S.ats(pair, 1) if S.ats(pair, 0) == key else found)


def error_messages_a():
    return [
        ["BROADCAST", Errors.broadcast_failed([2, 3], [4])],
        ["AXIS", Errors.axis_out_of_bounds(5, 2, "")],
        ["RESHAPE", Errors.reshape_failed(6, [2, 4])],
        ["RESHAPE_NEGX", Errors.reshape_unknown_text(6, [2, -1, 5], 1)],
        ["BCAST_TO", Errors.broadcast_to_failed([2, 1], [3, 5])],
        ["BCAST_TO_DIMS", Errors.broadcast_to_dims()],
        ["CONCAT_DIM", Errors.concat_dims_failed(0, 2, 1, 3)],
        ["CONCAT_SIZE", Errors.concat_size_failed(1, 0, 3, 1, 4)],
        ["SQUEEZE", Errors.squeeze_failed()],
        ["STACK_SHAPE", Errors.stack_shape_failed()],
    ]


def error_messages_b():
    return [
        ["MATMUL", Errors.matmul_mismatch(4, 3)],
        ["TAKE", Errors.index_out_of_bounds(5, 0, 3)],
        ["EYE", Errors.negative_dimensions()],
        ["LINSPACE", Errors.linspace_samples(-1)],
        ["MOVEAXIS_LEN", Errors.moveaxis_len()],
        ["CONCAT_EMPTY", Errors.need_one_array("concatenate")],
        ["RAGGED2", Errors.inhomogeneous(1, [2])],
        ["DOT", Errors.aligned_failed([2, 3], [4, 5], 3, 1, 4, 0)],
    ]


def caught_failure(work):
    match Errors.checked_int(work):
        case Error(message=message):
            return message
        case Success():
            return "missing error"


def malformed_product():
    """A 2x2 array holding three elements: both storage interpretations refuse
    the product with the same ValueError instead of reading past the end."""
    bad = Array.NDArray(shape=[2, 2], data=Storage.lift([1.0, 2.0, 3.0]))
    match Errors.checked_values(lambda: Array.tolist(Linalg.matmul(bad, Array.eye(2)))):
        case Error(message=message):
            return message
        case Success():
            return "missing error"


def unit_checked():
    match Errors.checked(lambda: None):
        case Success():
            return True
        case Error():
            return False


def error_cases(b):
    messages = listConcat(error_messages_a(), error_messages_b())
    checks = mapList(messages, lambda pair: Check.equal(f"errors.{S.ats(pair, 0)}.{b}", S.ats(pair, 1), error_text(S.ats(pair, 0))))
    return listConcat(checks, [
        Check.equal(f"errors.shape_text.{b}", Errors.shape_text([2, 3]), "(2,3)"),
        Check.equal(f"errors.tuple_text.{b}", Errors.tuple_text([2, 3]), "(2, 3)"),
        Check.equal(f"errors.fail.{b}", caught_failure(lambda: Errors.fail("sentinel")), "sentinel"),
        Check.equal(f"errors.require.{b}", caught_failure(lambda: Errors.require(False, "required")), "required"),
        Check.truth(f"errors.checked.{b}", unit_checked(), "unit work succeeds"),
        Check.equal(f"errors.product_lengths.{b}", malformed_product(), Errors.product_lengths("left", 3, 4)),
    ])


def layout_lists():
    return [
        Check.ints_equal("layout.broadcast_two", Layout.broadcast_two([2, 1], [3]), [2, 3]),
        Check.ints_equal("layout.broadcast_strides", Layout.broadcast_strides([1, 3], [2, 3]), [0, 1]),
        Check.ints_equal("layout.coords_of", Layout.coords_of(5, [2, 3]), [1, 2]),
        Check.ints_equal("layout.drop_at", Layout.drop_at([2, 3, 4], 1), [2, 4]),
        Check.ints_equal("layout.insert_at", Layout.insert_at([2, 4], 1, 3), [2, 3, 4]),
        Check.ints_equal("layout.set_at", Layout.set_at([2, 3], 1, 4), [2, 4]),
        Check.ints_equal("layout.strides_of", Layout.strides_of([2, 3, 4]), [12, 4, 1]),
        Check.ints_equal("layout.normalize_axes", Layout.normalize_axes([-1, 0], 2), [1, 0]),
        Check.ints_equal("layout.resolve_reshape", Layout.resolve_reshape(6, [2, -1]), [2, 3]),
        Check.ints_equal("layout.swap_perm", Layout.swap_perm(3, 0, 2), [2, 1, 0]),
        Check.ints_equal("layout.moveaxis_perm", Layout.moveaxis_perm(3, [0], [2]), [1, 2, 0]),
    ]


def layout_scalars():
    return [
        Check.equal("layout.flat_size", Layout.flat_size([2, 3, 4]), 24),
        Check.equal("layout.ravel_index", Layout.ravel_index([1, 2], [2, 3]), 5),
        Check.equal("layout.axis_of", Layout.axis_of([2, 3], -1), 1),
        Check.equal("layout.normalize_axis", Layout.normalize_axis(-1, 3, ""), 2),
        Check.equal("layout.contains", Layout.contains([2, 3], 3), True),
        Check.equal("layout.contains.absent", Layout.contains([2, 3], 4), False),
        Check.equal("layout.compatible", Layout.compatible([2, 1], [3]), True),
        Check.equal("layout.incompatible", Layout.compatible([2, 3], [4]), False),
    ]


def math_cases():
    return [
        Check.equal("math.pow10", Math.pow10(3), 1000),
        Check.equal("math.trunc_int", Math.trunc_int(-2.75), -2),
        Check.equal("math.ceil_int", Math.ceil_int(-2.75), -2),
        Check.equal("math.floor_int", Math.floor_int(-2.75), -3),
        Check.close("math.exp_of", Math.exp_of(0.0), 1.0, S.TIGHT, S.ATOL),
        Check.close("math.sqrt_of", Math.sqrt_of(4.0), 2.0, S.TIGHT, S.ATOL),
        Check.close("math.tanh_of", Math.tanh_of(0.0), 0.0, S.TIGHT, S.ATOL),
        Check.close("math.ln_of", Math.ln_of(1.0), 0.0, S.TIGHT, S.ATOL),
        Check.close("math.variance", Math.variance(2.0, 2, 1), 2.0, S.TIGHT, S.ATOL),
        Check.truth("math.variance.nan", Check.near(Math.variance(0.0, 1, 1), Check.float_of("nan"), S.TIGHT, S.ATOL), "zero degrees of freedom produces NaN"),
    ]


def storage_cases():
    buffer = Storage.lift([1.0, 2.0])
    built = perform(Storage.Buffer.build(2, lambda i: toFloat(i + 1)))
    return [
        Check.close_all("storage.lower", Storage.lower(buffer), [1.0, 2.0], S.TIGHT, S.ATOL),
        Check.close_all("storage.Buffer.build", Storage.lower(built), [1.0, 2.0], S.TIGHT, S.ATOL),
        Check.equal("storage.length_of", Storage.length_of(buffer), 2),
        Check.equal("storage.at", Storage.at(buffer, 1), 2.0),
        Check.equal("storage.add_f", Storage.add_f(2.0, 3.0), 5.0),
        Check.equal("storage.mul_f", Storage.mul_f(2.0, 3.0), 6.0),
        Check.equal("storage.max_f", Storage.max_f(-2.0, -3.0), -2.0),
        Check.equal("storage.min_f", Storage.min_f(-2.0, -3.0), -3.0),
        Check.truth("storage.infinity", Storage.infinity() == Check.float_of("inf"), "positive infinity"),
        Check.truth("storage.not_a_number", Check.near(Storage.not_a_number(), Check.float_of("nan"), S.TIGHT, S.ATOL), "NaN"),
    ]


def array_cases():
    a = Array.array2([[1.0, 2.0], [3.0, 4.0]])
    return [
        Check.close_all("array.array1", Array.tolist(Array.array1([1.0, 2.0])), [1.0, 2.0], S.TIGHT, S.ATOL),
        Check.close_all("array.array3", Array.tolist(Array.array3([[[1.0, 2.0]]])), [1.0, 2.0], S.TIGHT, S.ATOL),
        Check.close_all("array.NDArray", Array.tolist(Array.NDArray(shape=[2], data=Storage.lift([1.0, 2.0]))), [1.0, 2.0], S.TIGHT, S.ATOL),
        Check.equal("array.size", Array.size(a), 4),
        Check.equal("array.ndim", Array.ndim(a), 2),
        Check.equal("array.empty", Array.size(Array.empty()), 0),
        Check.close_all("array.scalar", Array.tolist(Array.scalar(3.0)), [3.0], S.TIGHT, S.ATOL),
        Check.close_all("array.ravel", Array.tolist(Array.ravel(a)), [1.0, 2.0, 3.0, 4.0], S.TIGHT, S.ATOL),
        Check.close_all("array.flatten", Array.flatten(a), [1.0, 2.0, 3.0, 4.0], S.TIGHT, S.ATOL),
        Check.close_all("array.squeeze_axis", Array.tolist(Array.squeeze_axis(Array.expand_dims(a, 0), 0)), [1.0, 2.0, 3.0, 4.0], S.TIGHT, S.ATOL),
    ]


def array_fill_cases():
    a = Array.array1([1.0, 2.0])
    return [
        Check.close_all("array.zeros_like", Array.tolist(Array.zeros_like(a)), [0.0, 0.0], S.TIGHT, S.ATOL),
        Check.close_all("array.ones_like", Array.tolist(Array.ones_like(a)), [1.0, 1.0], S.TIGHT, S.ATOL),
        Check.close_all("array.full_like", Array.tolist(Array.full_like(a, 3.0)), [3.0, 3.0], S.TIGHT, S.ATOL),
        Check.close_all("array.identity", Array.tolist(Array.identity(2)), S.atl(Expected.EYE_OUT, 0), S.TIGHT, S.ATOL),
        Check.close_all("array.eye_m", Array.tolist(Array.eye_m(2, 2)), S.atl(Expected.EYE_OUT, 0), S.TIGHT, S.ATOL),
    ]


def checked_cases(b):
    with Errors.raising:
        groups = [error_cases(b), layout_lists(), layout_scalars(), math_cases(), storage_cases(), array_cases(), array_fill_cases()]
        return Success(value=foldList(groups, [], listConcat))


def run(b):
    match checked_cases(b):
        case Success(value=value):
            return value
        case Error(message=message):
            return [Check.failed(f"contracts.{b}", message)]
