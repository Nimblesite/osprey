# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=scipy
# osprey: module=Beta
"""scipy.special's beta family, from the Cephes beta/lbeta kernels.

beta computes rgamma(a+b) * gamma(a) * gamma(b) with the Cephes ordering that
keeps the roundoff smallest, the lgam path for arguments beyond MAXGAM, and the
asymptotic lbeta series when |a| >> |b|. betaln is lgam(a) + lgam(b) - lgam(a+b).
Negative-integer poles reproduce scipy: beta(-2, b) with integer b is +inf
unless the reflection cancels it. Values match scipy to ~1e-13 relative.
Implements [PORT-TEST-ORACLE].
"""
from scipy.Support import (MAXGAM, MAXLOG, BETA_ASYMP_FACTOR, pos_inf, is_inf,
                           exp_s, log_s, floor, even_f, Signed)
from scipy.Gamma import gamma, rgamma, lgam_sgn

__all__ = ["beta", "betaln", "lbeta"]

INT_OK_MAX = 9.223372036854775e18


def int_ok(v: float) -> bool:
    """Whether v is an integer whose float form the Cephes kernels accept."""
    return v == floor(v) and abs(v) <= INT_OK_MAX


def beta_negint(a: float, b: float) -> float:
    """beta(a, b) with a a negative integer: the Cephes reflection rule."""
    if b == floor(b) and abs(b) < INT_OK_MAX and 1.0 - a - b > 0.0:
        sgn = 1.0 if even_f(b) else -1.0
        return sgn * beta(1.0 - a - b, b)
    else:
        return pos_inf()


def lbeta_negint(a: float, b: float) -> float:
    """lbeta(a, b) with a a negative integer: the Cephes reflection rule."""
    if b == floor(b) and abs(b) < INT_OK_MAX and 1.0 - a - b > 0.0:
        return lbeta(1.0 - a - b, b)
    else:
        return pos_inf()


def lbeta_asymp(a: float, b: float) -> Signed:
    """The Cephes asymptotic lbeta(a, b) for |a| >> |b|."""
    r = lgam_sgn(b)
    v0 = r.value - b * log_s(a)
    v1 = v0 + b * (1.0 - b) / (2.0 * a)
    v2 = v1 + b * (1.0 - b) * (1.0 - 2.0 * b) / (12.0 * a * a)
    v = v2 + (0.0 - b * b) * (1.0 - b) * (1.0 - b) / (12.0 * a * a * a)
    return Signed(value=v, sign=r.sign)


def beta_lgam(a: float, b: float, y: float) -> float:
    """beta's lgam path: exp(lgam(b) - lgam(y) + lgam(a)) with signs."""
    yv = lgam_sgn(y)
    v0 = yv.value
    sign0 = yv.sign
    y2 = lgam_sgn(b)
    v1 = y2.value - v0
    sign1 = sign0 * y2.sign
    y3 = lgam_sgn(a)
    v2 = y3.value + v1
    sign2 = sign1 * y3.sign
    if v2 > MAXLOG:
        return pos_inf()
    else:
        return sign2 * exp_s(v2)


def beta_core(a: float, b: float) -> float:
    """beta after the negative-integer pole checks."""
    if abs(a) < abs(b):
        return beta_core(b, a)
    elif abs(a) > BETA_ASYMP_FACTOR * abs(b) and a > BETA_ASYMP_FACTOR:
        s = lbeta_asymp(a, b)
        return s.sign * exp_s(s.value)
    else:
        y = a + b
        if abs(y) > MAXGAM or abs(a) > MAXGAM or abs(b) > MAXGAM:
            return beta_lgam(a, b, y)
        else:
            return beta_rgamma(a, b, y)


def beta_rgamma(a: float, b: float, y: float) -> float:
    """beta's rgamma path: rgamma(a+b) * gamma(a) * gamma(b), Cephes order."""
    y2 = rgamma(y)
    a2 = gamma(a)
    b2 = gamma(b)
    if is_inf(y2):
        return pos_inf()
    elif abs(abs(a2 * y2) - 1.0) > abs(abs(b2 * y2) - 1.0):
        return (b2 * y2) * a2
    else:
        return (a2 * y2) * b2


def beta(a: float, b: float) -> float:
    """scipy.special.beta(a, b): the Cephes beta."""
    if a <= 0.0 and a == floor(a):
        if int_ok(a):
            return beta_negint(a, b)
        else:
            return pos_inf()
    elif b <= 0.0 and b == floor(b):
        if int_ok(b):
            return beta_negint(b, a)
        else:
            return pos_inf()
    else:
        return beta_core(a, b)


def lbeta_lgam(a: float, b: float, y: float) -> float:
    """lbeta's lgam path: lgam(b) - lgam(y) + lgam(a)."""
    yv = lgam_sgn(y)
    v0 = yv.value
    y2 = lgam_sgn(b)
    v1 = y2.value - v0
    y3 = lgam_sgn(a)
    return y3.value + v1


def lbeta_core(a: float, b: float) -> float:
    """lbeta after the negative-integer pole checks."""
    if abs(a) < abs(b):
        return lbeta_core(b, a)
    elif abs(a) > BETA_ASYMP_FACTOR * abs(b) and a > BETA_ASYMP_FACTOR:
        return lbeta_asymp(a, b).value
    else:
        y = a + b
        if abs(y) > MAXGAM or abs(a) > MAXGAM or abs(b) > MAXGAM:
            return lbeta_lgam(a, b, y)
        else:
            y2 = rgamma(y)
            a2 = gamma(a)
            b2 = gamma(b)
            if is_inf(y2):
                return pos_inf()
            else:
                y3 = (b2 * y2) * a2 if abs(abs(a2 * y2) - 1.0) > abs(abs(b2 * y2) - 1.0) else (a2 * y2) * b2
                y4 = (0.0 - y3) if y3 < 0.0 else y3
                return log_s(y4)


def lbeta(a: float, b: float) -> float:
    """scipy.special.betaln(a, b): log(|beta(a, b)|), the Cephes lbeta."""
    if a <= 0.0 and a == floor(a):
        if int_ok(a):
            return lbeta_negint(a, b)
        else:
            return pos_inf()
    elif b <= 0.0 and b == floor(b):
        if int_ok(b):
            return lbeta_negint(b, a)
        else:
            return pos_inf()
    else:
        return lbeta_core(a, b)


def betaln(a: float, b: float) -> float:
    """scipy.special.betaln: lbeta."""
    return lbeta(a, b)
