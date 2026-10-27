"""CPython oracle for @osprey/numpy: every value and error message the suite
compares against.

Computes each case with the real numpy 2.5.4 and writes packages/numpy/tests/
expected.py ([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python scripts/oracles/numpy.py`. Osprey lists are homogeneous, so every
field (values, shapes, axes, indices) is written as its own parallel constant:
floats with floats, ints with ints. Non-finite floats are written as repr
strings and decoded by the suite with `testing.Check.float_of`.
"""
import os
import sys

from expected_module import write

import numpy

V = numpy.__version__


def arr(vals, shape):
    return numpy.array(vals, dtype=numpy.float64).reshape(shape)


def flat(x):
    return numpy.asarray(x, dtype=numpy.float64).ravel().tolist()


def iflat(x):
    return numpy.asarray(x).ravel().tolist()


def split(pairs):
    """[(vals, shape), ...] -> (vals_list, shapes_list)."""
    return [v for v, _ in pairs], [s for _, s in pairs]


# --- creation ------------------------------------------------------------

ARANGE = [[0.0, 10.0, 2.0], [0.0, 1.0, 0.3], [5.0, 1.0, -1.0], [0.0, 1.0, 0.1],
          [1.0, 3.0, 0.5], [-2.0, 2.0, 1.0], [3.0, 3.0, 1.0], [0.0, 0.5, 0.25]]
ARANGE_OUT = [flat(numpy.arange(*c)) for c in ARANGE]

LINSPACE_F = [[0.0, 1.0], [1.0, 0.0], [0.0, 1.0], [0.0, 1.0], [0.0, 0.1], [-1.0, 1.0]]
LINSPACE_NUM = [5, 4, 1, 0, 11, 3]
LINSPACE_OUT = [flat(numpy.linspace(a, b, n)) for (a, b), n in zip(LINSPACE_F, LINSPACE_NUM)]

EYE = [2, 1, 0, 3]
EYE_OUT = [flat(numpy.eye(n)) for n in EYE]

TRI = [[3, 3, 0], [3, 4, 0], [3, 4, 1], [3, 4, -1], [0, 0, 0]]
TRI_OUT = [flat(numpy.tri(n, m, k)) for n, m, k in TRI]

DIAG_VALS, DIAG_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]), ([1.0, 2.0, 3.0, 4.0], [2, 2])])
DIAG_OUT = [flat(numpy.diag(arr(v, s))) for v, s in zip(DIAG_VALS, DIAG_SHAPES)]

ZEROS = [[2, 3], [], [1], [2, 2, 2]]
ZEROS_OUT = [flat(numpy.zeros(s)) for s in ZEROS]
ONES_OUT = [flat(numpy.ones(s)) for s in ZEROS]
FULL_VALUES = [2.5, -1.0, 0.0]
FULL_OUT = [flat(numpy.full([2, 3], v)) for v in FULL_VALUES]

# --- shape operations ----------------------------------------------------

RESHAPE_VALS, RESHAPE_SHAPES = split([([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                      ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                      ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                      ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [6]),
                                      ([1.0, 2.0, 3.0, 4.0], [2, 2]),
                                      ([1.0, 2.0, 3.0, 4.0], [2, 2])])
RESHAPE_NEW = [[3, 2], [6], [2, -1], [-1, 2], [4], [1, 4]]
RESHAPE_OUT = [flat(arr(v, s).reshape(ns)) for (v, s), ns in zip(zip(RESHAPE_VALS, RESHAPE_SHAPES), RESHAPE_NEW)]
RESHAPE_OUT_SHAPES = [list(arr(v, s).reshape(ns).shape) for (v, s), ns in zip(zip(RESHAPE_VALS, RESHAPE_SHAPES), RESHAPE_NEW)]

TRANSPOSE_VALS, TRANSPOSE_SHAPES = split([([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                          ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                          (list(map(float, range(24))), [2, 3, 4]),
                                          (list(map(float, range(24))), [2, 3, 4])])
TRANSPOSE_AXES = [[1, 0], [], [2, 0, 1], []]
TRANSPOSE_OUT = [flat(arr(v, s).transpose(a) if a else arr(v, s).transpose()) for (v, s), a in zip(zip(TRANSPOSE_VALS, TRANSPOSE_SHAPES), TRANSPOSE_AXES)]
TRANSPOSE_OUT_SHAPES = [list((arr(v, s).transpose(a) if a else arr(v, s).transpose()).shape) for (v, s), a in zip(zip(TRANSPOSE_VALS, TRANSPOSE_SHAPES), TRANSPOSE_AXES)]

SWAPAXES_VALS, SWAPAXES_SHAPES = split([([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                        (list(map(float, range(24))), [2, 3, 4]),
                                        (list(map(float, range(24))), [2, 3, 4])])
SWAPAXES_A1 = [0, 0, -1]
SWAPAXES_A2 = [1, 2, 0]
SWAPAXES_OUT = [flat(arr(v, s).swapaxes(a, b)) for (v, s), a, b in zip(zip(SWAPAXES_VALS, SWAPAXES_SHAPES), SWAPAXES_A1, SWAPAXES_A2)]

MOVEAXIS_VALS, MOVEAXIS_SHAPES = split([(list(map(float, range(24))), [2, 3, 4])] * 3)
MOVEAXIS_SRC = [[0], [0, 1], [2]]
MOVEAXIS_DST = [[2], [2, 1], [0]]
MOVEAXIS_OUT = [flat(numpy.moveaxis(arr(v, s), src, dst)) for (v, s), src, dst in zip(zip(MOVEAXIS_VALS, MOVEAXIS_SHAPES), MOVEAXIS_SRC, MOVEAXIS_DST)]
MOVEAXIS_OUT_SHAPES = [list(numpy.moveaxis(arr(v, s), src, dst).shape) for (v, s), src, dst in zip(zip(MOVEAXIS_VALS, MOVEAXIS_SHAPES), MOVEAXIS_SRC, MOVEAXIS_DST)]

ED_VALS, ED_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0], [3]),
                            ([1.0, 2.0, 3.0, 4.0], [2, 2]), ([1.0, 2.0, 3.0, 4.0], [2, 2])])
ED_AXIS = [0, -1, 1, 2]
ED_OUT = [flat(numpy.expand_dims(arr(v, s), a)) for (v, s), a in zip(zip(ED_VALS, ED_SHAPES), ED_AXIS)]
ED_OUT_SHAPES = [list(numpy.expand_dims(arr(v, s), a).shape) for (v, s), a in zip(zip(ED_VALS, ED_SHAPES), ED_AXIS)]

SQ_VALS, SQ_SHAPES = split([([1.0, 2.0, 3.0], [1, 3]), ([1.0, 2.0, 3.0], [3, 1]),
                            (list(map(float, range(6))), [2, 1, 3]), ([1.0, 2.0], [1, 1, 2, 1])])
SQ_OUT = [flat(numpy.squeeze(arr(v, s))) for v, s in zip(SQ_VALS, SQ_SHAPES)]
SQ_OUT_SHAPES = [list(numpy.squeeze(arr(v, s)).shape) for v, s in zip(SQ_VALS, SQ_SHAPES)]

BS_SHAPES = [[[2, 3], [2, 3]], [[2, 1], [1, 3]], [[5, 1, 3], [1, 2, 1], [2, 1]], [[], [2, 3]], [[1], [3]]]
BS_OUT = [list(numpy.broadcast_shapes(*shps)) for shps in BS_SHAPES]

BT_VALS, BT_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0], [2, 1]), ([1.0, 2.0], [1, 2]), ([5.0], [1])])
BT_NEW = [[2, 3], [2, 3], [3, 2], [3]]
BT_OUT = [flat(numpy.broadcast_to(arr(v, s), ns)) for (v, s), ns in zip(zip(BT_VALS, BT_SHAPES), BT_NEW)]
BT_OUT_SHAPES = [list(numpy.broadcast_to(arr(v, s), ns).shape) for (v, s), ns in zip(zip(BT_VALS, BT_SHAPES), BT_NEW)]

CONCAT_VALS = [[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]], [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
               [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]], [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0]]]
CONCAT_SHAPES = [[[3], [3]], [[2], [2], [2]], [[2, 2], [2, 2]], [[2, 2], [1, 2]]]
CONCAT_AXIS = [0, 0, 1, 0]
CONCAT_OUT = [flat(numpy.concatenate([arr(v, s) for v, s in zip(vals, shapes)], axis=ax)) for vals, shapes, ax in zip(CONCAT_VALS, CONCAT_SHAPES, CONCAT_AXIS)]
CONCAT_OUT_SHAPES = [list(numpy.concatenate([arr(v, s) for v, s in zip(vals, shapes)], axis=ax).shape) for vals, shapes, ax in zip(CONCAT_VALS, CONCAT_SHAPES, CONCAT_AXIS)]

STACK_VALS = [[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]], [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
              [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]]
STACK_SHAPES = [[[3], [3]], [[3], [3]], [[2, 2], [2, 2]]]
STACK_AXIS = [0, 1, 2]
STACK_OUT = [flat(numpy.stack([arr(v, s) for v, s in zip(vals, shapes)], axis=ax)) for vals, shapes, ax in zip(STACK_VALS, STACK_SHAPES, STACK_AXIS)]
STACK_OUT_SHAPES = [list(numpy.stack([arr(v, s) for v, s in zip(vals, shapes)], axis=ax).shape) for vals, shapes, ax in zip(STACK_VALS, STACK_SHAPES, STACK_AXIS)]

TAKE_VALS, TAKE_SHAPES = split([([1.0, 2.0, 3.0, 4.0], [4]), (list(map(float, range(6))), [2, 3]),
                                (list(map(float, range(6))), [2, 3]), (list(map(float, range(24))), [2, 3, 4])])
TAKE_IDX = [[0, 2, -1], [1, 0, 1], [2, 0], [0, 1]]
TAKE_AXIS = [0, 0, 1, 1]
TAKE_OUT = [flat(numpy.take(arr(v, s), idx, axis=ax)) for (v, s), idx, ax in zip(zip(TAKE_VALS, TAKE_SHAPES), TAKE_IDX, TAKE_AXIS)]
TAKE_OUT_SHAPES = [list(numpy.take(arr(v, s), idx, axis=ax).shape) for (v, s), idx, ax in zip(zip(TAKE_VALS, TAKE_SHAPES), TAKE_IDX, TAKE_AXIS)]

FLIP_VALS, FLIP_SHAPES = split([([1.0, 2.0, 3.0], [3]), (list(map(float, range(6))), [2, 3]),
                                (list(map(float, range(6))), [2, 3]), (list(map(float, range(6))), [2, 3])])
FLIP_AXIS = [[0], [0], [1], []]
FLIP_OUT = [flat(numpy.flip(arr(v, s), axis=ax[0]) if ax else numpy.flip(arr(v, s))) for (v, s), ax in zip(zip(FLIP_VALS, FLIP_SHAPES), FLIP_AXIS)]

ROLL_VALS, ROLL_SHAPES = split([([1.0, 2.0, 3.0, 4.0], [4]), (list(map(float, range(6))), [2, 3]),
                                (list(map(float, range(6))), [2, 3])])
ROLL_SHIFT = [1, 2, -1]
ROLL_AXIS = [0, 1, 0]
ROLL_OUT = [flat(numpy.roll(arr(v, s), sh, axis=ax)) for (v, s), sh, ax in zip(zip(ROLL_VALS, ROLL_SHAPES), ROLL_SHIFT, ROLL_AXIS)]

RP_VALS, RP_SHAPES = split([([1.0, 2.0, 3.0], [3]), (list(map(float, range(6))), [2, 3]),
                            (list(map(float, range(6))), [2, 3])])
RP_REPS = [2, 3, 2]
RP_AXIS = [0, 1, 0]
RP_OUT = [flat(numpy.repeat(arr(v, s), r, axis=ax)) for (v, s), r, ax in zip(zip(RP_VALS, RP_SHAPES), RP_REPS, RP_AXIS)]
RP_OUT_SHAPES = [list(numpy.repeat(arr(v, s), r, axis=ax).shape) for (v, s), r, ax in zip(zip(RP_VALS, RP_SHAPES), RP_REPS, RP_AXIS)]

TILE_VALS, TILE_SHAPES = split([([1.0, 2.0, 3.0], [3]), (list(map(float, range(6))), [2, 3]),
                                ([1.0, 2.0], [2]), (list(map(float, range(6))), [2, 3])])
TILE_REPS = [[2], [2, 2], [2, 3], [1, 1, 2]]
TILE_OUT = [flat(numpy.tile(arr(v, s), reps)) for (v, s), reps in zip(zip(TILE_VALS, TILE_SHAPES), TILE_REPS)]
TILE_OUT_SHAPES = [list(numpy.tile(arr(v, s), reps).shape) for (v, s), reps in zip(zip(TILE_VALS, TILE_SHAPES), TILE_REPS)]

# --- elementwise ---------------------------------------------------------

BIN_A_VALS, BIN_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0], [3]),
                                  ([1.0, 2.0, 3.0, 4.0], [2, 2]), ([1.0, 2.0], [2, 1]),
                                  ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])])
BIN_B_VALS, BIN_B_SHAPES = split([([4.0, 5.0, 6.0], [3]), ([10.0], [1]), ([1.0, 2.0], [2]),
                                  ([3.0, 4.0, 5.0], [3]), ([2.0], [1, 1])])


def bin_case(fn):
    return [flat(fn(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(BIN_A_VALS, BIN_A_SHAPES), zip(BIN_B_VALS, BIN_B_SHAPES))]


BIN_ADD = bin_case(numpy.add)
BIN_SUB = bin_case(numpy.subtract)
BIN_MUL = bin_case(numpy.multiply)
BIN_DIV = bin_case(numpy.divide)
BIN_MAX = bin_case(numpy.maximum)
BIN_MIN = bin_case(numpy.minimum)
BIN_EQ = bin_case(numpy.equal)
BIN_NE = bin_case(numpy.not_equal)
BIN_LT = bin_case(numpy.less)
BIN_LE = bin_case(numpy.less_equal)
BIN_GT = bin_case(numpy.greater)
BIN_GE = bin_case(numpy.greater_equal)

WHERE_C_VALS, WHERE_C_SHAPES = split([([1.0, 0.0, 1.0], [3]), ([1.0, 0.0], [2])])
WHERE_A_VALS, WHERE_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0], [2, 2])])
WHERE_B_VALS, WHERE_B_SHAPES = split([([10.0, 20.0, 30.0], [3]), ([100.0], [1])])
WHERE_OUT = [flat(numpy.where(arr(c, sc), arr(a, sa), arr(b, sb))) for (c, sc), (a, sa), (b, sb) in zip(zip(WHERE_C_VALS, WHERE_C_SHAPES), zip(WHERE_A_VALS, WHERE_A_SHAPES), zip(WHERE_B_VALS, WHERE_B_SHAPES))]

UNARY_VALS, UNARY_SHAPES = split([([1.0, 2.0, 4.0, 8.0], [4]), ([0.5, 2.0, 10.0, 100.0], [2, 2]),
                                  ([1.0, 3.0, 9.0], [3]), ([0.25, 4.0], [2, 1])])


def un_case(fn):
    return [flat(fn(arr(v, s))) for v, s in zip(UNARY_VALS, UNARY_SHAPES)]


UN_NEG = un_case(numpy.negative)
UN_ABS = un_case(numpy.absolute)
UN_EXP = un_case(numpy.exp)
UN_LOG = un_case(numpy.log)
UN_SQRT = un_case(numpy.sqrt)
UN_TANH = un_case(numpy.tanh)
UN_SQR = un_case(numpy.square)
UN_RECIP = un_case(numpy.reciprocal)
UN_SIGN = un_case(numpy.sign)

CLIP = [[0.0, 5.0], [-5.0, 0.0], [2.0, 3.0]]
CLIP_OUT = [flat(numpy.clip(arr(v, s), lo, hi)) for (v, s), (lo, hi) in zip(zip(UNARY_VALS, UNARY_SHAPES), CLIP)]

NONFIN_VALS, NONFIN_SHAPES = split([([0.0, 1.0, 2.0], [3]), ([-1.0, 4.0, 9.0], [3]), ([0.0, 2.0, 4.0], [3])])
LOG_ZERO = [flat(numpy.log(arr(v, s))) for v, s in zip(NONFIN_VALS[:1], NONFIN_SHAPES[:1])]
SQRT_NEG = [flat(numpy.sqrt(arr(v, s))) for v, s in zip(NONFIN_VALS[1:2], NONFIN_SHAPES[1:2])]
RECIP_ZERO = [flat(numpy.reciprocal(arr(v, s))) for v, s in zip(NONFIN_VALS[2:], NONFIN_SHAPES[2:])]

NAN_A_VALS, NAN_A_SHAPES = [[numpy.nan, 1.0, 3.0]], [[3]]
NAN_B_VALS, NAN_B_SHAPES = [[2.0, numpy.nan, 0.0]], [[3]]
NAN_MAX = [flat(numpy.maximum(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(NAN_A_VALS, NAN_A_SHAPES), zip(NAN_B_VALS, NAN_B_SHAPES))]
NAN_MIN = [flat(numpy.minimum(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(NAN_A_VALS, NAN_A_SHAPES), zip(NAN_B_VALS, NAN_B_SHAPES))]
NAN_EQ = [flat(numpy.equal(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(NAN_A_VALS, NAN_A_SHAPES), zip(NAN_B_VALS, NAN_B_SHAPES))]

# --- reductions ----------------------------------------------------------

RED_VALS, RED_SHAPES = split([([1.0, 2.0, 3.0, 4.0], [4]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                              ([1.0, -2.0, 3.5, -0.5], [2, 2]), ([5.0], [1, 1]), (list(map(float, range(24))), [2, 3, 4])])


def red_all(fn):
    return [float(fn(arr(v, s))) for v, s in zip(RED_VALS, RED_SHAPES)]


def red_axis(fn, ax, keep):
    return [flat(fn(arr(v, s), axis=ax, keepdims=keep)) for v, s in zip(RED_VALS, RED_SHAPES)]


def cum_axis(fn, ax):
    return [flat(fn(arr(v, s), axis=ax)) for v, s in zip(RED_VALS, RED_SHAPES)]


SUM_ALL = red_all(numpy.sum)
SUM_AX0 = red_axis(numpy.sum, 0, False)
SUM_AXM1 = red_axis(numpy.sum, -1, False)
SUM_AXM1K = red_axis(numpy.sum, -1, True)
MEAN_ALL = red_all(numpy.mean)
MEAN_AX0 = red_axis(numpy.mean, 0, False)
MEAN_AXM1K = red_axis(numpy.mean, -1, True)
MAX_ALL = red_all(numpy.max)
MAX_AX0 = red_axis(numpy.max, 0, False)
MIN_ALL = red_all(numpy.min)
MIN_AXM1 = red_axis(numpy.min, -1, False)
PROD_ALL = red_all(numpy.prod)
PROD_AX0 = red_axis(numpy.prod, 0, False)
VAR_ALL0 = red_all(lambda a: numpy.var(a, ddof=0))
VAR_ALL1 = red_all(lambda a: numpy.var(a, ddof=1))
VAR_AX0 = red_axis(lambda a, axis, keepdims: numpy.var(a, axis=axis, ddof=0, keepdims=keepdims), 0, False)
STD_ALL0 = red_all(lambda a: numpy.std(a, ddof=0))
STD_AXM1 = red_axis(lambda a, axis, keepdims: numpy.std(a, axis=axis, ddof=1, keepdims=keepdims), -1, False)
ARGMAX_ALL = [int(numpy.argmax(arr(v, s))) for v, s in zip(RED_VALS, RED_SHAPES)]
ARGMIN_ALL = [int(numpy.argmin(arr(v, s))) for v, s in zip(RED_VALS, RED_SHAPES)]
ARGMAX_AX0 = [iflat(numpy.argmax(arr(v, s), axis=0)) for v, s in zip(RED_VALS, RED_SHAPES)]
ARGMIN_AXM1 = [iflat(numpy.argmin(arr(v, s), axis=-1)) for v, s in zip(RED_VALS, RED_SHAPES)]
ALL_ALL = [bool(numpy.all(arr(v, s))) for v, s in zip(RED_VALS, RED_SHAPES)]
ANY_ALL = [bool(numpy.any(arr(v, s))) for v, s in zip(RED_VALS, RED_SHAPES)]
ALL_AX0 = red_axis(numpy.all, 0, False)
ANY_AXM1 = red_axis(numpy.any, -1, False)
CUMSUM_AXM1 = cum_axis(numpy.cumsum, -1)
CUMPROD_AX0 = cum_axis(numpy.cumprod, 0)

# --- linalg --------------------------------------------------------------

MM_A_VALS, MM_A_SHAPES = split([([1.0, 2.0, 3.0, 4.0], [2, 2]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                ([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0], [3]),
                                ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                (list(map(float, range(24))), [2, 3, 4]), (list(map(float, range(12))), [1, 3, 4])])
MM_B_VALS, MM_B_SHAPES = split([([5.0, 6.0, 7.0, 8.0], [2, 2]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [3, 2]),
                                ([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [3, 2]),
                                ([1.0, 2.0, 3.0], [3]),
                                (list(map(float, range(40))), [2, 4, 5]), (list(map(float, range(40))), [2, 4, 5])])
MATMUL_OUT = [flat(numpy.matmul(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(MM_A_VALS, MM_A_SHAPES), zip(MM_B_VALS, MM_B_SHAPES))]
MATMUL_SHAPES = [list(numpy.asarray(numpy.matmul(arr(a, sa), arr(b, sb))).shape) for (a, sa), (b, sb) in zip(zip(MM_A_VALS, MM_A_SHAPES), zip(MM_B_VALS, MM_B_SHAPES))]

DOT_A_VALS, DOT_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0], [2, 2]),
                                  ([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                  (list(map(float, range(12))), [2, 3, 2])])
DOT_B_VALS, DOT_B_SHAPES = split([([4.0, 5.0, 6.0], [3]), ([5.0, 6.0, 7.0, 8.0], [2, 2]),
                                  ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [3, 2]), ([1.0, 2.0, 3.0], [3]),
                                  (list(map(float, range(12))), [2, 2, 3])])
DOT_OUT = [flat(numpy.dot(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(DOT_A_VALS, DOT_A_SHAPES), zip(DOT_B_VALS, DOT_B_SHAPES))]
DOT_SHAPES = [list(numpy.asarray(numpy.dot(arr(a, sa), arr(b, sb))).shape) for (a, sa), (b, sb) in zip(zip(DOT_A_VALS, DOT_A_SHAPES), zip(DOT_B_VALS, DOT_B_SHAPES))]

OUTER_A_VALS, OUTER_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0], [2])])
OUTER_B_VALS, OUTER_B_SHAPES = split([([4.0, 5.0], [2]), ([3.0, 4.0, 5.0], [3])])
OUTER_OUT = [flat(numpy.outer(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(OUTER_A_VALS, OUTER_A_SHAPES), zip(OUTER_B_VALS, OUTER_B_SHAPES))]

INNER_A_VALS, INNER_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                      ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])])
INNER_B_VALS, INNER_B_SHAPES = split([([4.0, 5.0, 6.0], [3]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                      ([7.0, 8.0, 9.0, 10.0, 11.0, 12.0], [2, 3])])
INNER_OUT = [flat(numpy.inner(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(INNER_A_VALS, INNER_A_SHAPES), zip(INNER_B_VALS, INNER_B_SHAPES))]

VDOT_A_VALS, VDOT_A_SHAPES = split([([1.0, 2.0, 3.0], [3]), ([1.0, 2.0, 3.0, 4.0], [2, 2])])
VDOT_B_VALS, VDOT_B_SHAPES = split([([4.0, 5.0, 6.0], [3]), ([5.0, 6.0, 7.0, 8.0], [2, 2])])
VDOT_OUT = [flat(numpy.vdot(arr(a, sa), arr(b, sb))) for (a, sa), (b, sb) in zip(zip(VDOT_A_VALS, VDOT_A_SHAPES), zip(VDOT_B_VALS, VDOT_B_SHAPES))]

TRACE_VALS, TRACE_IN_SHAPES = split([([1.0, 2.0, 3.0, 4.0], [2, 2]), ([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]),
                                  (list(map(float, range(24))), [2, 3, 4])])
TRACE_OUT = [flat(numpy.trace(arr(v, s))) for v, s in zip(TRACE_VALS, TRACE_IN_SHAPES)]
TRACE_OUT_SHAPES = [list(numpy.asarray(numpy.trace(arr(v, s))).shape) for v, s in zip(TRACE_VALS, TRACE_IN_SHAPES)]

# --- error messages ------------------------------------------------------

def err(fn):
    try:
        fn()
        return "NO ERROR"
    except Exception as e:  # noqa: BLE001
        return str(e)


ERRORS = sorted(
    [
        ["BROADCAST", err(lambda: numpy.add(numpy.zeros((2, 3)), numpy.zeros((4,))))],
        ["BCAST_SHAPES", err(lambda: numpy.broadcast_shapes((2, 3), (4,)))],
        ["BCAST_SHAPES_3", err(lambda: numpy.broadcast_shapes((5, 1, 3), (2, 1), (4, 1)))],
        ["RESHAPE", err(lambda: numpy.zeros(6).reshape((2, 4)))],
        ["RESHAPE_NEG", err(lambda: numpy.zeros(6).reshape((-1, -1)))],
        ["RESHAPE_NEG0", err(lambda: numpy.zeros(6).reshape((-1, 4)))],
        ["RESHAPE_NEGX", err(lambda: numpy.zeros(6).reshape((2, -1, 5)))],
        ["AXIS", err(lambda: numpy.sum(numpy.zeros((2, 3)), axis=5))],
        ["AXIS_NEG", err(lambda: numpy.sum(numpy.zeros((2, 3)), axis=-3))],
        ["TRANSPOSE", err(lambda: numpy.zeros((2, 3)).transpose((0, 1, 2)))],
        ["TRANSPOSE_REPEAT", err(lambda: numpy.zeros((2, 3)).transpose((0, 0)))],
        ["TRANSPOSE_BAD", err(lambda: numpy.zeros((2, 3)).transpose((5, 1)))],
        ["MOVEAXIS_LEN", err(lambda: numpy.moveaxis(numpy.zeros((2, 3, 4)), (0,), (1, 2)))],
        ["MOVEAXIS_REPEAT", err(lambda: numpy.moveaxis(numpy.zeros((2, 3, 4)), (0, 1), (1, 1)))],
        ["CONCAT_DIM", err(lambda: numpy.concatenate((numpy.zeros((2, 3)), numpy.zeros((2, 3, 1))), axis=0))],
        ["CONCAT_SIZE", err(lambda: numpy.concatenate((numpy.zeros((2, 3)), numpy.zeros((2, 4))), axis=0))],
        ["CONCAT_EMPTY", err(lambda: numpy.concatenate((), axis=0))],
        ["STACK_SHAPE", err(lambda: numpy.stack((numpy.zeros((2, 3)), numpy.zeros((2, 3, 1))), axis=0))],
        ["STACK_EMPTY", err(lambda: numpy.stack((), axis=0))],
        ["TAKE", err(lambda: numpy.take(numpy.zeros(3), [5], axis=0))],
        ["SQUEEZE", err(lambda: numpy.squeeze(numpy.zeros((2, 3)), axis=0))],
        ["MATMUL", err(lambda: numpy.matmul(numpy.zeros((2, 3)), numpy.zeros((4, 5))))],
        ["MATMUL_1D", err(lambda: numpy.matmul(numpy.zeros(3), numpy.zeros(4)))],
        ["DOT", err(lambda: numpy.dot(numpy.zeros((2, 3)), numpy.zeros((4, 5))))],
        ["INNER", err(lambda: numpy.inner(numpy.zeros(3), numpy.zeros(4)))],
        ["VDOT", err(lambda: numpy.vdot(numpy.zeros(3), numpy.zeros(4)))],
        ["EYE", err(lambda: numpy.eye(-1))],
        ["LINSPACE", err(lambda: numpy.linspace(0, 1, -1))],
        ["BCAST_TO_DIMS", err(lambda: numpy.broadcast_to(numpy.zeros((2, 3)), (3,)))],
        ["BCAST_TO", err(lambda: numpy.broadcast_to(numpy.zeros((2, 1)), (3, 5)))],
        ["RAGGED2", err(lambda: numpy.array([[1, 2, 3], [4, 5]]))],
        ["RAGGED3", err(lambda: numpy.array([[[1, 2], [3, 4]], [[5], [6]]]))],
    ],
    key=lambda pair: pair[0],
)

if __name__ == "__main__":
    write("numpy", "every value and error message the @osprey/numpy suite compares against.",
          {"numpy": V}, {
              "ARANGE": ARANGE, "ARANGE_OUT": ARANGE_OUT,
              "LINSPACE_F": LINSPACE_F, "LINSPACE_NUM": LINSPACE_NUM, "LINSPACE_OUT": LINSPACE_OUT,
              "EYE": EYE, "EYE_OUT": EYE_OUT,
              "TRI": TRI, "TRI_OUT": TRI_OUT,
              "DIAG_VALS": DIAG_VALS, "DIAG_SHAPES": DIAG_SHAPES, "DIAG_OUT": DIAG_OUT,
              "ZEROS": ZEROS, "ZEROS_OUT": ZEROS_OUT, "ONES_OUT": ONES_OUT,
              "FULL_VALUES": FULL_VALUES, "FULL_OUT": FULL_OUT,
              "RESHAPE_VALS": RESHAPE_VALS, "RESHAPE_SHAPES": RESHAPE_SHAPES, "RESHAPE_NEW": RESHAPE_NEW,
              "RESHAPE_OUT": RESHAPE_OUT, "RESHAPE_OUT_SHAPES": RESHAPE_OUT_SHAPES,
              "TRANSPOSE_VALS": TRANSPOSE_VALS, "TRANSPOSE_SHAPES": TRANSPOSE_SHAPES, "TRANSPOSE_AXES": TRANSPOSE_AXES,
              "TRANSPOSE_OUT": TRANSPOSE_OUT, "TRANSPOSE_OUT_SHAPES": TRANSPOSE_OUT_SHAPES,
              "SWAPAXES_VALS": SWAPAXES_VALS, "SWAPAXES_SHAPES": SWAPAXES_SHAPES,
              "SWAPAXES_A1": SWAPAXES_A1, "SWAPAXES_A2": SWAPAXES_A2, "SWAPAXES_OUT": SWAPAXES_OUT,
              "MOVEAXIS_VALS": MOVEAXIS_VALS, "MOVEAXIS_SHAPES": MOVEAXIS_SHAPES,
              "MOVEAXIS_SRC": MOVEAXIS_SRC, "MOVEAXIS_DST": MOVEAXIS_DST, "MOVEAXIS_OUT": MOVEAXIS_OUT, "MOVEAXIS_OUT_SHAPES": MOVEAXIS_OUT_SHAPES,
              "ED_VALS": ED_VALS, "ED_SHAPES": ED_SHAPES, "ED_AXIS": ED_AXIS, "ED_OUT": ED_OUT, "ED_OUT_SHAPES": ED_OUT_SHAPES,
              "SQ_VALS": SQ_VALS, "SQ_SHAPES": SQ_SHAPES, "SQ_OUT": SQ_OUT, "SQ_OUT_SHAPES": SQ_OUT_SHAPES,
              "BS_SHAPES": BS_SHAPES, "BS_OUT": BS_OUT,
              "BT_VALS": BT_VALS, "BT_SHAPES": BT_SHAPES, "BT_NEW": BT_NEW, "BT_OUT": BT_OUT, "BT_OUT_SHAPES": BT_OUT_SHAPES,
              "CONCAT_VALS": CONCAT_VALS, "CONCAT_SHAPES": CONCAT_SHAPES, "CONCAT_AXIS": CONCAT_AXIS,
              "CONCAT_OUT": CONCAT_OUT, "CONCAT_OUT_SHAPES": CONCAT_OUT_SHAPES,
              "STACK_VALS": STACK_VALS, "STACK_SHAPES": STACK_SHAPES, "STACK_AXIS": STACK_AXIS,
              "STACK_OUT": STACK_OUT, "STACK_OUT_SHAPES": STACK_OUT_SHAPES,
              "TAKE_VALS": TAKE_VALS, "TAKE_SHAPES": TAKE_SHAPES, "TAKE_IDX": TAKE_IDX, "TAKE_AXIS": TAKE_AXIS,
              "TAKE_OUT": TAKE_OUT, "TAKE_OUT_SHAPES": TAKE_OUT_SHAPES,
              "FLIP_VALS": FLIP_VALS, "FLIP_SHAPES": FLIP_SHAPES, "FLIP_AXIS": FLIP_AXIS, "FLIP_OUT": FLIP_OUT,
              "ROLL_VALS": ROLL_VALS, "ROLL_SHAPES": ROLL_SHAPES, "ROLL_SHIFT": ROLL_SHIFT, "ROLL_AXIS": ROLL_AXIS, "ROLL_OUT": ROLL_OUT,
              "RP_VALS": RP_VALS, "RP_SHAPES": RP_SHAPES, "RP_REPS": RP_REPS, "RP_AXIS": RP_AXIS, "RP_OUT": RP_OUT, "RP_OUT_SHAPES": RP_OUT_SHAPES,
              "TILE_VALS": TILE_VALS, "TILE_SHAPES": TILE_SHAPES, "TILE_REPS": TILE_REPS, "TILE_OUT": TILE_OUT, "TILE_OUT_SHAPES": TILE_OUT_SHAPES,
              "BIN_A_VALS": BIN_A_VALS, "BIN_A_SHAPES": BIN_A_SHAPES, "BIN_B_VALS": BIN_B_VALS, "BIN_B_SHAPES": BIN_B_SHAPES,
              "BIN_ADD": BIN_ADD, "BIN_SUB": BIN_SUB, "BIN_MUL": BIN_MUL, "BIN_DIV": BIN_DIV,
              "BIN_MAX": BIN_MAX, "BIN_MIN": BIN_MIN, "BIN_EQ": BIN_EQ, "BIN_NE": BIN_NE,
              "BIN_LT": BIN_LT, "BIN_LE": BIN_LE, "BIN_GT": BIN_GT, "BIN_GE": BIN_GE,
              "WHERE_C_VALS": WHERE_C_VALS, "WHERE_C_SHAPES": WHERE_C_SHAPES,
              "WHERE_A_VALS": WHERE_A_VALS, "WHERE_A_SHAPES": WHERE_A_SHAPES,
              "WHERE_B_VALS": WHERE_B_VALS, "WHERE_B_SHAPES": WHERE_B_SHAPES, "WHERE_OUT": WHERE_OUT,
              "UNARY_VALS": UNARY_VALS, "UNARY_SHAPES": UNARY_SHAPES,
              "UN_NEG": UN_NEG, "UN_ABS": UN_ABS, "UN_EXP": UN_EXP, "UN_LOG": UN_LOG,
              "UN_SQRT": UN_SQRT, "UN_TANH": UN_TANH, "UN_SQR": UN_SQR, "UN_RECIP": UN_RECIP, "UN_SIGN": UN_SIGN,
              "CLIP": CLIP, "CLIP_OUT": CLIP_OUT,
              "NONFIN_VALS": NONFIN_VALS, "NONFIN_SHAPES": NONFIN_SHAPES,
              "LOG_ZERO": LOG_ZERO, "SQRT_NEG": SQRT_NEG, "RECIP_ZERO": RECIP_ZERO,
              "NAN_A_VALS": NAN_A_VALS, "NAN_A_SHAPES": NAN_A_SHAPES, "NAN_B_VALS": NAN_B_VALS, "NAN_B_SHAPES": NAN_B_SHAPES,
              "NAN_MAX": NAN_MAX, "NAN_MIN": NAN_MIN, "NAN_EQ": NAN_EQ,
              "RED_VALS": RED_VALS, "RED_SHAPES": RED_SHAPES,
              "SUM_ALL": SUM_ALL, "SUM_AX0": SUM_AX0, "SUM_AXM1": SUM_AXM1, "SUM_AXM1K": SUM_AXM1K,
              "MEAN_ALL": MEAN_ALL, "MEAN_AX0": MEAN_AX0, "MEAN_AXM1K": MEAN_AXM1K,
              "MAX_ALL": MAX_ALL, "MAX_AX0": MAX_AX0, "MIN_ALL": MIN_ALL, "MIN_AXM1": MIN_AXM1,
              "PROD_ALL": PROD_ALL, "PROD_AX0": PROD_AX0, "VAR_ALL0": VAR_ALL0, "VAR_ALL1": VAR_ALL1,
              "VAR_AX0": VAR_AX0, "STD_ALL0": STD_ALL0, "STD_AXM1": STD_AXM1,
              "ARGMAX_ALL": ARGMAX_ALL, "ARGMIN_ALL": ARGMIN_ALL, "ARGMAX_AX0": ARGMAX_AX0, "ARGMIN_AXM1": ARGMIN_AXM1,
              "ALL_ALL": ALL_ALL, "ANY_ALL": ANY_ALL, "ALL_AX0": ALL_AX0, "ANY_AXM1": ANY_AXM1,
              "CUMSUM_AXM1": CUMSUM_AXM1, "CUMPROD_AX0": CUMPROD_AX0,
              "MM_A_VALS": MM_A_VALS, "MM_A_SHAPES": MM_A_SHAPES, "MM_B_VALS": MM_B_VALS, "MM_B_SHAPES": MM_B_SHAPES,
              "MATMUL_OUT": MATMUL_OUT, "MATMUL_SHAPES": MATMUL_SHAPES,
              "DOT_A_VALS": DOT_A_VALS, "DOT_A_SHAPES": DOT_A_SHAPES, "DOT_B_VALS": DOT_B_VALS, "DOT_B_SHAPES": DOT_B_SHAPES,
              "DOT_OUT": DOT_OUT, "DOT_SHAPES": DOT_SHAPES,
              "OUTER_A_VALS": OUTER_A_VALS, "OUTER_A_SHAPES": OUTER_A_SHAPES, "OUTER_B_VALS": OUTER_B_VALS, "OUTER_B_SHAPES": OUTER_B_SHAPES,
              "OUTER_OUT": OUTER_OUT,
              "INNER_A_VALS": INNER_A_VALS, "INNER_A_SHAPES": INNER_A_SHAPES, "INNER_B_VALS": INNER_B_VALS, "INNER_B_SHAPES": INNER_B_SHAPES,
              "INNER_OUT": INNER_OUT,
              "VDOT_A_VALS": VDOT_A_VALS, "VDOT_A_SHAPES": VDOT_A_SHAPES, "VDOT_B_VALS": VDOT_B_VALS, "VDOT_B_SHAPES": VDOT_B_SHAPES,
              "VDOT_OUT": VDOT_OUT,
              "TRACE_VALS": TRACE_VALS, "TRACE_IN_SHAPES": TRACE_IN_SHAPES, "TRACE_OUT": TRACE_OUT, "TRACE_OUT_SHAPES": TRACE_OUT_SHAPES,
              "ERRORS": ERRORS,
          })
