# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Information
"""scipy.special's information-theoretic functions and list reductions.

xlogy/xlog1py/entr/rel_entr/kl_div/huber/pseudo_huber follow the xsf
convex_analysis.h formulas (kl_div is x*ln(x/y)-x+y, rel_entr uses the log1p
form near ratio 1). logsumexp/softmax/log_softmax follow scipy 1.18's
Python algorithms: subtract the maximum, then the out_inf fallback for
non-finite rows. Edge cases reproduce scipy: entr(0)=0, entr(<0)=-inf,
kl_div(0,y)=y, rel_entr(0,y)=0, huber/pseudo_huber at negative delta are inf.
Implements [PORT-TEST-ORACLE].
"""
from scipy.Support import (nan, pos_inf, neg_inf, is_nan, is_inf, exp_s, log_s,
                           log1p, expm1, sign_f, list_max)

__all__ = ["xlogy", "xlog1py", "entr", "rel_entr", "kl_div", "huber",
           "pseudo_huber", "logsumexp", "logsumexp_b", "softmax", "log_softmax"]


def xlogy(x: float, y: float) -> float:
    """scipy.special.xlogy(x, y): x*ln(y), 0 at x=0."""
    if x == 0.0 and (not is_nan(y)):
        return 0.0
    else:
        return x * log_s(y)


def xlog1py(x: float, y: float) -> float:
    """scipy.special.xlog1py(x, y): x*ln(1+y), 0 at x=0."""
    if x == 0.0 and (not is_nan(y)):
        return 0.0
    else:
        return x * log1p(y)


def entr(x: float) -> float:
    """scipy.special.entr(x): -x*ln(x), 0 at 0, -inf below zero."""
    if is_nan(x):
        return x
    elif x > 0.0:
        return (0.0 - x) * log_s(x)
    elif x == 0.0:
        return 0.0
    else:
        return neg_inf()


def kl_div(x: float, y: float) -> float:
    """scipy.special.kl_div(x, y): x*ln(x/y) - x + y."""
    if is_nan(x) or is_nan(y):
        return nan()
    elif x > 0.0 and y > 0.0:
        return x * log_s(x / y) - x + y
    elif x == 0.0 and y >= 0.0:
        return y
    else:
        return pos_inf()


def rel_entr(x: float, y: float) -> float:
    """scipy.special.rel_entr(x, y): x*ln(x/y) with the xsf near-ratio-1 form."""
    if is_nan(x) or is_nan(y):
        return nan()
    elif x <= 0.0 or y <= 0.0:
        if x == 0.0 and y >= 0.0:
            return 0.0
        else:
            return pos_inf()
    else:
        ratio = x / y
        if 0.5 < ratio and ratio < 2.0:
            return x * log1p((x - y) / y)
        elif 2.2250738585072014e-308 < ratio and (not is_inf(ratio)):
            return x * log_s(ratio)
        else:
            return x * (log_s(x) - log_s(y))


def huber(delta: float, r: float) -> float:
    """scipy.special.huber(delta, r): the Huber loss."""
    if delta < 0.0:
        return pos_inf()
    elif abs(r) <= delta:
        return 0.5 * r * r
    else:
        return delta * (abs(r) - 0.5 * delta)


def pseudo_huber(delta: float, r: float) -> float:
    """scipy.special.pseudo_huber(delta, r): the pseudo-Huber loss."""
    if delta < 0.0:
        return pos_inf()
    elif delta == 0.0 or r == 0.0:
        return 0.0
    else:
        v = r / delta
        return delta * delta * expm1(0.5 * log1p(v * v))


def ones_like(a) -> list[float]:
    """A list of 1.0 the length of `a`, the unweighted logsumexp weights."""
    return mapList(a, lambda v: 1.0)


def out_inf_from(s: float) -> float:
    """scipy logsumexp's out_inf fallback from the plain sum of exponentials."""
    if is_nan(s):
        return nan()
    elif s > 0.0:
        return log_s(s)
    elif s < 0.0:
        return nan()
    else:
        return neg_inf()


def lse_main(a2, weights, a_max: float) -> float:
    """logsumexp's max-separated core over the pre-filtered row."""
    n = len(a2)
    mask = mapList(a2, lambda v: v == a_max)
    exps = toList(map(range(n), lambda i: unwrap_or(listGet(weights, i), 1.0) *
                      (0.0 if unwrap_or(listGet(mask, i), False)
                       else exp_s(unwrap_or(listGet(a2, i), 0.0) - a_max))))
    s0 = foldList(exps, 0.0, lambda acc, e: acc + e)
    m0 = fold(range(n), 0.0, lambda acc, i: acc +
             (unwrap_or(listGet(weights, i), 1.0) if unwrap_or(listGet(mask, i), False) else 0.0))
    s1 = s0 / m0 if (s0 != 0.0 and m0 != 0.0) else (nan() if s0 != 0.0 else s0)
    sgn = sign_f(s1 + 1.0) * sign_f(m0)
    s2 = (0.0 - s1 - 2.0) if s1 < -1.0 else s1
    m1 = abs(m0)
    out = nan() if (m1 == 0.0 or is_nan(s2)) else (log1p(s2) + log_s(m1) + a_max)
    return nan() if sgn < 0.0 else out


def logsumexp(a) -> float:
    """scipy.special.logsumexp(a): ln(sum(exp(a))) with max subtraction."""
    s0 = foldList(a, 0.0, lambda acc, v: acc + exp_s(v))
    out = lse_main(a, ones_like(a), list_max(a))
    if is_nan(out) or is_inf(out):
        return out_inf_from(s0)
    else:
        return out


def logsumexp_b(a, b) -> float:
    """scipy.special.logsumexp(a, b=b): ln(sum(b*exp(a))) with max subtraction."""
    n = len(a)
    s0 = fold(range(n), 0.0, lambda acc, i: acc +
              unwrap_or(listGet(b, i), 0.0) * exp_s(unwrap_or(listGet(a, i), 0.0)))
    a2 = toList(map(range(n), lambda i: neg_inf() if unwrap_or(listGet(b, i), 0.0) == 0.0
                    else unwrap_or(listGet(a, i), 0.0)))
    out = lse_main(a2, b, list_max(a2))
    if is_nan(out) or is_inf(out):
        return out_inf_from(s0)
    else:
        return out


def softmax(xs) -> list[float]:
    """scipy.special.softmax(x): exp(x - max) / sum(exp(x - max))."""
    x_max = list_max(xs)
    exps = mapList(xs, lambda v: exp_s(v - x_max))
    s = foldList(exps, 0.0, lambda acc, e: acc + e)
    return mapList(exps, lambda e: e / s)


def log_softmax(xs) -> list[float]:
    """scipy.special.log_softmax(x): x - max - ln(sum(exp(x - max)))."""
    x_max0 = list_max(xs)
    x_max = 0.0 if (is_nan(x_max0) or is_inf(x_max0)) else x_max0
    tmp = mapList(xs, lambda v: v - x_max)
    s = foldList(tmp, 0.0, lambda acc, t: acc + exp_s(t))
    out = log_s(s)
    return mapList(tmp, lambda t: t - out)
