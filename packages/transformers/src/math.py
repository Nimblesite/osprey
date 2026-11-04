# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Math
"""Stable softmax and log-softmax, greedy argmax and inverse-CDF multinomial sampling.

These are the shared numerical building blocks every logits warper and sampler
in `transformers.LogitsProcess` is built from. softmax and log_softmax subtract
the row maximum before exponentiating, exactly as torch's `softmax`/`log_softmax`
do, so an all-`-inf` row yields NaN (exp(-inf + inf)) just as torch does.
`argmax` returns the first index holding the maximum, matching `torch.argmax`
on ties. `multinomial` is torch's inverse-CDF rule: the first index whose
cumulative probability strictly exceeds a uniform draw `u` in `[0, 1)`; torch's
own RNG stream is not reproduced, so callers pass an explicit draw. Values from
`+ - * /` are correctly rounded on every host; values downstream of exp/ln carry
the usual libm last-digit spread. Implements [PORT-TEST-ORACLE].
"""

__all__ = ["softmax", "log_softmax", "argmax", "multinomial", "neg_inf", "max_f", "max_i", "min_f", "min_i"]


def neg_inf():
    """Negative infinity, the value every warper's filter lands on."""
    return toFloat32(0.0) - exp(toFloat32(1000.0))


def list_max(xs):
    """The largest element of a float list; -inf for an empty list."""
    return fold(range(len(xs)), neg_inf(), lambda a, i: max_f(a, unwrap_or(listGet(xs, i), neg_inf())))


def max_f(a, x):
    """The larger of two floats."""
    return x if x > a else a


def min_f(a, b):
    """The smaller of two floats."""
    return a if a < b else b


def max_i(a, b):
    """The larger of two ints."""
    return a if a > b else b


def min_i(a, b):
    """The smaller of two ints."""
    return a if a < b else b


def softmax(logits):
    """torch.softmax(logits, dim=-1): exp(x - max) / sum(exp(x - max))."""
    if len(logits) == 0:
        return logits
    else:
        m = list_max(logits)
        exps = mapList(logits, lambda x: exp(x - m))
        inverse = toFloat32(1.0) / sum_exps(exps)
        return mapList(exps, lambda e: e * inverse)


def sum_lane(xs, lane):
    """One binary32 lane of the pinned torch CPU oracle's four-lane reduction."""
    return fold(range(len(xs)), toFloat32(0.0), lambda total, i: total + unwrap_or(listGet(xs, i), toFloat32(0.0)) if i % 4 == lane else total)


def sum_exps(xs):
    """[PORT-TEST-ORACLE]: preserve the pinned binary32 reduction order."""
    if len(xs) < 4:
        return foldList(xs, toFloat32(0.0), lambda a, x: a + x)
    else:
        return (sum_lane(xs, 0) + sum_lane(xs, 2)) + (sum_lane(xs, 1) + sum_lane(xs, 3))


def log_softmax(logits):
    """torch.log_softmax(logits, dim=-1) with max subtraction: `(x - max) -
    ln(sum(exp(x - max)))`, so a large max never cancels the log term against x."""
    m = list_max(logits)
    exps = mapList(logits, lambda x: exp(x - m))
    s = sum_exps(exps)
    lse = ln(s)
    return mapList(logits, lambda x: (x - m) - lse)


def argmax(scores):
    """The first index holding the largest score, like torch.argmax on ties."""
    return fold(range(len(scores)), 0, lambda best, i: i if unwrap_or(listGet(scores, i), neg_inf()) > unwrap_or(listGet(scores, best), neg_inf()) else best)


def multinomial(probs, u):
    """torch.multinomial's inverse CDF: the first index whose cumulative
    probability strictly exceeds the uniform draw `u` in `[0, 1)`; the last
    index when none does."""
    return first_above(probs, u, 0, toFloat32(0.0))


def first_above(probs, u, i, running):
    if i == len(probs):
        return len(probs) - 1
    else:
        acc = running + unwrap_or(listGet(probs, i), toFloat32(0.0))
        return i if acc > u else first_above(probs, u, i + 1, acc)
