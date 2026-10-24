# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=scipy
# osprey: module=Zeta
"""scipy.special's zeta family, from the Cephes zeta/zetac kernels.

`zeta(x)` is the Riemann zeta (q=1); `zeta_q(x, q)` is the Hurwitz zeta,
computed with the Cephes Euler-Maclaurin tail after a partial sum. Riemann zeta
is zetac(x)+1: a rational approximation on (0,1], another on (1,10], an
exp(polynomial) on (10,50], and a direct inverse-power sum above. Poles and
domain edges reproduce scipy: zeta(1)=inf, zeta_q at a non-positive integer q
is inf. Implements [PORT-TEST-ORACLE].
"""
from scipy.Support import (MACHEP, nan, pos_inf, neg_inf, is_nan, exp_s,
                           pow_pos, pow_int_signed, floor, float_to_int,
                           polevl, p1evl)

__all__ = ["zeta", "zeta_q", "zetac"]

zeta_A = [12.0, -720.0, 30240.0, -1209600.0, 47900160.0, -1.8924375803183791606e9,
          7.47242496e10, -2.950130727918164224e12, 1.1646782814350067249e14,
          -4.5979787224074726105e15, 1.8152105401943546773e17, -7.1661652561756670113e18]

azetac = [-1.5, 0.0,
          6.44934066848226436472e-1, 2.02056903159594285400e-1,
          8.23232337111381915160e-2, 3.69277551433699263314e-2,
          1.73430619844491397145e-2, 8.34927738192282683980e-3,
          4.07735619794433937869e-3, 2.00839282608221441785e-3,
          9.94575127818085337146e-4, 4.94188604119464558702e-4,
          2.46086553308048298638e-4, 1.22713347578489146752e-4,
          6.12481350587048292585e-5, 3.05882363070204935517e-5,
          1.52822594086518717326e-5, 7.63719763789976227360e-6,
          3.81729326499983985646e-6, 1.90821271655393892566e-6,
          9.53962033872796113152e-7, 4.76932986787806463117e-7,
          2.38450502727732990004e-7, 1.19219925965311073068e-7,
          5.96081890512594796124e-8, 2.98035035146522801861e-8,
          1.49015548283650412347e-8, 7.45071178983542949198e-9,
          3.72533402478845705482e-9, 1.86265972351304900640e-9,
          9.31327432419668182872e-10]

zetac_P = [5.85746514569725319540e11, 2.57534127756102572888e11, 4.87781159567948256438e10,
           5.15399538023885770696e9, 3.41646073514754094281e8, 1.60837006880656492731e7,
           5.92785467342109522998e5, 1.51129169964938823117e4, 2.01822444485997955865e2]
zetac_Q = [3.90497676373371157516e11, 5.22858235368272161797e10, 5.64451517271280543351e9,
           3.39006746015350418834e8, 1.79410371500126453702e7, 5.66666825131384797029e5,
           1.60382976810944131506e4, 1.96436237223387314144e2]
zetac_A = [8.70728567484590192539e6, 1.76506865670346462757e8, 2.60889506707483264896e10,
           5.29806374009894791647e11, 2.26888156119238241487e13, 3.31884402932705083599e14,
           5.13778997975868230192e15, -1.98123688133907171455e15, -9.92763810039983572356e16,
           7.82905376180870586444e16, 9.26786275768927717187e16]
zetac_B = [-7.92625410563741062861e6, -1.60529969932920229676e8, -2.37669260975543221788e10,
           -4.80319584350455169857e11, -2.07820961754173320170e13, -2.96075404507272223680e14,
           -4.86299103694609136686e15, 5.34589509675789930199e15, 5.71464111092297631292e16,
           -1.79915597658676556828e16]
zetac_R = [-3.28717474506562731748e-1, 1.55162528742623950834e1, -2.48762831680821954401e2,
           1.01050368053237678329e3, 1.26726061410235149405e4, -1.11578094770515181334e5]
zetac_S = [1.95107674914060531512e1, 3.17710311750646984099e2, 3.03835500874445748734e3,
           2.03665876435770579345e4, 7.43853965136767874343e4]
zetac_TAYLOR0 = [-1.0000000009110164892, -1.0000000057646759799,
                 -9.9999983138417361078e-1, -1.0000013011460139596,
                 -1.000001940896320456, -9.9987929950057116496e-1,
                 -1.000785194477042408, -1.0031782279542924256,
                 -9.1893853320467274178e-1, -1.5]
ZETAC_MAXL2 = 127.0


def azetac_at(x: float) -> float:
    """azetac[int(x)] for an integer-valued x in [0, 30]."""
    return unwrap_or(listGet(azetac, float_to_int(x)), 0.0)


def zeta_em_step(x: float, s: float, a: float, bb: float, w: float, i: int) -> float:
    """One Euler-Maclaurin tail step (two half-steps), Cephes order; bounded
    to the 12-term table."""
    if i >= 12:
        return s
    else:
        k = 2.0 * toFloat(i)
        a2 = a * (x + k)
        b2 = bb / w
        t = a2 * b2 / unwrap_or(listGet(zeta_A, i), 0.0)
        s2 = s + t
        if abs(t) < MACHEP * abs(s2):
            return s2
        else:
            a3 = a2 * (x + k + 1.0)
            b3 = b2 / w
            return zeta_em_step(x, s2, a3, b3, w, i + 1)


def zeta_em_tail(x: float, s: float, b: float, w: float) -> float:
    """The Cephes Euler-Maclaurin correction for the Hurwitz zeta."""
    return zeta_em_step(x, s, 1.0, b, w, 0)


def zeta_q(x: float, q: float) -> float:
    """scipy.special.zeta(x, q): the Hurwitz zeta, Cephes algorithm."""
    if x == 1.0:
        return pos_inf()
    elif x < 1.0:
        return nan()
    elif q <= 0.0:
        if q == floor(q):
            return pos_inf()
        elif x != floor(x):
            return nan()
        else:
            return zeta_q_loop(x, q, pow_int_signed(q, 0.0 - x), q, 0, 0.0)
    elif q > 1.0e8:
        return (1.0 / (x - 1.0) + 1.0 / (2.0 * q)) * pow_pos(q, 1.0 - x)
    else:
        return zeta_q_loop(x, q, pow_int_signed(q, 0.0 - x), q, 0, 0.0)


def zeta_q_loop(x: float, q: float, s: float, a: float, i: int, b: float) -> float:
    if (i < 9) or (a <= 9.0):
        a2 = a + 1.0
        b2 = pow_int_signed(a2, 0.0 - x)
        s2 = s + b2
        if abs(b2) < MACHEP * abs(s2):
            return s2
        else:
            return zeta_q_loop(x, q, s2, a2, i + 1, b2)
    else:
        w = a
        s2 = s + b * w / (x - 1.0)
        s3 = s2 - 0.5 * b
        return zeta_em_tail(x, s3, b, w)


def zetac_tail(x: float, a: float, s: float) -> float:
    """The x > 50 direct inverse-power sum for zetac."""
    a2 = a + 2.0
    b = pow_pos(a2, 0.0 - x)
    s2 = s + b
    if not (b > MACHEP * s2):
        bb = pow_pos(2.0, 0.0 - x)
        return (s2 + bb) / (1.0 - bb)
    else:
        return zetac_tail(x, a2, s2)


def zetac_positive(x: float) -> float:
    """Cephes zetac (zeta(x)-1) for x > 0."""
    if x == 1.0:
        return pos_inf()
    elif x >= ZETAC_MAXL2:
        return 0.0
    elif x == floor(x) and x < 31.0:
        return azetac_at(x)
    else:
        return zetac_rational(x)


def zetac_rational(x: float) -> float:
    """zetac's rational/exp/sum branches for non-integer x below 127."""
    if x < 1.0:
        w = 1.0 - x
        return polevl(x, zetac_R) / (w * p1evl(x, zetac_S))
    elif x <= 10.0:
        b = pow_pos(2.0, x) * (x - 1.0)
        w = 1.0 / x
        return (x * polevl(w, zetac_P)) / (b * p1evl(w, zetac_Q))
    elif x <= 50.0:
        b = pow_pos(2.0, 0.0 - x)
        w = polevl(x, zetac_A) / p1evl(x, zetac_B)
        return exp_s(w) + b
    else:
        return zetac_tail(x, 1.0, 0.0)


def zetac(x: float) -> float:
    """Cephes zetac: zeta(x) - 1 for x > 0; NaN below (the reflection path
    needs gamma, which would cycle the module graph)."""
    if is_nan(x):
        return x
    elif x == neg_inf():
        return nan()
    elif x < 0.0 and x > -0.01:
        return polevl(x, zetac_TAYLOR0)
    elif x < 0.0:
        return nan()
    else:
        return zetac_positive(x)


def zeta(x: float) -> float:
    """scipy.special.zeta(x): the Riemann zeta (q=1) for x > 0, inf at 1,
    and the Cephes Taylor form just below zero; NaN below -0.01."""
    if is_nan(x):
        return x
    elif x == neg_inf():
        return nan()
    elif x < 0.0 and x > -0.01:
        return 1.0 + polevl(x, zetac_TAYLOR0)
    elif x < 0.0:
        return nan()
    else:
        return 1.0 + zetac_positive(x)
