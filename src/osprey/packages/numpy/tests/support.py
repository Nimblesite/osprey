# osprey: flavor=python
# osprey: namespace=numpy_tests
# osprey: module=Support
from numpy import Array, Errors
from numpy_expected import Expected
from testing import Check

__all__ = ['TIGHT', 'LOOSE', 'ATOL', 'atl', 'atf', 'ati', 'ats', 'atb', 'f', 'it', 'arr', 'finish_list', 'finish_float', 'finish_int', 'finish_ints', 'finish_bool', 'red_arr']

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


def red_arr(i):
    return arr(atl(Expected.RED_VALS, i), atl(Expected.RED_SHAPES, i))


