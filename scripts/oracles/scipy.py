"""CPython oracle for @osprey/scipy: scipy.special scalar-function values.

Writes packages/scipy/tests/expected.py, which the suite compares the port's
`scipy.Special` functions against ([PORT-TEST-ORACLE], spec 0043). Run with the
oracle virtualenv: `python scripts/oracles/scipy.py`.
"""
import math
import sys

from expected_module import ROOT, literal, non_finite

# This script is named scipy.py; drop its own directory from sys.path so
# `import scipy` resolves to the real package, not to this file.
sys.path.pop(0)

import numpy
import scipy
import scipy.special as sp


def grid(lo, hi, n):
    return [lo + (hi - lo) * i / (n - 1) for i in range(n)]


def split1(xs, ys):
    """Split paired (x, y) into all-finite lists and lists holding a non-finite."""
    fin_x, fin_y, nf_x, nf_y = [], [], [], []
    for x, y in zip(xs, ys):
        if math.isfinite(x) and math.isfinite(y):
            fin_x.append(x)
            fin_y.append(y)
        else:
            nf_x.append(x)
            nf_y.append(y)
    return fin_x, fin_y, nf_x, nf_y


def split2(xs, ys, zs):
    fin_x, fin_y, fin_z, nf_x, nf_y, nf_z = [], [], [], [], [], []
    for x, y, z in zip(xs, ys, zs):
        if math.isfinite(x) and math.isfinite(y) and math.isfinite(z):
            fin_x.append(x)
            fin_y.append(y)
            fin_z.append(z)
        else:
            nf_x.append(x)
            nf_y.append(y)
            nf_z.append(z)
    return fin_x, fin_y, fin_z, nf_x, nf_y, nf_z


# --- gamma / gammaln / loggamma / rgamma / digamma ----------------------
gamma_x = (grid(-33.5, 32.5, 67) + grid(33.0001, 171.5, 40)
           + grid(-171.5, -33.0001, 40) + [1e-300, 1e-20, 1e-10, 1e-9, -1e-10,
           -1e-9, 0.5, 1.0, 2.0, 0.001, 100.0, 170.0, 171.0,
           0.0, -0.0, -1.0, -2.0, -3.0, -170.0, -171.0, float("inf"),
           float("-inf"), float("nan")])
GAMMA_X, GAMMA_Y, GAMMA_NF_X, GAMMA_NF_Y = split1(gamma_x, [float(sp.gamma(x)) for x in gamma_x])

gammaln_x = (grid(0.0001, 12.999, 40) + grid(13.0, 999.0, 40)
             + [1000.5, 1e5, 1e7, 1e8, 1e15, 1e100, 1e300]
             + grid(-33.999, -0.0001, 34) + grid(-171.5, -34.001, 30)
             + [1 + 1e-12, 1 - 1e-12, 2 + 1e-12, 2 - 1e-12, 0.5, 3.5,
                0.0, -0.0, -1.0, -2.0, -171.0, float("inf"), float("-inf"),
                float("nan"), 1e300, -1e300])
GAMMALN_X, GAMMALN_Y, GAMMALN_NF_X, GAMMALN_NF_Y = split1(gammaln_x, [float(sp.gammaln(x)) for x in gammaln_x])

loggamma_x = grid(0.0001, 200.0, 40) + [0.5, 1.0, 1.5, 2.0, 5.0, 100.0, -1.0, float("nan")]
LOGGAMMA_X, LOGGAMMA_Y, LOGGAMMA_NF_X, LOGGAMMA_NF_Y = split1(loggamma_x, [float(sp.loggamma(x)) for x in loggamma_x])

rgamma_x = grid(-4.0, 4.0, 33) + [-10.5, -5.5, 10.5, 33.5, -33.5, 100.0, -100.0,
                                   0.0, -0.0, -1.0, -2.0, float("inf"), float("nan")]
RGAMMA_X, RGAMMA_Y, RGAMMA_NF_X, RGAMMA_NF_Y = split1(rgamma_x, [float(sp.rgamma(x)) for x in rgamma_x])

digamma_x = (grid(1e-10, 10.0, 40) + grid(10.0, 1e6, 40) + [1e8, 1e16, 1e17]
             + grid(-0.999, -0.001, 20) + grid(-100.5, -1.5, 33)
             + [-0.5040830082644554, 0.5, 1.0, 2.0, 10.0,
                0.0, -0.0, -1.0, -2.0, float("inf"), float("-inf"), float("nan")])
DIGAMMA_X, DIGAMMA_Y, DIGAMMA_NF_X, DIGAMMA_NF_Y = split1(digamma_x, [float(sp.digamma(x)) for x in digamma_x])

# --- polygamma ----------------------------------------------------------
poly_n, poly_x, poly_y = [], [], []
for n in (1, 2, 3, 5):
    for x in grid(0.05, 50.0, 20) + grid(-100.5, -0.05, 20) + [0.5, 1.0]:
        poly_n.append(n)
        poly_x.append(x)
        poly_y.append(float(sp.polygamma(n, x)))
POLY_N, POLY_X, POLY_Y, POLY_NF_N, POLY_NF_X, POLY_NF_Y = split2(poly_n, poly_x, poly_y)
poly_nf_n = [1, 1, 1, 2, 2, 2]
poly_nf_x = [0.0, -1.0, float("inf"), 0.0, -1.0, float("nan")]
poly_nf_y = [float(sp.polygamma(n, x)) for n, x in zip(poly_nf_n, poly_nf_x)]
POLY_NF_N = POLY_NF_N + poly_nf_n
POLY_NF_X = POLY_NF_X + poly_nf_x
POLY_NF_Y = POLY_NF_Y + poly_nf_y

# --- zeta ---------------------------------------------------------------
zeta_x = (grid(1.0001, 9.999, 30) + grid(0.01, 0.9999, 20)
          + grid(10.0001, 50.0, 20) + grid(50.0001, 126.999, 15)
          + [2.0, 3.0, 4.0, 6.0, 30.0, 100.0, 126.5, 0.5,
             1.0, float("inf"), float("-inf"), float("nan"), -0.005])
ZETA_X, ZETA_Y, ZETA_NF_X, ZETA_NF_Y = split1(zeta_x, [float(sp.zeta(x)) for x in zeta_x])

zetaq_x, zetaq_q, zetaq_y = [], [], []
for x in [1.5, 2.0, 2.5, 3.0, 5.5, 10.5, 30.5, 100.0]:
    for q in [0.1, 0.5, 1.0, 1.5, 2.5, 10.5, 100.5]:
        zetaq_x.append(x)
        zetaq_q.append(q)
        zetaq_y.append(float(sp.zeta(x, q)))
for x, q in [(3.0, -0.5040830082644554), (2.0, -0.5), (4.0, -2.5), (5.0, -0.5),
             (1.0, 1.0), (0.5, 1.0)]:
    zetaq_x.append(x)
    zetaq_q.append(q)
    zetaq_y.append(float(sp.zeta(x, q)))
ZETAQ_X, ZETAQ_Q, ZETAQ_Y, ZETAQ_NF_X, ZETAQ_NF_Q, ZETAQ_NF_Y = split2(zetaq_x, zetaq_q, zetaq_y)

# --- beta / betaln ------------------------------------------------------
beta_a, beta_b, beta_y = [], [], []
for a in [0.05, 0.5, 1.5, 2.5, 8.5, 60.5, 170.5]:
    for b in [0.05, 0.5, 1.5, 2.5, 8.5, 60.5, 170.5]:
        beta_a.append(a)
        beta_b.append(b)
        beta_y.append(float(sp.beta(a, b)))
for a, b in [(-0.5, 2.5), (-1.5, 2.5), (-2.5, 3.5), (-5.5, 0.5), (-0.5, -0.5),
             (-10.5, 3.5), (3.5, 200.5), (10.5, 200.5), (60.5, 150.5),
             (-2.0, 2.5), (-1.0, 2.0), (-3.0, 1.5), (-2.5, -1.5), (-100.5, 50.5),
             (0.0, 0.0)]:
    beta_a.append(a)
    beta_b.append(b)
    beta_y.append(float(sp.beta(a, b)))
BETA_A, BETA_B, BETA_Y, BETA_NF_A, BETA_NF_B, BETA_NF_Y = split2(beta_a, beta_b, beta_y)

betaln_a, betaln_b, betaln_y = [], [], []
for a in [0.05, 0.5, 1.5, 2.5, 8.5, 60.5, 170.5]:
    for b in [0.05, 0.5, 1.5, 2.5, 8.5, 60.5, 170.5]:
        betaln_a.append(a)
        betaln_b.append(b)
        betaln_y.append(float(sp.betaln(a, b)))
for a, b in [(-0.5, 2.5), (-1.5, 2.5), (3.5, 200.5), (60.5, 150.5), (-2.5, -1.5),
             (0.0, 0.0)]:
    betaln_a.append(a)
    betaln_b.append(b)
    betaln_y.append(float(sp.betaln(a, b)))
BETALN_A, BETALN_B, BETALN_Y, BETALN_NF_A, BETALN_NF_B, BETALN_NF_Y = split2(betaln_a, betaln_b, betaln_y)

# --- factorial / comb / perm / binom / poch -----------------------------
fact_x = [-3.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 3.0, 5.5, 10.0, 20.0, 100.0,
          170.0, 171.0, float("nan")]
FACT_X, FACT_Y, FACT_NF_X, FACT_NF_Y = split1(fact_x, [float(sp.factorial(x)) for x in fact_x])
fact_ex_n = list(range(0, 21))
fact_ex_y = [int(sp.factorial(n, exact=True)) for n in fact_ex_n]

comb_n, comb_k, comb_y = [], [], []
for n in [0.5, 1.5, 2.5, 5.5, 10.5, 25.5, 50.5, 100.5, 5.0, 10.0, 20.0, 50.0, -1.0, 2.0]:
    for k in [0.0, 0.5, 1.5, 2.5, 5.5, 19.5, 1.0, 2.0, 3.0, 5.0, 7.0, 60.0]:
        comb_n.append(n)
        comb_k.append(k)
        comb_y.append(float(sp.comb(n, k)))
for n, k in [(1100.0, 550.0), (2000.0, 1000.0)]:
    comb_n.append(n)
    comb_k.append(k)
    comb_y.append(float(sp.comb(n, k)))
COMB_N, COMB_K, COMB_Y, COMB_NF_N, COMB_NF_K, COMB_NF_Y = split2(comb_n, comb_k, comb_y)
comb_ex_n = [5, 10, 20, 30, 12, 8, 7, 30, 0, 5, 5]
comb_ex_k = [2, 3, 10, 15, 12, 8, 0, 0, 0, 0, 5]
comb_ex_y = [int(sp.comb(n, k, exact=True)) for n, k in zip(comb_ex_n, comb_ex_k)]

perm_n, perm_k, perm_y = [], [], []
for n in [0.5, 1.5, 2.5, 5.5, 10.5, 5.0, 10.0, 20.0, -1.0]:
    for k in [0.0, 0.5, 1.5, 2.5, 5.5, 1.0, 2.0, 3.0, 5.0, 7.0]:
        perm_n.append(n)
        perm_k.append(k)
        perm_y.append(float(sp.perm(n, k)))
for n, k in [(1000.0, 500.0), (2000.0, 1000.0)]:
    perm_n.append(n)
    perm_k.append(k)
    perm_y.append(float(sp.perm(n, k)))
PERM_N, PERM_K, PERM_Y, PERM_NF_N, PERM_NF_K, PERM_NF_Y = split2(perm_n, perm_k, perm_y)
perm_ex_n = [5, 10, 20, 12, 8, 7]
perm_ex_k = [2, 3, 5, 12, 8, 0]
perm_ex_y = [int(sp.perm(n, k, exact=True)) for n, k in zip(perm_ex_n, perm_ex_k)]

binom_n, binom_k, binom_y = [], [], []
for n in [0.5, 1.5, 2.5, 5.5, 10.5, 25.5, 50.5, 100.5, 5.0, 10.0, 20.0, 50.0, 100.0]:
    for k in [0.0, 0.5, 1.5, 2.5, 5.5, 19.5, 1.0, 2.0, 3.0, 5.0, 10.0, 19.0, 25.0, 60.0]:
        binom_n.append(n)
        binom_k.append(k)
        binom_y.append(float(sp.binom(n, k)))
BINOM_N, BINOM_K, BINOM_Y, BINOM_NF_N, BINOM_NF_K, BINOM_NF_Y = split2(binom_n, binom_k, binom_y)
binom_nf_n = [-1.0, -2.0, -40.5]
binom_nf_k = [2.0, 0.5, 5e9]
binom_nf_y = [float(sp.binom(n, k)) for n, k in zip(binom_nf_n, binom_nf_k)]
BINOM_NF_N = BINOM_NF_N + binom_nf_n
BINOM_NF_K = BINOM_NF_K + binom_nf_k
BINOM_NF_Y = BINOM_NF_Y + binom_nf_y

poch_a, poch_m, poch_y = [], [], []
for a, m in [(8.0, 3.0), (10.0, 0.5), (0.5, 0.5), (1.5, 2.5), (100.5, 10.0),
             (2e4, 1.0), (1e5, 0.5), (3e4, 1.5), (-2.5, 2.5), (5.5, -2.5),
             (10.5, -3.5), (0.5, -0.5), (3.0, 5.0), (10.0, 2.0),
             (-2.0, 2.0), (-3.0, 3.0), (-2.5, 2.5)]:
    poch_a.append(a)
    poch_m.append(m)
    poch_y.append(float(sp.poch(a, m)))
POCH_A, POCH_M, POCH_Y, POCH_NF_A, POCH_NF_M, POCH_NF_Y = split2(poch_a, poch_m, poch_y)

# --- logistic / information / boxcox ------------------------------------
expit_x = grid(-800.0, 800.0, 40) + [-745.2, -708.5, 36.8, 37.0, float("inf"), float("-inf"), float("nan")]
EXPIT_X, EXPIT_Y, EXPIT_NF_X, EXPIT_NF_Y = split1(expit_x, [float(sp.expit(x)) for x in expit_x])

log_expit_x = grid(-800.0, 800.0, 40) + [-745.2, -745.0, -708.5, 36.8, float("nan")]
LOG_EXPIT_X, LOG_EXPIT_Y, LOG_EXPIT_NF_X, LOG_EXPIT_NF_Y = split1(log_expit_x, [float(sp.log_expit(x)) for x in log_expit_x])

logit_x = grid(1e-12, 0.999999999999, 30) + [1e-300, 0.3, 0.65, 0.5, 0.0, 1.0, -0.5, 1.5, float("nan")]
LOGIT_X, LOGIT_Y, LOGIT_NF_X, LOGIT_NF_Y = split1(logit_x, [float(sp.logit(x)) for x in logit_x])

xlogy_x, xlogy_y, xlogy_z = [], [], []
for x in [-1.0, 0.0, 0.5, 2.5, float("inf"), float("-inf")]:
    for y in [0.0, 0.5, 2.0, -1.0, float("inf"), float("nan")]:
        xlogy_x.append(x)
        xlogy_y.append(y)
        xlogy_z.append(float(sp.xlogy(x, y)))
XLOGY_X, XLOGY_Y, XLOGY_Z, XLOGY_NF_X, XLOGY_NF_Y, XLOGY_NF_Z = split2(xlogy_x, xlogy_y, xlogy_z)

xlog1py_x, xlog1py_y, xlog1py_z = [], [], []
for x in [-1.0, 0.0, 0.5, 2.5, float("inf")]:
    for y in [-0.5, 0.0, 0.5, 2.0, float("nan")]:
        xlog1py_x.append(x)
        xlog1py_y.append(y)
        xlog1py_z.append(float(sp.xlog1py(x, y)))
XLOG1PY_X, XLOG1PY_Y, XLOG1PY_Z, XLOG1PY_NF_X, XLOG1PY_NF_Y, XLOG1PY_NF_Z = split2(xlog1py_x, xlog1py_y, xlog1py_z)

entr_x = [-2.0, -1.0, -0.5, 0.0, 0.1, 0.5, 1.0, 2.0, 100.0, float("inf"), float("nan")]
ENTR_X, ENTR_Y, ENTR_NF_X, ENTR_NF_Y = split1(entr_x, [float(sp.entr(x)) for x in entr_x])

rel_entr_x, rel_entr_y, rel_entr_z = [], [], []
for x in [0.0, 0.5, 1.0, 2.0, -1.0, float("inf")]:
    for y in [0.0, 0.5, 1.0, 2.0, -1.0, float("inf"), float("nan")]:
        rel_entr_x.append(x)
        rel_entr_y.append(y)
        rel_entr_z.append(float(sp.rel_entr(x, y)))
REL_ENTR_X, REL_ENTR_Y, REL_ENTR_Z, REL_ENTR_NF_X, REL_ENTR_NF_Y, REL_ENTR_NF_Z = split2(rel_entr_x, rel_entr_y, rel_entr_z)

kl_div_x, kl_div_y, kl_div_z = [], [], []
for x in [0.0, 0.5, 1.0, 2.0, -1.0, float("inf")]:
    for y in [0.0, 0.5, 1.0, 2.0, -1.0, float("inf"), float("nan")]:
        kl_div_x.append(x)
        kl_div_y.append(y)
        kl_div_z.append(float(sp.kl_div(x, y)))
KL_DIV_X, KL_DIV_Y, KL_DIV_Z, KL_DIV_NF_X, KL_DIV_NF_Y, KL_DIV_NF_Z = split2(kl_div_x, kl_div_y, kl_div_z)

huber_d, huber_r, huber_y = [], [], []
for d in [-1.0, 0.0, 0.5, 1.0, 2.0]:
    for r in [-3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 3.0]:
        huber_d.append(d)
        huber_r.append(r)
        huber_y.append(float(sp.huber(d, r)))
HUBER_D, HUBER_R, HUBER_Y, HUBER_NF_D, HUBER_NF_R, HUBER_NF_Y = split2(huber_d, huber_r, huber_y)

phuber_d, phuber_r, phuber_y = [], [], []
for d in [-1.0, 0.0, 0.5, 1.0, 2.0]:
    for r in [-3.0, -1.0, 0.0, 0.5, 1.0, 3.0]:
        phuber_d.append(d)
        phuber_r.append(r)
        phuber_y.append(float(sp.pseudo_huber(d, r)))
PHUBER_D, PHUBER_R, PHUBER_Y, PHUBER_NF_D, PHUBER_NF_R, PHUBER_NF_Y = split2(phuber_d, phuber_r, phuber_y)

boxcox_x, boxcox_l, boxcox_y = [], [], []
for x in [1e-300, 0.5, 1.0, 2.0, 10.0, 1e10, 1e300]:
    for l in [1e-20, -1e-20, 0.5, -0.5, 1.0, -1.0, 2.5, -2.5, 100.0, -100.0, 710.0, -710.0]:
        boxcox_x.append(x)
        boxcox_l.append(l)
        boxcox_y.append(float(sp.boxcox(x, l)))
BOXCOX_X, BOXCOX_L, BOXCOX_Y, BOXCOX_NF_X, BOXCOX_NF_L, BOXCOX_NF_Y = split2(boxcox_x, boxcox_l, boxcox_y)

ibox_x, ibox_l, ibox_y = [], [], []
for x in [0.5, 2.0, 10.0, 1e10, -0.5, 1e300, 1.0, 100.0]:
    for l in [0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 0.001, -0.001, 0.0]:
        ibox_x.append(x)
        ibox_l.append(l)
        ibox_y.append(float(sp.inv_boxcox(x, l)))
IBOX_X, IBOX_L, IBOX_Y, IBOX_NF_X, IBOX_NF_L, IBOX_NF_Y = split2(ibox_x, ibox_l, ibox_y)

# --- list reductions ----------------------------------------------------
lse_in = [[1.0, 2.0, 3.0], [-1.0, -2.0, -3.0], [0.0], [1000.0, -1000.0],
          [-1e300, -2e300], [1.0, 1.0, 1.0]]
lse_y = [float(sp.logsumexp(xs)) for xs in lse_in]
lse_nf_in = [[], [float("inf")], [float("-inf")], [float("-inf"), float("-inf")],
             [1.0, float("-inf")], [float("inf"), float("-inf")]]
lse_nf_y = [float(sp.logsumexp(xs)) for xs in lse_nf_in]

lseb_a = [[1.0, 2.0, 3.0], [-1.0, -2.0, -3.0], [1000.0, -1000.0, 5.0],
          [1.0, 2.0], [0.0, 0.0]]
lseb_b = [[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 2.0], [0.0, 1.0], [1.0, 1.0]]
lseb_y = [float(sp.logsumexp(a, b=b)) for a, b in zip(lseb_a, lseb_b)]

soft_in = [[1.0, 2.0, 3.0], [-1.0, -2.0, -3.0], [0.0], [1000.0, -1000.0], [1.0, 1.0]]
soft_y = [[float(v) for v in sp.softmax(xs)] for xs in soft_in]

logsoft_in = soft_in
logsoft_y = [[float(v) for v in sp.log_softmax(xs)] for xs in logsoft_in]

constants = {
    "GAMMA_X": GAMMA_X, "GAMMA_Y": GAMMA_Y,
    "GAMMA_NF_X": GAMMA_NF_X, "GAMMA_NF_Y": GAMMA_NF_Y,
    "GAMMALN_X": GAMMALN_X, "GAMMALN_Y": GAMMALN_Y,
    "GAMMALN_NF_X": GAMMALN_NF_X, "GAMMALN_NF_Y": GAMMALN_NF_Y,
    "LOGGAMMA_X": LOGGAMMA_X, "LOGGAMMA_Y": LOGGAMMA_Y,
    "LOGGAMMA_NF_X": LOGGAMMA_NF_X, "LOGGAMMA_NF_Y": LOGGAMMA_NF_Y,
    "RGAMMA_X": RGAMMA_X, "RGAMMA_Y": RGAMMA_Y,
    "RGAMMA_NF_X": RGAMMA_NF_X, "RGAMMA_NF_Y": RGAMMA_NF_Y,
    "DIGAMMA_X": DIGAMMA_X, "DIGAMMA_Y": DIGAMMA_Y,
    "DIGAMMA_NF_X": DIGAMMA_NF_X, "DIGAMMA_NF_Y": DIGAMMA_NF_Y,
    "POLY_N": POLY_N, "POLY_X": POLY_X, "POLY_Y": POLY_Y,
    "POLY_NF_N": POLY_NF_N, "POLY_NF_X": POLY_NF_X, "POLY_NF_Y": POLY_NF_Y,
    "ZETA_X": ZETA_X, "ZETA_Y": ZETA_Y,
    "ZETA_NF_X": ZETA_NF_X, "ZETA_NF_Y": ZETA_NF_Y,
    "ZETAQ_X": ZETAQ_X, "ZETAQ_Q": ZETAQ_Q, "ZETAQ_Y": ZETAQ_Y,
    "ZETAQ_NF_X": ZETAQ_NF_X, "ZETAQ_NF_Q": ZETAQ_NF_Q, "ZETAQ_NF_Y": ZETAQ_NF_Y,
    "BETA_A": BETA_A, "BETA_B": BETA_B, "BETA_Y": BETA_Y,
    "BETA_NF_A": BETA_NF_A, "BETA_NF_B": BETA_NF_B, "BETA_NF_Y": BETA_NF_Y,
    "BETALN_A": BETALN_A, "BETALN_B": BETALN_B, "BETALN_Y": BETALN_Y,
    "BETALN_NF_A": BETALN_NF_A, "BETALN_NF_B": BETALN_NF_B, "BETALN_NF_Y": BETALN_NF_Y,
    "FACT_X": FACT_X, "FACT_Y": FACT_Y,
    "FACT_NF_X": FACT_NF_X, "FACT_NF_Y": FACT_NF_Y,
    "FACT_EX_N": fact_ex_n, "FACT_EX_Y": fact_ex_y,
    "COMB_N": COMB_N, "COMB_K": COMB_K, "COMB_Y": COMB_Y,
    "COMB_NF_N": COMB_NF_N, "COMB_NF_K": COMB_NF_K, "COMB_NF_Y": COMB_NF_Y,
    "COMB_EX_N": comb_ex_n, "COMB_EX_K": comb_ex_k, "COMB_EX_Y": comb_ex_y,
    "PERM_N": PERM_N, "PERM_K": PERM_K, "PERM_Y": PERM_Y,
    "PERM_NF_N": PERM_NF_N, "PERM_NF_K": PERM_NF_K, "PERM_NF_Y": PERM_NF_Y,
    "PERM_EX_N": perm_ex_n, "PERM_EX_K": perm_ex_k, "PERM_EX_Y": perm_ex_y,
    "BINOM_N": BINOM_N, "BINOM_K": BINOM_K, "BINOM_Y": BINOM_Y,
    "BINOM_NF_N": BINOM_NF_N, "BINOM_NF_K": BINOM_NF_K, "BINOM_NF_Y": BINOM_NF_Y,
    "POCH_A": POCH_A, "POCH_M": POCH_M, "POCH_Y": POCH_Y,
    "POCH_NF_A": POCH_NF_A, "POCH_NF_M": POCH_NF_M, "POCH_NF_Y": POCH_NF_Y,
    "EXPIT_X": EXPIT_X, "EXPIT_Y": EXPIT_Y,
    "EXPIT_NF_X": EXPIT_NF_X, "EXPIT_NF_Y": EXPIT_NF_Y,
    "LOG_EXPIT_X": LOG_EXPIT_X, "LOG_EXPIT_Y": LOG_EXPIT_Y,
    "LOG_EXPIT_NF_X": LOG_EXPIT_NF_X, "LOG_EXPIT_NF_Y": LOG_EXPIT_NF_Y,
    "LOGIT_X": LOGIT_X, "LOGIT_Y": LOGIT_Y,
    "LOGIT_NF_X": LOGIT_NF_X, "LOGIT_NF_Y": LOGIT_NF_Y,
    "XLOGY_X": XLOGY_X, "XLOGY_Y": XLOGY_Y, "XLOGY_Z": XLOGY_Z,
    "XLOGY_NF_X": XLOGY_NF_X, "XLOGY_NF_Y": XLOGY_NF_Y, "XLOGY_NF_Z": XLOGY_NF_Z,
    "XLOG1PY_X": XLOG1PY_X, "XLOG1PY_Y": XLOG1PY_Y, "XLOG1PY_Z": XLOG1PY_Z,
    "XLOG1PY_NF_X": XLOG1PY_NF_X, "XLOG1PY_NF_Y": XLOG1PY_NF_Y, "XLOG1PY_NF_Z": XLOG1PY_NF_Z,
    "ENTR_X": ENTR_X, "ENTR_Y": ENTR_Y,
    "ENTR_NF_X": ENTR_NF_X, "ENTR_NF_Y": ENTR_NF_Y,
    "REL_ENTR_X": REL_ENTR_X, "REL_ENTR_Y": REL_ENTR_Y, "REL_ENTR_Z": REL_ENTR_Z,
    "REL_ENTR_NF_X": REL_ENTR_NF_X, "REL_ENTR_NF_Y": REL_ENTR_NF_Y, "REL_ENTR_NF_Z": REL_ENTR_NF_Z,
    "KL_DIV_X": KL_DIV_X, "KL_DIV_Y": KL_DIV_Y, "KL_DIV_Z": KL_DIV_Z,
    "KL_DIV_NF_X": KL_DIV_NF_X, "KL_DIV_NF_Y": KL_DIV_NF_Y, "KL_DIV_NF_Z": KL_DIV_NF_Z,
    "HUBER_D": HUBER_D, "HUBER_R": HUBER_R, "HUBER_Y": HUBER_Y,
    "HUBER_NF_D": HUBER_NF_D, "HUBER_NF_R": HUBER_NF_R, "HUBER_NF_Y": HUBER_NF_Y,
    "PHUBER_D": PHUBER_D, "PHUBER_R": PHUBER_R, "PHUBER_Y": PHUBER_Y,
    "PHUBER_NF_D": PHUBER_NF_D, "PHUBER_NF_R": PHUBER_NF_R, "PHUBER_NF_Y": PHUBER_NF_Y,
    "BOXCOX_X": BOXCOX_X, "BOXCOX_L": BOXCOX_L, "BOXCOX_Y": BOXCOX_Y,
    "BOXCOX_NF_X": BOXCOX_NF_X, "BOXCOX_NF_L": BOXCOX_NF_L, "BOXCOX_NF_Y": BOXCOX_NF_Y,
    "IBOX_X": IBOX_X, "IBOX_L": IBOX_L, "IBOX_Y": IBOX_Y,
    "IBOX_NF_X": IBOX_NF_X, "IBOX_NF_L": IBOX_NF_L, "IBOX_NF_Y": IBOX_NF_Y,
    "LSE_IN": lse_in, "LSE_Y": lse_y,
    "LSE_NF_IN": lse_nf_in, "LSE_NF_Y": lse_nf_y,
    "LSEB_A": lseb_a, "LSEB_B": lseb_b, "LSEB_Y": lseb_y,
    "SOFT_IN": soft_in, "SOFT_Y": soft_y,
    "LOGSOFT_IN": logsoft_in, "LOGSOFT_Y": logsoft_y,
}

if __name__ == "__main__":
    # The shared writer is mid-edit (it now emits per-constant docstrings the
    # compiler does not lower yet), so emit the pre-landing format here using
    # the shared literal/non_finite/ROOT helpers. Switch back to `write(...)`
    # once notes.md says LANDED-WRITER.
    import platform
    provenance = ", ".join(f"{name} {version}" for name, version in sorted({"scipy": scipy.__version__, "numpy": numpy.__version__}.items()))
    lines = [
        "# osprey: flavor=python",
        "# osprey: namespace=scipy_expected",
        "# osprey: module=Expected",
        '"""scipy.special scalar-function values the @osprey/scipy port reproduces.',
        "",
        f"Generated by scripts/oracles/scipy.py with CPython {platform.python_version()} and {provenance}.",
        'Do not edit: rerun the oracle."""',
        "",
        "__all__ = [" + ", ".join(f'"{name}"' for name in constants) + "]",
        "",
    ]
    lines += [f"{name} = {literal(value, non_finite(value))}" for name, value in constants.items()]
    (ROOT / "packages" / "scipy" / "tests" / "expected.py").write_text("\n".join(lines) + "\n")
