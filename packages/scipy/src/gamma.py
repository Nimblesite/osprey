# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Gamma
"""scipy.special's gamma family, from the Cephes kernels scipy compiles.

gamma is the Cephes rational approximation below |x|<=33 and Stirling's series
above; gammaln/loggamma are the Cephes lgam rational/Stirling path with the
x<-34 reflection; digamma is the Boost/Cephes psi (rational near [1,2], the
asymptotic series above, and the zeta-series expansion around the negative
root); polygamma is (-1)^(n+1) * n! * zeta(n+1, x); rgamma is the Cephes
Chebyshev approximation. Poles reproduce scipy exactly: gamma(0)=-inf and
gamma at negative integers are NaN, gammaln at a pole is +inf. Values match
scipy to ~1e-13 relative where exp/ln dominate and ~1e-15 elsewhere.
Implements [PORT-TEST-ORACLE].
"""
from scipy.Support import (MACHEP, PI, LOGPI_CEPHES, SQRT2PI, LS2PI, MAXGAM,
                           MAXLGM, SCIPY_EULER, EULER16, MAXSTIR, nan, pos_inf,
                           neg_inf, is_nan, is_inf, is_neg_zero, exp_s, log_s,
                           floor, pow_pos, even_f, float_to_int, polevl,
                           p1evl, chbevl, sinpi, cot_pi, Signed)
from scipy.Zeta import zeta_q

__all__ = ["gamma", "gammaln", "loggamma", "digamma", "polygamma", "rgamma", "psi", "lgam_sgn"]

gamma_P = [1.60119522476751861407e-4, 1.19135147006586384913e-3, 1.04213797561761569935e-2,
           4.76367800457137231464e-2, 2.07448227648435975150e-1, 4.94214826801497100753e-1,
           9.99999999999999996796e-1]
gamma_Q = [-2.31581873324120129819e-5, 5.39605580493303397842e-4, -4.45641913851797240494e-3,
           1.18139785222060435552e-2, 3.58236398605498653373e-2, -2.34591795718243348568e-1,
           7.14304917030273074085e-2, 1.00000000000000000320]
gamma_STIR = [7.87311395793093628397e-4, -2.29549961613378126380e-4, -2.68132617805781232825e-3,
              3.47222221605458667310e-3, 8.33333333333482257126e-2]

gamma_A = [8.11614167470508450300e-4, -5.95061904284301438324e-4, 7.93650340457716943945e-4,
           -2.77777777730099687205e-3, 8.33333333333331927722e-2]
gamma_B = [-1.37825152569120859100e3, -3.88016315134637840924e4, -3.31612992738871184744e5,
           -1.16237097492762307383e6, -1.72173700820839662146e6, -8.53555664245765465627e5]
gamma_C = [-3.51815701436523470549e2, -1.70642106651881159223e4, -2.20528590553854454839e5,
           -1.13933444367982507207e6, -2.53252307177582951285e6, -2.01889141433532773231e6]

psi_A = [8.33333333333333333333e-2, -2.10927960927960927961e-2, 7.57575757575757575758e-3,
         -4.16666666666666666667e-3, 3.96825396825396825397e-3, -8.33333333333333333333e-3,
         8.33333333333333333333e-2]
PSI_Y = 0.9955816268920898
psi_root1 = 1569415565.0 / 1073741824.0
psi_root2 = (381566830.0 / 1073741824.0) / 1073741824.0
psi_root3 = 0.9016312093258695918615325266959189453125e-19
psi_P = [-0.0020713321167745952, -0.045251321448739056, -0.28919126444774784,
         -0.65031853770896507, -0.32555031186804491, 0.25479851061131551]
psi_Q = [-0.55789841321675513e-6, 0.0021284987017821144, 0.054151797245674225,
         0.43593529692665969, 1.4606242909763515, 2.0767117023730469, 1.0]

digamma_posroot = 1.4616321449683623
digamma_posrootval = -9.2412655217294275e-17
digamma_negroot = -0.504083008264455409
digamma_negrootval = 7.2897639029768949e-17

rgamma_R = [3.13173458231230000000e-17, -6.70718606477908000000e-16,
            2.20039078172259550000e-15, 2.47691630348254132600e-13,
            -6.60074100411295197440e-12, 5.13850186324226978840e-11,
            1.08965386454418662084e-9, -3.33964630686836942556e-8,
            2.68975996440595483619e-7, 2.96001177518801696639e-6,
            -8.04814124978471142852e-5, 4.16609138709688864714e-4,
            5.06579864028608725080e-3, -6.41925436109158228810e-2,
            -4.98558728684003594785e-3, 1.27546015610523951063e-1]


def stirf_pow(x: float, y0: float) -> float:
    """Stirling's power term, the overflow-safe split above MAXSTIR."""
    if x > MAXSTIR:
        v = pow_pos(x, 0.5 * x - 0.25)
        return v * (v / y0)
    else:
        return pow_pos(x, x - 0.5) / y0


def stirf(x: float) -> float:
    """Cephes Stirling series for gamma(x), x > 33."""
    if x >= MAXGAM:
        return pos_inf()
    else:
        w0 = 1.0 / x
        w = 1.0 + w0 * polevl(w0, gamma_STIR)
        y0 = exp_s(x)
        return SQRT2PI * stirf_pow(x, y0) * w


def gamma_small(x: float, z: float) -> float:
    """gamma near zero, the Cephes small-argument term."""
    if x == 0.0:
        return nan()
    else:
        return z / ((1.0 + EULER16 * x) * x)


def gamma_c3(x: float, z: float) -> float:
    if x < 2.0:
        if x < 1.0e-9:
            return gamma_small(x, z)
        else:
            return gamma_c3(x + 1.0, z / x)
    elif x == 2.0:
        return z
    else:
        t = x - 2.0
        return z * polevl(t, gamma_P) / polevl(t, gamma_Q)


def gamma_c2(x: float, z: float) -> float:
    if x < 0.0:
        if x > -1.0e-9:
            return gamma_small(x, z)
        else:
            return gamma_c2(x + 1.0, z / x)
    else:
        return gamma_c3(x, z)


def gamma_c1(x: float, z: float) -> float:
    if x >= 3.0:
        return gamma_c1(x - 1.0, z * (x - 1.0))
    else:
        return gamma_c2(x, z)


def gamma_neg(x: float) -> float:
    """gamma(x) for x < -33, the Cephes reflection off Stirling."""
    q = abs(x)
    p = floor(q)
    if p == q:
        return nan()
    else:
        sgngam = -1.0 if even_f(p) else 1.0
        z0 = q - p
        z1 = q - (p + 1.0) if z0 > 0.5 else z0
        z2 = q * sinpi(z1)
        if z2 == 0.0:
            return sgngam * pos_inf()
        else:
            return sgngam * (PI / (abs(z2) * stirf(q)))


def gamma(x: float) -> float:
    """scipy.special.gamma: the Cephes gamma, poles and signs included."""
    if is_nan(x) or is_inf(x):
        return x if x > 0.0 else nan()
    elif x == 0.0:
        return neg_inf() if is_neg_zero(x) else pos_inf()
    elif abs(x) > 33.0:
        if x < 0.0:
            return gamma_neg(x)
        else:
            return stirf(x)
    else:
        return gamma_c1(x, 1.0)


def lgam_large_x(x: float) -> float:
    """gammaln for x >= 1e8: Stirling with the first two corrections."""
    q = (x - 0.5) * log_s(x) - x + LS2PI
    if x > 1.0e8:
        return q
    else:
        p0 = 1.0 / (x * x)
        p1 = ((7.9365079365079365079365e-4 * p0 - 2.7777777777777777777778e-3) * p0
              + 0.0833333333333333333333) / x
        return q + p1


def lgam_b(x: float, z: float, p: float) -> Signed:
    u = x + p
    if u < 2.0:
        if u == 0.0:
            return Signed(value=pos_inf(), sign=1.0)
        else:
            return lgam_b(x, z / u, p + 1.0)
    else:
        sign = -1.0 if z < 0.0 else 1.0
        zz = abs(z)
        if u == 2.0:
            return Signed(value=log_s(zz), sign=sign)
        else:
            pp = p - 2.0
            xx = x + pp
            r = xx * polevl(xx, gamma_B) / p1evl(xx, gamma_C)
            return Signed(value=log_s(zz) + r, sign=sign)


def lgam_a(x: float, z: float, p: float) -> Signed:
    u = x + p
    if u >= 3.0:
        return lgam_a(x, z * (x + p - 1.0), p - 1.0)
    else:
        return lgam_b(x, z, p)


def lgam_neg(x: float) -> Signed:
    """lgam for x < -34, the Cephes reflection."""
    q = 0.0 - x
    w = lgam_sgn(q)
    p = floor(q)
    if p == q:
        return Signed(value=pos_inf(), sign=1.0)
    else:
        sign = -1.0 if even_f(p) else 1.0
        z0 = q - p
        z1 = p + 1.0 - q if z0 > 0.5 else z0
        z2 = q * sinpi(z1)
        if z2 == 0.0:
            return Signed(value=pos_inf(), sign=1.0)
        else:
            return Signed(value=LOGPI_CEPHES - log_s(z2) - w.value, sign=sign)


def lgam_sgn(x: float) -> Signed:
    """Cephes lgam: (log|gamma(x)|, sign of gamma(x))."""
    if is_nan(x):
        return Signed(value=nan(), sign=1.0)
    elif is_inf(x):
        return Signed(value=x, sign=1.0)
    elif x < -34.0:
        return lgam_neg(x)
    elif x < 13.0:
        return lgam_a(x, 1.0, 0.0)
    elif x > MAXLGM:
        return Signed(value=pos_inf(), sign=1.0)
    elif x >= 1000.0:
        return Signed(value=lgam_large_x(x), sign=1.0)
    else:
        q = (x - 0.5) * log_s(x) - x + LS2PI
        p = 1.0 / (x * x)
        return Signed(value=q + polevl(p, gamma_A) / x, sign=1.0)


def gammaln(x: float) -> float:
    """scipy.special.gammaln: log(|gamma(x)|)."""
    return lgam_sgn(x).value


def loggamma(x: float) -> float:
    """scipy.special.loggamma: gammaln for x >= 0, NaN below."""
    if x < 0.0:
        return nan()
    else:
        return gammaln(x)


class Xy:
    """A reduced (x, y) pair from psi's downward recurrence."""
    x: float
    y: float


def psi_asy(x: float) -> float:
    """The psi asymptotic series log(x) - 1/(2x) - sum, for x >= 10."""
    z = 1.0 / (x * x)
    y = z * polevl(z, psi_A) if x < 1.0e17 else 0.0
    return log_s(x) - (0.5 / x) - y


def psi_int_sum(y: float, n: int) -> float:
    """psi at a positive integer: y + sum_{i=1}^{n-1} 1/i - Euler."""
    return psi_int_sum_from(y, n, 1)


def psi_int_sum_from(y: float, n: int, i: int) -> float:
    if i >= n:
        return y - SCIPY_EULER
    else:
        return psi_int_sum_from(y + 1.0 / toFloat(i), n, i + 1)


def psi_reduce(x: float, y: float) -> Xy:
    if x > 2.0:
        return psi_reduce(x - 1.0, y + 1.0 / (x - 1.0))
    else:
        return Xy(x=x, y=y)


def psi_imp_1_2(x: float) -> float:
    """psi on [1, 2]: the Boost root-shifted rational."""
    g0 = x - psi_root1
    g1 = g0 - psi_root2
    g = g1 - psi_root3
    r = polevl(x - 1.0, psi_P) / polevl(x - 1.0, psi_Q)
    return g * PSI_Y + g * r


def psi_tail(x: float, y: float) -> float:
    """psi's final step: the [1,2] rational or the asymptotic series."""
    if 1.0 <= x and x <= 2.0:
        return y + psi_imp_1_2(x)
    else:
        return y + psi_asy(x)


def psi_pos(x: float, y: float) -> float:
    """psi for x > 0: integer, upward, or downward reduction to [1, 2]."""
    if x <= 10.0 and x == floor(x):
        return psi_int_sum(y, float_to_int(x))
    elif x < 1.0:
        return psi_pos(x + 1.0, y - 1.0 / x)
    elif x < 10.0:
        xy = psi_reduce(x, y)
        return psi_tail(xy.x, xy.y)
    else:
        return psi_tail(x, y)


def psi(x: float) -> float:
    """Cephes psi (digamma without the zeta-series root path)."""
    if is_nan(x):
        return x
    elif x == pos_inf():
        return x
    elif x == neg_inf():
        return nan()
    elif x == 0.0:
        return pos_inf() if is_neg_zero(x) else neg_inf()
    elif x < 0.0:
        r = x % 1.0
        if r == 0.0:
            return nan()
        else:
            y = (0.0 - PI) * cot_pi(r)
            return psi_pos(1.0 - x, y)
    else:
        return psi_pos(x, 0.0)


def digamma_zeta_series(z: float, root: float, rootval: float) -> float:
    """digamma around a root via its zeta series, Cephes order."""
    coeff = -1.0
    return digamma_series(z - root, root, coeff, rootval, 1)


def digamma_series(zz: float, root: float, coeff: float, res: float, n: int) -> float:
    coeff2 = coeff * (0.0 - zz)
    term = coeff2 * zeta_q(toFloat(n) + 1.0, root)
    res2 = res + term
    if abs(term) < MACHEP * abs(res2):
        return res2
    else:
        return digamma_series(zz, root, coeff2, res2, n + 1)


def digamma(z: float) -> float:
    """scipy.special.digamma (psi): zeta series around the negative root, else
    the Cephes psi."""
    if abs(z - digamma_negroot) < 0.3:
        return digamma_zeta_series(z, digamma_negroot, digamma_negrootval)
    else:
        return psi(z)


def polygamma(n: int, x: float) -> float:
    """scipy.special.polygamma(n, x): (-1)^(n+1) * n! * zeta(n+1, x)."""
    if n == 0:
        return psi(x)
    else:
        sgn = 1.0 if even_f(toFloat(n) + 1.0) else -1.0
        return sgn * gamma(toFloat(n) + 1.0) * zeta_q(toFloat(n) + 1.0, x)


def rgamma_down(w: float, z: float) -> float:
    if w > 1.0:
        return rgamma_down(w - 1.0, z * (w - 1.0))
    else:
        return rgamma_up(w, z)


def rgamma_up(w: float, z: float) -> float:
    if w < 0.0:
        return rgamma_up(w + 1.0, z / w)
    elif w == 0.0:
        return 0.0
    elif w == 1.0:
        return 1.0 / z
    else:
        return w * (1.0 + chbevl(4.0 * w - 2.0, rgamma_R, 16)) / z


def rgamma(x: float) -> float:
    """scipy.special.rgamma: 1/gamma(x), with the Cephes sign-of-zero rules."""
    if x == 0.0:
        return x
    elif x < 0.0 and x == floor(x):
        return 0.0
    elif abs(x) > 4.0:
        g = gamma(x)
        if g == 0.0:
            return neg_inf() if is_neg_zero(g) else pos_inf()
        else:
            return 1.0 / g
    else:
        return rgamma_down(x, 1.0)
