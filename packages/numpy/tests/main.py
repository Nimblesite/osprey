# osprey: flavor=python
"""@osprey/numpy's suite: every value computed by the real numpy in
scripts/oracles/numpy.py, compared against the port through @osprey/testing,
and run under BOTH the host and the GPU storage handlers.

Floats compare with numpy's isclose rule: rtol 1e-12 for results built only
from + - * / and sqrt, rtol 1e-9 for results downstream of exp/ln/tanh (libm
last-digit differences), and rtol 1e-9 for var/std (numpy's compensated mean
differs in the last ulp). Error cases compare the ValueError message with
str(exception) from the real numpy."""
from numpy import Array, Device, Errors, Host, Layout, Linalg
from numpy_expected import Expected
from testing import Check

TIGHT = 0.000000000001
LOOSE = 0.000000001
ATOL = 0.0


def atl(xs, i):
    return unwrap_or(listGet(xs, i), [])


def atf(xs, i):
    return unwrap_or(listGet(xs, i), 0.0)


def ati(xs, i):
    return unwrap_or(listGet(xs, i), 0)


def ats(xs, i):
    return unwrap_or(listGet(xs, i), "")


def atb(xs, i):
    return unwrap_or(listGet(xs, i), False)


def f(xs, i, j):
    return atf(atl(xs, i), j)


def it(xs, i, j):
    return ati(atl(xs, i), j)


def arr(vals, shape):
    return Array.array(vals, shape)


def finish_list(name, compute, want, tol):
    match Errors.checked_values(compute):
        case Success(value=value):
            return Check.close_all(name, value, want, tol, ATOL)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def finish_float(name, compute, want, tol):
    match Errors.checked_float(compute):
        case Success(value=value):
            return Check.close(name, value, want, tol, ATOL)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def finish_int(name, compute, want):
    match Errors.checked_int(compute):
        case Success(value=value):
            return Check.equal(name, value, want)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def finish_ints(name, compute, want):
    match Errors.checked_ints(compute):
        case Success(value=value):
            return Check.ints_equal(name, value, want)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def finish_bool(name, compute, want):
    match Errors.checked_bool(compute):
        case Success(value=value):
            return Check.equal(name, value, want)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def creation_cases(b):
    ar = fold(range(len(Expected.ARANGE)), [], lambda acc, i: listAppend(acc, finish_list(f"arange.{i}.{b}", lambda: Array.tolist(Array.arange(f(Expected.ARANGE, i, 0), f(Expected.ARANGE, i, 1), f(Expected.ARANGE, i, 2))), atl(Expected.ARANGE_OUT, i), TIGHT)))
    lp = fold(range(len(Expected.LINSPACE_NUM)), [], lambda acc, i: listAppend(acc, finish_list(f"linspace.{i}.{b}", lambda: Array.tolist(Array.linspace(f(Expected.LINSPACE_F, i, 0), f(Expected.LINSPACE_F, i, 1), ati(Expected.LINSPACE_NUM, i))), atl(Expected.LINSPACE_OUT, i), TIGHT)))
    eye = fold(range(len(Expected.EYE)), [], lambda acc, i: listAppend(acc, finish_list(f"eye.{i}.{b}", lambda: Array.tolist(Array.eye(ati(Expected.EYE, i))), atl(Expected.EYE_OUT, i), TIGHT)))
    tri = fold(range(len(Expected.TRI)), [], lambda acc, i: listAppend(acc, finish_list(f"tri.{i}.{b}", lambda: Array.tolist(Array.tri_k(it(Expected.TRI, i, 0), it(Expected.TRI, i, 1), it(Expected.TRI, i, 2))), atl(Expected.TRI_OUT, i), TIGHT)))
    diag = fold(range(len(Expected.DIAG_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"diag.{i}.{b}", lambda: Array.tolist(Array.diag(arr(atl(Expected.DIAG_VALS, i), atl(Expected.DIAG_SHAPES, i)))), atl(Expected.DIAG_OUT, i), TIGHT)))
    zeros = fold(range(len(Expected.ZEROS)), [], lambda acc, i: listAppend(acc, finish_list(f"zeros.{i}.{b}", lambda: Array.tolist(Array.zeros(atl(Expected.ZEROS, i))), atl(Expected.ZEROS_OUT, i), TIGHT)))
    ones = fold(range(len(Expected.ZEROS)), [], lambda acc, i: listAppend(acc, finish_list(f"ones.{i}.{b}", lambda: Array.tolist(Array.ones(atl(Expected.ZEROS, i))), atl(Expected.ONES_OUT, i), TIGHT)))
    full = fold(range(len(Expected.FULL_VALUES)), [], lambda acc, i: listAppend(acc, finish_list(f"full.{i}.{b}", lambda: Array.tolist(Array.full([2, 3], atf(Expected.FULL_VALUES, i))), atl(Expected.FULL_OUT, i), TIGHT)))
    return listConcat(listConcat(listConcat(ar, lp), listConcat(eye, tri)), listConcat(listConcat(diag, zeros), listConcat(ones, full)))


def reshape_cases(b):
    return fold(range(len(Expected.RESHAPE_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"reshape.{i}.{b}", lambda: Array.tolist(Array.reshape(arr(atl(Expected.RESHAPE_VALS, i), atl(Expected.RESHAPE_SHAPES, i)), atl(Expected.RESHAPE_NEW, i))), atl(Expected.RESHAPE_OUT, i), TIGHT), finish_ints(f"reshape.shape.{i}.{b}", lambda: Array.reshape(arr(atl(Expected.RESHAPE_VALS, i), atl(Expected.RESHAPE_SHAPES, i)), atl(Expected.RESHAPE_NEW, i)).shape, atl(Expected.RESHAPE_OUT_SHAPES, i))]))


def transpose_of(i):
    a = arr(atl(Expected.TRANSPOSE_VALS, i), atl(Expected.TRANSPOSE_SHAPES, i))
    axes = atl(Expected.TRANSPOSE_AXES, i)
    return Array.transpose(a, axes) if len(axes) > 0 else Array.transpose_all(a)


def transpose_cases(b):
    return fold(range(len(Expected.TRANSPOSE_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"transpose.{i}.{b}", lambda: Array.tolist(transpose_of(i)), atl(Expected.TRANSPOSE_OUT, i), TIGHT), finish_ints(f"transpose.shape.{i}.{b}", lambda: transpose_of(i).shape, atl(Expected.TRANSPOSE_OUT_SHAPES, i))]))


def swapaxes_cases(b):
    return fold(range(len(Expected.SWAPAXES_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"swapaxes.{i}.{b}", lambda: Array.tolist(Array.swapaxes(arr(atl(Expected.SWAPAXES_VALS, i), atl(Expected.SWAPAXES_SHAPES, i)), ati(Expected.SWAPAXES_A1, i), ati(Expected.SWAPAXES_A2, i))), atl(Expected.SWAPAXES_OUT, i), TIGHT)))


def moveaxis_of(i):
    return Array.moveaxis(arr(atl(Expected.MOVEAXIS_VALS, i), atl(Expected.MOVEAXIS_SHAPES, i)), atl(Expected.MOVEAXIS_SRC, i), atl(Expected.MOVEAXIS_DST, i))


def moveaxis_cases(b):
    return fold(range(len(Expected.MOVEAXIS_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"moveaxis.{i}.{b}", lambda: Array.tolist(moveaxis_of(i)), atl(Expected.MOVEAXIS_OUT, i), TIGHT), finish_ints(f"moveaxis.shape.{i}.{b}", lambda: moveaxis_of(i).shape, atl(Expected.MOVEAXIS_OUT_SHAPES, i))]))


def expand_dims_cases(b):
    return fold(range(len(Expected.ED_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"expand_dims.{i}.{b}", lambda: Array.tolist(Array.expand_dims(arr(atl(Expected.ED_VALS, i), atl(Expected.ED_SHAPES, i)), ati(Expected.ED_AXIS, i))), atl(Expected.ED_OUT, i), TIGHT), finish_ints(f"expand_dims.shape.{i}.{b}", lambda: Array.expand_dims(arr(atl(Expected.ED_VALS, i), atl(Expected.ED_SHAPES, i)), ati(Expected.ED_AXIS, i)).shape, atl(Expected.ED_OUT_SHAPES, i))]))


def squeeze_cases(b):
    return fold(range(len(Expected.SQ_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"squeeze.{i}.{b}", lambda: Array.tolist(Array.squeeze(arr(atl(Expected.SQ_VALS, i), atl(Expected.SQ_SHAPES, i)))), atl(Expected.SQ_OUT, i), TIGHT), finish_ints(f"squeeze.shape.{i}.{b}", lambda: Array.squeeze(arr(atl(Expected.SQ_VALS, i), atl(Expected.SQ_SHAPES, i))).shape, atl(Expected.SQ_OUT_SHAPES, i))]))


def broadcast_shapes_cases(b):
    return fold(range(len(Expected.BS_OUT)), [], lambda acc, i: listAppend(acc, finish_ints(f"broadcast_shapes.{i}.{b}", lambda: Array.broadcast_shapes(atl(Expected.BS_SHAPES, i)), atl(Expected.BS_OUT, i))))


def broadcast_to_of(i):
    return Array.broadcast_to(arr(atl(Expected.BT_VALS, i), atl(Expected.BT_SHAPES, i)), atl(Expected.BT_NEW, i))


def broadcast_to_cases(b):
    return fold(range(len(Expected.BT_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"broadcast_to.{i}.{b}", lambda: Array.tolist(broadcast_to_of(i)), atl(Expected.BT_OUT, i), TIGHT), finish_ints(f"broadcast_to.shape.{i}.{b}", lambda: broadcast_to_of(i).shape, atl(Expected.BT_OUT_SHAPES, i))]))


def concat_arrays(i):
    vals = atl(Expected.CONCAT_VALS, i)
    shapes = atl(Expected.CONCAT_SHAPES, i)
    return toList(map(range(len(vals)), lambda t: arr(atl(vals, t), atl(shapes, t))))


def concat_cases(b):
    return fold(range(len(Expected.CONCAT_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"concat.{i}.{b}", lambda: Array.tolist(Array.concatenate(concat_arrays(i), ati(Expected.CONCAT_AXIS, i))), atl(Expected.CONCAT_OUT, i), TIGHT), finish_ints(f"concat.shape.{i}.{b}", lambda: Array.concatenate(concat_arrays(i), ati(Expected.CONCAT_AXIS, i)).shape, atl(Expected.CONCAT_OUT_SHAPES, i))]))


def stack_arrays(i):
    vals = atl(Expected.STACK_VALS, i)
    shapes = atl(Expected.STACK_SHAPES, i)
    return toList(map(range(len(vals)), lambda t: arr(atl(vals, t), atl(shapes, t))))


def stack_cases(b):
    return fold(range(len(Expected.STACK_OUT)), [], lambda acc, i: listConcat(acc, [finish_list(f"stack.{i}.{b}", lambda: Array.tolist(Array.stack(stack_arrays(i), ati(Expected.STACK_AXIS, i))), atl(Expected.STACK_OUT, i), TIGHT), finish_ints(f"stack.shape.{i}.{b}", lambda: Array.stack(stack_arrays(i), ati(Expected.STACK_AXIS, i)).shape, atl(Expected.STACK_OUT_SHAPES, i))]))


def take_cases(b):
    return fold(range(len(Expected.TAKE_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"take.{i}.{b}", lambda: Array.tolist(Array.take(arr(atl(Expected.TAKE_VALS, i), atl(Expected.TAKE_SHAPES, i)), atl(Expected.TAKE_IDX, i), ati(Expected.TAKE_AXIS, i))), atl(Expected.TAKE_OUT, i), TIGHT)))


def flip_of(i):
    a = arr(atl(Expected.FLIP_VALS, i), atl(Expected.FLIP_SHAPES, i))
    ax = atl(Expected.FLIP_AXIS, i)
    return Array.flip(a, ati(ax, 0)) if len(ax) == 1 else Array.flip_all(a)


def flip_cases(b):
    return fold(range(len(Expected.FLIP_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"flip.{i}.{b}", lambda: Array.tolist(flip_of(i)), atl(Expected.FLIP_OUT, i), TIGHT)))


def roll_cases(b):
    return fold(range(len(Expected.ROLL_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"roll.{i}.{b}", lambda: Array.tolist(Array.roll(arr(atl(Expected.ROLL_VALS, i), atl(Expected.ROLL_SHAPES, i)), ati(Expected.ROLL_SHIFT, i), ati(Expected.ROLL_AXIS, i))), atl(Expected.ROLL_OUT, i), TIGHT)))


def repeat_cases(b):
    return fold(range(len(Expected.RP_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"repeat.{i}.{b}", lambda: Array.tolist(Array.repeat(arr(atl(Expected.RP_VALS, i), atl(Expected.RP_SHAPES, i)), ati(Expected.RP_REPS, i), ati(Expected.RP_AXIS, i))), atl(Expected.RP_OUT, i), TIGHT)))


def tile_cases(b):
    return fold(range(len(Expected.TILE_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"tile.{i}.{b}", lambda: Array.tolist(Array.tile(arr(atl(Expected.TILE_VALS, i), atl(Expected.TILE_SHAPES, i)), atl(Expected.TILE_REPS, i))), atl(Expected.TILE_OUT, i), TIGHT)))


def shape_cases(b):
    return listConcat(listConcat(listConcat(listConcat(reshape_cases(b), transpose_cases(b)), listConcat(swapaxes_cases(b), moveaxis_cases(b))), listConcat(listConcat(expand_dims_cases(b), squeeze_cases(b)), listConcat(broadcast_shapes_cases(b), broadcast_to_cases(b)))), listConcat(listConcat(listConcat(concat_cases(b), stack_cases(b)), listConcat(take_cases(b), flip_cases(b))), listConcat(listConcat(roll_cases(b), repeat_cases(b)), tile_cases(b))))


def bin_compute(tag, a, b):
    match tag:
        case "add":
            return Array.add(a, b)
        case "sub":
            return Array.subtract(a, b)
        case "mul":
            return Array.multiply(a, b)
        case "div":
            return Array.divide(a, b)
        case "maximum":
            return Array.maximum(a, b)
        case "minimum":
            return Array.minimum(a, b)
        case "equal":
            return Array.equal(a, b)
        case "not_equal":
            return Array.not_equal(a, b)
        case "less":
            return Array.less(a, b)
        case "less_equal":
            return Array.less_equal(a, b)
        case "greater":
            return Array.greater(a, b)
        case _:
            return Array.greater_equal(a, b)


def bin_out(tag):
    match tag:
        case "add":
            return Expected.BIN_ADD
        case "sub":
            return Expected.BIN_SUB
        case "mul":
            return Expected.BIN_MUL
        case "div":
            return Expected.BIN_DIV
        case "maximum":
            return Expected.BIN_MAX
        case "minimum":
            return Expected.BIN_MIN
        case "equal":
            return Expected.BIN_EQ
        case "not_equal":
            return Expected.BIN_NE
        case "less":
            return Expected.BIN_LT
        case "less_equal":
            return Expected.BIN_LE
        case "greater":
            return Expected.BIN_GT
        case _:
            return Expected.BIN_GE


def bin_cases(b, tag):
    return fold(range(len(Expected.BIN_A_VALS)), [], lambda acc, i: listAppend(acc, finish_list(f"{tag}.{i}.{b}", lambda: Array.tolist(bin_compute(tag, arr(atl(Expected.BIN_A_VALS, i), atl(Expected.BIN_A_SHAPES, i)), arr(atl(Expected.BIN_B_VALS, i), atl(Expected.BIN_B_SHAPES, i)))), atl(bin_out(tag), i), TIGHT)))


def un_compute(tag, a):
    match tag:
        case "negative":
            return Array.negative(a)
        case "absolute":
            return Array.absolute(a)
        case "exp":
            return Array.exp(a)
        case "log":
            return Array.log(a)
        case "sqrt":
            return Array.sqrt(a)
        case "tanh":
            return Array.tanh(a)
        case "square":
            return Array.square(a)
        case "reciprocal":
            return Array.reciprocal(a)
        case _:
            return Array.sign(a)


def un_out(tag):
    match tag:
        case "negative":
            return Expected.UN_NEG
        case "absolute":
            return Expected.UN_ABS
        case "exp":
            return Expected.UN_EXP
        case "log":
            return Expected.UN_LOG
        case "sqrt":
            return Expected.UN_SQRT
        case "tanh":
            return Expected.UN_TANH
        case "square":
            return Expected.UN_SQR
        case "reciprocal":
            return Expected.UN_RECIP
        case _:
            return Expected.UN_SIGN


def un_tol(tag):
    return LOOSE if tag == "exp" or tag == "log" or tag == "tanh" else TIGHT


def un_cases(b, tag):
    return fold(range(len(Expected.UNARY_VALS)), [], lambda acc, i: listAppend(acc, finish_list(f"{tag}.{i}.{b}", lambda: Array.tolist(un_compute(tag, arr(atl(Expected.UNARY_VALS, i), atl(Expected.UNARY_SHAPES, i)))), atl(un_out(tag), i), un_tol(tag))))


def where_cases(b):
    return fold(range(len(Expected.WHERE_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"where.{i}.{b}", lambda: Array.tolist(Array.where(arr(atl(Expected.WHERE_C_VALS, i), atl(Expected.WHERE_C_SHAPES, i)), arr(atl(Expected.WHERE_A_VALS, i), atl(Expected.WHERE_A_SHAPES, i)), arr(atl(Expected.WHERE_B_VALS, i), atl(Expected.WHERE_B_SHAPES, i)))), atl(Expected.WHERE_OUT, i), TIGHT)))


def clip_cases(b):
    return fold(range(len(Expected.CLIP_OUT)), [], lambda acc, i: listAppend(acc, finish_list(f"clip.{i}.{b}", lambda: Array.tolist(Array.clip(arr(atl(Expected.UNARY_VALS, i), atl(Expected.UNARY_SHAPES, i)), f(Expected.CLIP, i, 0), f(Expected.CLIP, i, 1))), atl(Expected.CLIP_OUT, i), TIGHT)))


BIN_TAGS = ["add", "sub", "mul", "div", "maximum", "minimum", "equal", "not_equal", "less", "less_equal", "greater", "greater_equal"]
UN_TAGS = ["negative", "absolute", "exp", "log", "sqrt", "tanh", "square", "reciprocal", "sign"]


def elementwise_cases(b):
    bt = fold(range(len(BIN_TAGS)), [], lambda acc, t: listConcat(acc, bin_cases(b, ats(BIN_TAGS, t))))
    ut = fold(range(len(UN_TAGS)), [], lambda acc, t: listConcat(acc, un_cases(b, ats(UN_TAGS, t))))
    return listConcat(listConcat(bt, ut), listConcat(where_cases(b), clip_cases(b)))


ERROR_KEYS = [
    "BROADCAST", "BCAST_SHAPES", "BCAST_SHAPES_3", "RESHAPE", "RESHAPE_NEG",
    "RESHAPE_NEG0", "RESHAPE_NEGX", "AXIS", "AXIS_NEG", "TRANSPOSE",
    "TRANSPOSE_REPEAT", "TRANSPOSE_BAD", "MOVEAXIS_LEN", "MOVEAXIS_REPEAT",
    "CONCAT_DIM", "CONCAT_SIZE", "CONCAT_EMPTY", "STACK_SHAPE", "STACK_EMPTY",
    "TAKE", "SQUEEZE", "MATMUL", "MATMUL_1D", "DOT", "INNER", "VDOT", "EYE",
    "LINSPACE", "BCAST_TO_DIMS", "BCAST_TO", "RAGGED2", "RAGGED3",
]



def red_arr(i):
    return arr(atl(Expected.RED_VALS, i), atl(Expected.RED_SHAPES, i))


def reduction_cases(b):
    n = len(Expected.RED_VALS)
    sum_ = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"sum.{i}.{b}", lambda: Array.sum(red_arr(i)), atf(Expected.SUM_ALL, i), TIGHT)))
    mean_ = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"mean.{i}.{b}", lambda: Array.mean(red_arr(i)), atf(Expected.MEAN_ALL, i), TIGHT)))
    max_ = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"max.{i}.{b}", lambda: Array.max(red_arr(i)), atf(Expected.MAX_ALL, i), TIGHT)))
    min_ = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"min.{i}.{b}", lambda: Array.min(red_arr(i)), atf(Expected.MIN_ALL, i), TIGHT)))
    prod_ = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"prod.{i}.{b}", lambda: Array.prod(red_arr(i)), atf(Expected.PROD_ALL, i), TIGHT)))
    var0 = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"var0.{i}.{b}", lambda: Array.var(red_arr(i), 0), atf(Expected.VAR_ALL0, i), LOOSE)))
    std0 = fold(range(n), [], lambda acc, i: listAppend(acc, finish_float(f"std0.{i}.{b}", lambda: Array.std(red_arr(i), 0), atf(Expected.STD_ALL0, i), LOOSE)))
    groups = [sum_, mean_, max_, min_, prod_, var0, std0]
    return fold(range(len(groups)), [], lambda acc, t: listConcat(acc, atl(groups, t)))



def layout_cases(backend):
    return [Check.equal(f"offset.broadcast.{backend}", Layout.offset_of(4, [2, 3], [0, 1]), 1), Check.equal(f"offset.permuted.{backend}", Layout.offset_of(3, [3, 2], [1, 3]), 4)]


def cases(backend):
    offsets = layout_cases(backend)
    a = listConcat(creation_cases(backend), shape_cases(backend))
    b = elementwise_cases(backend)
    return listConcat(offsets, listConcat(a, listConcat(b, reduction_cases(backend))))


def main():
    match Check.strictly(lambda: Check.report(listConcat(Host.on_host(lambda: cases("host")), Device.on_gpu(lambda: cases("gpu"))))):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
