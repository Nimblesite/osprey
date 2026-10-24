# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Combinatorics
"""scipy.special's combinatorial functions: factorial, comb, perm, binom, poch.

The float forms reproduce scipy's gamma/lbeta-based algorithms exactly;
`*_exact` return Osprey's 64-bit int (a divergence from Python's arbitrary
precision: only results that fit 2^63-1 are supported). comb/perm are zero
outside 0 <= k <= n, and binom carries scipy's poles: binom at a negative
integer n is NaN, and huge-k paths return infinities. Implements
[PORT-TEST-ORACLE].
"""
from scipy.Support import (PI, pos_inf, neg_inf, nan, is_nan, is_inf, is_neg_zero,
                           exp_s, log_s, floor, pow_pos, even_f, sinpi)
from scipy.Gamma import gamma, lgam_sgn
from scipy.Beta import beta, lbeta

__all__ = ["factorial", "factorial_exact", "comb", "comb_exact", "perm",
           "perm_exact", "binom", "poch"]


def factorial(x: float) -> float:
    """scipy.special.factorial(n, exact=False): gamma(n+1), zero for n < 0."""
    if is_nan(x):
        return x
    elif x < 0.0:
        return 0.0
    else:
        return gamma(x + 1.0)


def factorial_exact(n: int) -> int:
    """scipy.special.factorial(n, exact=True) as a 64-bit int (n <= 20)."""
    return fold(range(n), 1, lambda acc, i: acc * (i + 1))


def comb_exact_step(n: int, k: int, i: int, acc: int) -> int:
    if i > k:
        return acc
    else:
        return comb_exact_step(n, k, i + 1, acc * (n - i + 1) // i)


def comb_exact(n: int, k: int) -> int:
    """scipy.special.comb(n, k, exact=True) as a 64-bit int."""
    return comb_exact_step(n, k, 1, 1)


def perm_exact(n: int, k: int) -> int:
    """scipy.special.perm(n, k, exact=True) as a 64-bit int."""
    return fold(range(k), 1, lambda acc, i: acc * (n - i))


def binom_prod(n: float, kx: float, i: float, num: float, den: float) -> float:
    """binom's integer-k product loop, with the Cephes rescaling guard."""
    if i > kx:
        return num / den
    else:
        num2 = num * (i + n - kx)
        den2 = den * i
        if abs(num2) > 1e50:
            return binom_prod(n, kx, i + 1.0, num2 / den2, 1.0)
        else:
            return binom_prod(n, kx, i + 1.0, num2, den2)


def binom_intk(n: float, k: float, kx: float) -> float:
    """binom's integer-k branch: the product, or fall through."""
    nx = floor(n)
    kxx = nx - kx if (nx == n and kx > nx / 2.0 and nx > 0.0) else kx
    if kxx >= 0.0 and kxx < 20.0:
        return binom_prod(n, kxx, 1.0, 1.0, 1.0)
    else:
        return binom_large(n, k)


def sign_inf(v: float) -> float:
    """The sign of a huge-k binom numerator as an infinity or NaN."""
    return pos_inf() if v > 0.0 else (neg_inf() if v < 0.0 else nan())


def binom_hugek(n: float, k: float) -> float:
    """binom's huge-k asymptotic path."""
    num = gamma(1.0 + n) / abs(k) + gamma(1.0 + n) * n / (2.0 * k * k)
    d = PI * pow_pos(abs(k), n)
    if k > 0.0:
        return binom_hugek_pos(n, k, num, d)
    else:
        return binom_hugek_neg(n, k, num, d)


def binom_hugek_pos(n: float, k: float, num: float, d: float) -> float:
    """binom's huge-k path for k > 0."""
    kx = floor(k)
    intk = kx == k
    dk = 0.0 if intk else k
    sgn = (1.0 if even_f(kx) else -1.0) if intk else 1.0
    if d == 0.0:
        return sign_inf(num * sinpi(dk - n) * sgn)
    else:
        return (num / d) * sinpi(dk - n) * sgn


def binom_hugek_neg(n: float, k: float, num: float, d: float) -> float:
    """binom's huge-k path for k < 0."""
    kx = floor(k)
    if kx == k:
        return 0.0
    elif d == 0.0:
        return sign_inf(num * sinpi(k))
    else:
        return (num / d) * sinpi(k)


def binom_large(n: float, k: float) -> float:
    """binom's non-integer-k paths: lbeta, huge-k, or 1/((n+1) beta)."""
    if n >= 1.0e10 * k and k > 0.0:
        return exp_s(0.0 - lbeta(1.0 + n - k, 1.0 + k) - log_s(n + 1.0))
    elif k > 1.0e8 * abs(n):
        return binom_hugek(n, k)
    else:
        t = 1.0 / (n + 1.0)
        bv = beta(1.0 + n - k, 1.0 + k)
        if bv == 0.0:
            return pos_inf() if t > 0.0 else neg_inf()
        else:
            return t / bv


def binom(n: float, k: float) -> float:
    """scipy.special.binom(n, k): the Cephes binomial."""
    if n < 0.0 and n == floor(n):
        return nan()
    else:
        kx = floor(k)
        if k == kx and (abs(n) > 1.0e-8 or n == 0.0):
            return binom_intk(n, k, kx)
        else:
            return binom_large(n, k)


class PochState:
    """poch's running (a, m, r) state through its two recurrences."""
    a: float
    m: float
    r: float


def poch_down(a: float, m: float, r: float) -> PochState:
    """poch's m >= 1 recurrence: r *= (a + m - 1) while m descends."""
    if m < 1.0:
        return PochState(a=a, m=m, r=r)
    elif a + m == 1.0:
        return PochState(a=a, m=m, r=r)
    else:
        m2 = m - 1.0
        r2 = r * (a + m2)
        if is_inf(r2) or r2 == 0.0:
            return PochState(a=a, m=m2, r=r2)
        else:
            return poch_down(a, m2, r2)


def poch_up(a: float, m: float, r: float) -> PochState:
    """poch's m <= -1 recurrence: r /= (a + m) while m ascends."""
    if m > -1.0:
        return PochState(a=a, m=m, r=r)
    elif a + m == 0.0:
        return PochState(a=a, m=m, r=r)
    else:
        r2 = r / (a + m)
        m2 = m + 1.0
        if is_inf(r2) or r2 == 0.0:
            return PochState(a=a, m=m2, r=r2)
        else:
            return poch_up(a, m2, r2)


def is_nonpos_int(x: float) -> bool:
    """Whether x is a non-positive integer of magnitude below 1e13."""
    return x <= 0.0 and x == floor(x) and abs(x) < 1.0e13


def gammasgn(x: float) -> float:
    """Cephes gammasgn: the sign of gamma(x), NaN at poles."""
    if is_nan(x):
        return x
    elif x > 0.0:
        return 1.0
    elif x == 0.0:
        return -1.0 if is_neg_zero(x) else 1.0
    elif is_inf(x):
        return nan()
    else:
        fx = floor(x)
        if x - fx == 0.0:
            return nan()
        elif not even_f(fx):
            return -1.0
        else:
            return 1.0


def poch_asymp(a: float, m: float, r: float) -> float:
    """poch's asymptotic form for a > 1e4 and |m| <= 1."""
    t0 = 1.0 + m * (m - 1.0) / (2.0 * a)
    t1 = t0 + m * (m - 1.0) * (m - 2.0) * (3.0 * m - 1.0) / (24.0 * a * a)
    t = t1 + m * m * (m - 1.0) * (m - 1.0) * (m - 2.0) * (m - 3.0) / (48.0 * a * a * a)
    return r * pow_pos(a, m) * t


def poch_finish(a: float, s: PochState) -> float:
    """poch after its two recurrences: the Cephes terminal cases."""
    if s.m == 0.0:
        return s.r
    elif a > 1.0e4 and abs(s.m) <= 1.0:
        return poch_asymp(a, s.m, s.r)
    elif is_nonpos_int(a + s.m) and (not is_nonpos_int(a)) and a + s.m != s.m:
        return pos_inf()
    elif (not is_nonpos_int(a + s.m)) and is_nonpos_int(a):
        return 0.0
    else:
        lv = lgam_sgn(a + s.m)
        lv2 = lgam_sgn(a)
        sv1 = gammasgn(a + s.m)
        sv2 = gammasgn(a)
        return s.r * exp_s(lv.value - lv2.value) * sv1 * sv2


def poch(a: float, m: float) -> float:
    """scipy.special.poch(a, m): the Cephes rising factorial."""
    s0 = poch_down(a, m, 1.0)
    s1 = poch_up(a, s0.m, s0.r)
    return poch_finish(a, s1)


def comb(n: float, k: float) -> float:
    """scipy.special.comb(N, k, exact=False): binom, zero outside 0 <= k <= N."""
    if (k <= n) and (n >= 0.0) and (k >= 0.0):
        return binom(n, k)
    else:
        return 0.0


def perm(n: float, k: float) -> float:
    """scipy.special.perm(N, k, exact=False): poch(N-k+1, k), zero outside range."""
    if (k <= n) and (n >= 0.0) and (k >= 0.0):
        return poch(n - k + 1.0, k)
    else:
        return 0.0
