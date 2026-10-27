# osprey: flavor=python
# osprey: namespace=numpy_tests
# osprey: module=ExtraCases
"""Exercise the reduction and product oracle tables beyond the flat reductions."""
from numpy import Array, Linalg
from numpy_expected import Expected
from numpy_tests import Support as S
from testing import Check

__all__ = ["run"]


def axis_first(tag, a):
    match tag:
        case 0:
            return Array.sum_axis(a, 0, False)
        case 1:
            return Array.sum_axis(a, -1, False)
        case 2:
            return Array.sum_axis(a, -1, True)
        case 3:
            return Array.mean_axis(a, 0, False)
        case 4:
            return Array.mean_axis(a, -1, True)
        case 5:
            return Array.max_axis(a, 0, False)
        case _:
            return Array.min_axis(a, -1, False)


def axis_last(tag, a):
    match tag:
        case 7:
            return Array.prod_axis(a, 0, False)
        case 8:
            return Array.var_axis(a, 0, False, 0)
        case 9:
            return Array.std_axis(a, -1, False, 1)
        case 10:
            return Array.all_axis(a, 0, False)
        case 11:
            return Array.any_axis(a, -1, False)
        case 12:
            return Array.cumsum(a, -1)
        case _:
            return Array.cumprod(a, 0)


def axis_operation(tag, a):
    return axis_first(tag, a) if tag < 7 else axis_last(tag, a)


def axis_expected(tag):
    std = mapList(Expected.STD_AXM1, lambda xs: mapList(xs, Check.float_of))
    groups = [Expected.SUM_AX0, Expected.SUM_AXM1, Expected.SUM_AXM1K, Expected.MEAN_AX0, Expected.MEAN_AXM1K, Expected.MAX_AX0, Expected.MIN_AXM1, Expected.PROD_AX0, Expected.VAR_AX0, std, Expected.ALL_AX0, Expected.ANY_AXM1, Expected.CUMSUM_AXM1, Expected.CUMPROD_AX0]
    return S.atl(groups, tag)


def axis_case(backend, tag, i):
    return S.finish_list(f"axis.{tag}.{i}.{backend}", lambda: Array.tolist(axis_operation(tag, S.red_arr(i))), S.atl(axis_expected(tag), i), S.LOOSE)


def scalar_reductions(backend, i):
    return [
        S.finish_int(f"argmax.{i}.{backend}", lambda: Array.argmax(S.red_arr(i)), S.ati(Expected.ARGMAX_ALL, i)),
        S.finish_int(f"argmin.{i}.{backend}", lambda: Array.argmin(S.red_arr(i)), S.ati(Expected.ARGMIN_ALL, i)),
        S.finish_bool(f"all.{i}.{backend}", lambda: Array.all(S.red_arr(i)), S.atb(Expected.ALL_ALL, i)),
        S.finish_bool(f"any.{i}.{backend}", lambda: Array.any(S.red_arr(i)), S.atb(Expected.ANY_ALL, i)),
        S.finish_ints(f"argmax_axis.{i}.{backend}", lambda: Array.argmax_axis(S.red_arr(i), 0), S.atl(Expected.ARGMAX_AX0, i)),
        S.finish_ints(f"argmin_axis.{i}.{backend}", lambda: Array.argmin_axis(S.red_arr(i), -1), S.atl(Expected.ARGMIN_AXM1, i)),
        S.finish_float(f"var.ddof1.{i}.{backend}", lambda: Array.var(S.red_arr(i), 1), Check.float_of(S.ats(Expected.VAR_ALL1, i)), S.LOOSE),
    ]


def reductions(backend):
    axes = fold(range(14), [], lambda acc, tag: listConcat(acc, toList(map(range(len(Expected.RED_VALS)), lambda i: axis_case(backend, tag, i)))))
    scalars = fold(range(len(Expected.RED_VALS)), [], lambda acc, i: listConcat(acc, scalar_reductions(backend, i)))
    return listConcat(axes, scalars)


def product(tag, a, b):
    match tag:
        case 0:
            return Linalg.matmul(a, b)
        case 1:
            return Linalg.dot(a, b)
        case 2:
            return Linalg.outer(a, b)
        case 3:
            return Linalg.inner(a, b)
        case _:
            return Linalg.vdot(a, b)


def product_a(tag, i):
    vals = S.atl([Expected.MM_A_VALS, Expected.DOT_A_VALS, Expected.OUTER_A_VALS, Expected.INNER_A_VALS, Expected.VDOT_A_VALS], tag)
    shapes = S.atl([Expected.MM_A_SHAPES, Expected.DOT_A_SHAPES, Expected.OUTER_A_SHAPES, Expected.INNER_A_SHAPES, Expected.VDOT_A_SHAPES], tag)
    return S.arr(S.atl(vals, i), S.atl(shapes, i))


def product_b(tag, i):
    vals = S.atl([Expected.MM_B_VALS, Expected.DOT_B_VALS, Expected.OUTER_B_VALS, Expected.INNER_B_VALS, Expected.VDOT_B_VALS], tag)
    shapes = S.atl([Expected.MM_B_SHAPES, Expected.DOT_B_SHAPES, Expected.OUTER_B_SHAPES, Expected.INNER_B_SHAPES, Expected.VDOT_B_SHAPES], tag)
    return S.arr(S.atl(vals, i), S.atl(shapes, i))


def product_expected(tag):
    return S.atl([Expected.MATMUL_OUT, Expected.DOT_OUT, Expected.OUTER_OUT, Expected.INNER_OUT, Expected.VDOT_OUT], tag)


def product_case(backend, tag, i):
    return S.finish_list(f"product.{tag}.{i}.{backend}", lambda: Array.tolist(product(tag, product_a(tag, i), product_b(tag, i))), S.atl(product_expected(tag), i), S.TIGHT)


def trace_case(backend, i):
    return S.finish_list(f"trace.{i}.{backend}", lambda: Array.tolist(Linalg.trace(S.arr(S.atl(Expected.TRACE_VALS, i), S.atl(Expected.TRACE_IN_SHAPES, i)))), S.atl(Expected.TRACE_OUT, i), S.TIGHT)


def products(backend):
    pairs = fold(range(5), [], lambda acc, tag: listConcat(acc, toList(map(range(len(product_expected(tag))), lambda i: product_case(backend, tag, i)))))
    traces = toList(map(range(len(Expected.TRACE_OUT)), lambda i: trace_case(backend, i)))
    return listConcat(pairs, traces)


def run(backend):
    return listConcat(reductions(backend), products(backend))
