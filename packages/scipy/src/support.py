# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=scipy
# osprey: module=Support
"""Private numerical helpers every `scipy` module is built from.

These are the elementary functions scipy.special's Cephes/xsf kernels need and
the Osprey flavor does not yet ship as builtins: floor via exact fmod
arithmetic, power via exp/ln, sine and cosine via quadrant-reduced Taylor
series, log1p and expm1 via series, and the polynomial/Chebyshev evaluators the
Cephes rational approximations use. Each is written with only `+ - * / %`,
`sqrt`, `exp`, `ln`, `tanh`, `abs` and comparisons, so a value here reproduces
the same binary64 arithmetic the port's CPython prototype measured against real
scipy. Values downstream of exp/ln carry the usual libm last-digit spread.
Implements [PORT-TEST-ORACLE].
"""

__all__ = [
    "MACHEP", "PI", "LOGPI_CEPHES", "SQRT2PI", "LS2PI", "MAXGAM",
    "MAXLOG", "MAXLGM", "SCIPY_EULER", "EULER16", "MAXSTIR",
    "BETA_ASYMP_FACTOR", "nan", "pos_inf", "neg_inf", "is_nan", "is_inf",
    "is_neg_zero", "exp_s", "log_s", "floor", "sign_f", "pow_pos",
    "pow_int_signed", "even_f", "float_to_int", "polevl", "p1evl",
    "chbevl", "sinpi", "cot_pi", "log1p", "expm1", "list_max", "Signed",
]

# IEEE binary64 constants used by the Cephes kernels.
MACHEP = 1.1102230246251565e-16
PI = 3.141592653589793238462643
LOGPI = 1.1447298858494001741434262
LOGPI_CEPHES = 1.144729885849400174143427351353058711647
SQRT2PI = 2.506628274631000502415765284811045253007
LS2PI = 0.91893853320467274178
MAXGAM = 171.624376956302725
MAXLOG = 709.7827128933839730962063185871
MAXLGM = 2.556348e305
SCIPY_EULER = 0.577215664901532860606512090082402431
EULER16 = 0.5772156649015329
HALF_PI = 1.5707963267948966
PI_HALF_TAIL = 6.12323399573676603587e-17
MAXSTIR = 143.01608
BETA_ASYMP_FACTOR = 1e6
# 1/(2k(2k+1)) for k = 12..1: the sin(t)/t Taylor coefficients, Horner order.
SIN_CO = [0.0016666666666666668, 0.001976284584980237, 0.002380952380952381,
          0.0029239766081871343, 0.003676470588235294, 0.004761904761904762,
          0.00641025641025641, 0.00909090909090909, 0.013888888888888888,
          0.023809523809523808, 0.05, 0.16666666666666666]
# 1/((2k-1)(2k)) for k = 13..1: the cos(t) Taylor coefficients, Horner order.
COS_CO = [0.0015384615384615385, 0.0018115942028985507, 0.0021645021645021645,
          0.002631578947368421, 0.0032679738562091504, 0.004166666666666667,
          0.005494505494505495, 0.007575757575757576, 0.011111111111111112,
          0.017857142857142856, 0.03333333333333333, 0.08333333333333333, 0.5]


def nan() -> float:
    """IEEE quiet NaN, as `inf - inf`."""
    return exp(1000.0) - exp(1000.0)


def pos_inf() -> float:
    """Positive infinity."""
    return exp(1000.0)


def neg_inf() -> float:
    """Negative infinity."""
    return 0.0 - exp(1000.0)


def is_nan(x: float) -> bool:
    """Whether `x` is NaN (the only value unequal to itself)."""
    return x != x


def is_inf(x: float) -> bool:
    """Whether `x` is an infinity of either sign."""
    return abs(x) == exp(1000.0)


def is_neg_zero(x: float) -> bool:
    """Whether `x` is negative zero, which the flavor prints as "-0.0"."""
    return x == 0.0 and toString(x) == "-0.0"


def exp_s(x: float) -> float:
    """`exp` with Cephes' overflow/underflow thresholds, matching scipy."""
    if x > MAXLOG:
        return pos_inf()
    elif x < -745.1332191019411:
        return 0.0
    else:
        return exp(x)


def log_s(x: float) -> float:
    """`ln` with Cephes' domain handling: ln(0) is -inf, ln(<0) is NaN."""
    if x > 0.0:
        return ln(x)
    elif x == 0.0:
        return neg_inf()
    else:
        return nan()


def floor(x: float) -> float:
    """Floor via exact fmod arithmetic: `x - (x % 1.0)`, minus one when the
    fractional part is nonzero and `x` is negative."""
    if is_nan(x) or is_inf(x):
        return x
    else:
        r = x % 1.0
        t = x - r
        if r == 0.0 or x > 0.0:
            return t
        else:
            return t - 1.0


def sign_f(x: float) -> float:
    """numpy's float sign: -1.0, 0.0, 1.0, or NaN."""
    if is_nan(x):
        return nan()
    elif x > 0.0:
        return 1.0
    elif x < 0.0:
        return -1.0
    else:
        return 0.0


def pow_pos(x: float, y: float) -> float:
    """`x**y` for `x > 0`, via exp(y * ln(x))."""
    return exp_s(y * log_s(x))


def pow_int_signed(x: float, y: float) -> float:
    """`x**y` for an integer-valued `y`, any sign of `x`: the magnitude via
    pow_pos, the sign from the exact parity of `y`."""
    if x >= 0.0:
        return pow_pos(x, y)
    else:
        half = y * 0.5
        sgn = 1.0 if half == floor(half) else -1.0
        return pow_pos(0.0 - x, y) * sgn


def even_f(f: float) -> bool:
    """Parity of an integer-valued float, without a float->int conversion."""
    h = f * 0.5
    return h == floor(h)


def float_to_int(f: float) -> int:
    """An integer-valued float in `[0, 2^31)` as an int, by exact subtraction."""
    if f < 1.0:
        return 0
    else:
        return 1 + float_to_int(f - 1.0)


def polevl(x: float, coef) -> float:
    """Cephes polevl: Horner evaluation of a polynomial from coefficient 0."""
    return polevl_from(x, coef, 1, unwrap_or(listGet(coef, 0), 0.0))


def polevl_from(x: float, coef, i: int, ans: float) -> float:
    if i >= len(coef):
        return ans
    else:
        return polevl_from(x, coef, i + 1, ans * x + unwrap_or(listGet(coef, i), 0.0))


def p1evl(x: float, coef) -> float:
    """Cephes p1evl: Horner evaluation of a polynomial with a leading 1."""
    return p1evl_from(x, coef, 1, x + unwrap_or(listGet(coef, 0), 0.0))


def p1evl_from(x: float, coef, i: int, ans: float) -> float:
    if i >= len(coef):
        return ans
    else:
        return p1evl_from(x, coef, i + 1, ans * x + unwrap_or(listGet(coef, i), 0.0))


class ChState:
    """Chebyshev recurrence state."""
    b0: float
    b1: float
    b2: float


def chbevl(x: float, coef, n: int) -> float:
    """Cephes chbevl: Clenshaw evaluation of a Chebyshev series of degree n."""
    s = chbevl_step(x, coef, 1, ChState(b0=unwrap_or(listGet(coef, 0), 0.0), b1=0.0, b2=0.0), n)
    return 0.5 * (s.b0 - s.b2)


def chbevl_step(x: float, coef, i: int, s: ChState, n: int) -> ChState:
    if i >= n:
        return s
    else:
        return chbevl_step(x, coef, i + 1,
                           ChState(b0=x * s.b0 - s.b1 + unwrap_or(listGet(coef, i), 0.0),
                                   b1=s.b0, b2=s.b1), n)


def taylor_horner(u: float, coef, i: int, p: float) -> float:
    """The shared Horner loop of the sin and cos Taylor series."""
    if i >= len(coef):
        return p
    else:
        return taylor_horner(u, coef, i + 1, 1.0 - u * unwrap_or(listGet(coef, i), 0.0) * p)


def sin_quad(t: float) -> float:
    """sin(t) for `t >= 0` and `|t| <= pi/2`, Taylor/Horner."""
    return t * taylor_horner(t * t, SIN_CO, 0, 1.0)


def sin_half(t: float) -> float:
    """sin(t) for `-pi/2 <= t <= pi/2`."""
    if t >= 0.0:
        return sin_quad(t)
    else:
        return 0.0 - sin_quad(0.0 - t)


def cos_quarter(t: float) -> float:
    """cos(t) for `0 <= t <= pi/4`, Taylor/Horner."""
    return taylor_horner(t * t, COS_CO, 0, 1.0)


def cos_half(t: float) -> float:
    """cos(t) for `0 <= t <= pi/2`, with a double-double pi/2 so tiny results
    keep their relative accuracy above pi/4."""
    if t > 0.5 * HALF_PI:
        d = HALF_PI - t
        return sin_quad(d) + cos_quarter(d) * PI_HALF_TAIL
    else:
        return cos_quarter(t)


def sinpi(z: float) -> float:
    """Cephes sinpi: argument reduced by fmod(x, 2), then the Taylor sine."""
    neg = z < 0.0 or is_neg_zero(z)
    s = -1.0 if neg else 1.0
    x = (0.0 - z) if neg else z
    r = x % 2.0
    if r < 0.5:
        return s * sin_half(PI * r)
    elif r > 1.5:
        return s * sin_half(PI * (r - 2.0))
    else:
        return (0.0 - s) * sin_half(PI * (r - 1.0))


def cot_pi(r: float) -> float:
    """cot(pi*r) for `-1 < r < 0` (the psi reflection term), reduced exactly."""
    u = PI * r
    v = u % PI
    w = v if v >= -HALF_PI else v + PI
    if abs(w) <= HALF_PI * 0.5:
        return cos_half(abs(w)) / sin_half(abs(w)) * (1.0 if w >= 0.0 else -1.0)
    else:
        x = HALF_PI - abs(w)
        val = sin_quad(x) / cos_half(x)
        return val * (1.0 if w >= 0.0 else -1.0)


def log1p(x: float) -> float:
    """ln(1+x) accurate for small |x|: the 2*atanh(x/(2+x)) series."""
    if x == 0.0:
        return x
    elif abs(x) < 2.220446049250313e-16:
        return x
    elif abs(x) > 0.5:
        return log_s(1.0 + x)
    else:
        u = x / (2.0 + x)
        u2 = u * u
        return 2.0 * u * atanh_series(u2, 15, 1.0 / 31.0)


def atanh_series(u2: float, k: int, p: float) -> float:
    """The atanh Horner tail: p = 1/(2k-1) + u2*p for k down to 1."""
    if k < 1:
        return p
    else:
        return atanh_series(u2, k - 1, 1.0 / toFloat(2 * k - 1) + u2 * p)


def expm1(x: float) -> float:
    """exp(x)-1 accurate for small |x|: Taylor below 0.5, exp(x)-1 above."""
    if abs(x) > 0.5:
        return exp_s(x) - 1.0
    else:
        return x * expm1_p(x, 19, 1.0 / 20.0)


def expm1_p(x: float, k: int, p: float) -> float:
    """The expm1 Horner tail: p = 1 + x*p/k for k down to 2."""
    if k < 2:
        return p
    else:
        return expm1_p(x, k - 1, 1.0 + x * p / toFloat(k))


def list_max(xs) -> float:
    """The largest element of a float list; -inf for an empty list."""
    return fold(range(len(xs)), neg_inf(),
                lambda a, i: (unwrap_or(listGet(xs, i), neg_inf()) if unwrap_or(listGet(xs, i), neg_inf()) > a else a))


class Signed:
    """A real value and its sign (1.0 or -1.0), the shape lgam/lbeta return."""
    value: float
    sign: float
