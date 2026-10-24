# osprey: flavor=python
"""@osprey/scipy's suite: every ported `scipy.Special` function against values
the real scipy 1.18.1 computed (scripts/oracles/scipy.py).

The shared writer writes each constant that holds a NaN/inf with all its floats
as repr strings; those are decoded here with `Check.float_of`. Tolerances come
from a CPython prototype's measured worst relative error against scipy: the
Cephes rational paths agree to ~1e-13 (FMA-level spread), the gamma Stirling
reflection and beta to ~1e-12, and gammaln near its zeros at x=1,2 needs the
absolute term."""
from testing import Check
from scipy_expected import Expected
from scipy import Special

PURE_R = 1e-12
PURE_A = 0.0
LGAM_R = 1e-13
LGAM_A = 1e-13
ROOT_R = 1e-12
ROOT_A = 1e-12
LIST_R = 1e-12
LIST_A = 1e-12


def close1(name, fn, i, xs, ys, rtol, atol):
    return Check.close(f"{name}[{i}]", fn(unwrap_or(listGet(xs, i), 0.0)), unwrap_or(listGet(ys, i), 0.0), rtol, atol)


def scalar1(name, fn, xs, ys, rtol, atol):
    """One-argument scalar function over finite float lists."""
    return toList(map(range(len(ys)), lambda i: close1(name, fn, i, xs, ys, rtol, atol)))


def scalar1_nf(name, fn, xs, ys, rtol, atol):
    """One-argument scalar function; inputs/outputs decode from text."""
    return toList(map(range(len(ys)), lambda i: Check.close(f"{name}.nf[{i}]",
                                                             fn(Check.float_of(unwrap_or(listGet(xs, i), ""))),
                                                             Check.float_of(unwrap_or(listGet(ys, i), "")), rtol, atol)))


def scalar2(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function over finite float lists."""
    return toList(map(range(len(zs)), lambda i: Check.close(f"{name}[{i}]",
                                                             fn(unwrap_or(listGet(xs, i), 0.0), unwrap_or(listGet(ys, i), 0.0)),
                                                             unwrap_or(listGet(zs, i), 0.0), rtol, atol)))


def scalar2_nf(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function; inputs/outputs decode from text."""
    return toList(map(range(len(zs)), lambda i: Check.close(f"{name}.nf[{i}]",
                                                             fn(Check.float_of(unwrap_or(listGet(xs, i), "")), Check.float_of(unwrap_or(listGet(ys, i), ""))),
                                                             Check.float_of(unwrap_or(listGet(zs, i), "")), rtol, atol)))


def scalar2_nf_fin(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function: finite float inputs, text output."""
    return toList(map(range(len(zs)), lambda i: Check.close(f"{name}.nf[{i}]",
                                                             fn(unwrap_or(listGet(xs, i), 0.0), unwrap_or(listGet(ys, i), 0.0)),
                                                             Check.float_of(unwrap_or(listGet(zs, i), "")), rtol, atol)))


def ints1(name, fn, xs, ys):
    """One-argument int function, exact equality."""
    return toList(map(range(len(ys)), lambda i: Check.equal(f"{name}[{i}]", fn(unwrap_or(listGet(xs, i), 0)), unwrap_or(listGet(ys, i), 0))))


def ints2(name, fn, xs, ys, zs):
    """Two-argument int function, exact equality."""
    return toList(map(range(len(zs)), lambda i: Check.equal(f"{name}[{i}]", fn(unwrap_or(listGet(xs, i), 0), unwrap_or(listGet(ys, i), 0)), unwrap_or(listGet(zs, i), 0))))


def poly_cases(nf):
    if nf:
        return toList(map(range(len(Expected.POLY_NF_Y)), lambda i: Check.close(
            f"polygamma.nf[{i}]",
            Special.polygamma(unwrap_or(listGet(Expected.POLY_NF_N, i), 0), Check.float_of(unwrap_or(listGet(Expected.POLY_NF_X, i), ""))),
            Check.float_of(unwrap_or(listGet(Expected.POLY_NF_Y, i), "")), ROOT_R, ROOT_A)))
    else:
        return toList(map(range(len(Expected.POLY_Y)), lambda i: Check.close(
            f"polygamma[{i}]",
            Special.polygamma(unwrap_or(listGet(Expected.POLY_N, i), 0), unwrap_or(listGet(Expected.POLY_X, i), 0.0)),
            unwrap_or(listGet(Expected.POLY_Y, i), 0.0), ROOT_R, ROOT_A)))


def gamma_cases():
    return listConcat(scalar1("gamma", Special.gamma, Expected.GAMMA_X, Expected.GAMMA_Y, PURE_R, PURE_A),
                      scalar1_nf("gamma", Special.gamma, Expected.GAMMA_NF_X, Expected.GAMMA_NF_Y, PURE_R, PURE_A))


def gammaln_cases():
    return listConcat(scalar1("gammaln", Special.gammaln, Expected.GAMMALN_X, Expected.GAMMALN_Y, LGAM_R, LGAM_A),
                      scalar1_nf("gammaln", Special.gammaln, Expected.GAMMALN_NF_X, Expected.GAMMALN_NF_Y, LGAM_R, LGAM_A))


def loggamma_cases():
    return listConcat(scalar1("loggamma", Special.loggamma, Expected.LOGGAMMA_X, Expected.LOGGAMMA_Y, LGAM_R, LGAM_A),
                      scalar1_nf("loggamma", Special.loggamma, Expected.LOGGAMMA_NF_X, Expected.LOGGAMMA_NF_Y, LGAM_R, LGAM_A))


def rgamma_cases():
    return listConcat(scalar1("rgamma", Special.rgamma, Expected.RGAMMA_X, Expected.RGAMMA_Y, PURE_R, PURE_A),
                      scalar1_nf("rgamma", Special.rgamma, Expected.RGAMMA_NF_X, Expected.RGAMMA_NF_Y, PURE_R, PURE_A))


def digamma_cases():
    return listConcat(scalar1("digamma", Special.digamma, Expected.DIGAMMA_X, Expected.DIGAMMA_Y, ROOT_R, ROOT_A),
                      scalar1_nf("digamma", Special.digamma, Expected.DIGAMMA_NF_X, Expected.DIGAMMA_NF_Y, ROOT_R, ROOT_A))


def zeta_cases():
    return listConcat(scalar1("zeta", Special.zeta, Expected.ZETA_X, Expected.ZETA_Y, PURE_R, PURE_A),
                      scalar1_nf("zeta", Special.zeta, Expected.ZETA_NF_X, Expected.ZETA_NF_Y, PURE_R, PURE_A))


def zetaq_cases():
    return listConcat(scalar2("zeta_q", Special.zeta_q, Expected.ZETAQ_X, Expected.ZETAQ_Q, Expected.ZETAQ_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("zeta_q", Special.zeta_q, Expected.ZETAQ_NF_X, Expected.ZETAQ_NF_Q, Expected.ZETAQ_NF_Y, PURE_R, PURE_A))


def beta_cases():
    return listConcat(scalar2("beta", Special.beta, Expected.BETA_A, Expected.BETA_B, Expected.BETA_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("beta", Special.beta, Expected.BETA_NF_A, Expected.BETA_NF_B, Expected.BETA_NF_Y, PURE_R, PURE_A))


def betaln_cases():
    return listConcat(scalar2("betaln", Special.betaln, Expected.BETALN_A, Expected.BETALN_B, Expected.BETALN_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("betaln", Special.betaln, Expected.BETALN_NF_A, Expected.BETALN_NF_B, Expected.BETALN_NF_Y, PURE_R, PURE_A))


def factorial_cases():
    return listConcat(scalar1("factorial", Special.factorial, Expected.FACT_X, Expected.FACT_Y, PURE_R, PURE_A),
                      scalar1_nf("factorial", Special.factorial, Expected.FACT_NF_X, Expected.FACT_NF_Y, PURE_R, PURE_A))


def factorial_exact_cases():
    return ints1("factorial_exact", Special.factorial_exact, Expected.FACT_EX_N, Expected.FACT_EX_Y)


def comb_cases():
    return listConcat(scalar2("comb", Special.comb, Expected.COMB_N, Expected.COMB_K, Expected.COMB_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("comb", Special.comb, Expected.COMB_NF_N, Expected.COMB_NF_K, Expected.COMB_NF_Y, PURE_R, PURE_A))


def comb_exact_cases():
    return ints2("comb_exact", Special.comb_exact, Expected.COMB_EX_N, Expected.COMB_EX_K, Expected.COMB_EX_Y)


def perm_cases():
    return listConcat(scalar2("perm", Special.perm, Expected.PERM_N, Expected.PERM_K, Expected.PERM_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("perm", Special.perm, Expected.PERM_NF_N, Expected.PERM_NF_K, Expected.PERM_NF_Y, PURE_R, PURE_A))


def perm_exact_cases():
    return ints2("perm_exact", Special.perm_exact, Expected.PERM_EX_N, Expected.PERM_EX_K, Expected.PERM_EX_Y)


def binom_cases():
    return listConcat(scalar2("binom", Special.binom, Expected.BINOM_N, Expected.BINOM_K, Expected.BINOM_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("binom", Special.binom, Expected.BINOM_NF_N, Expected.BINOM_NF_K, Expected.BINOM_NF_Y, PURE_R, PURE_A))


def poch_cases():
    return listConcat(scalar2("poch", Special.poch, Expected.POCH_A, Expected.POCH_M, Expected.POCH_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("poch", Special.poch, Expected.POCH_NF_A, Expected.POCH_NF_M, Expected.POCH_NF_Y, PURE_R, PURE_A))


def expit_cases():
    return listConcat(scalar1("expit", Special.expit, Expected.EXPIT_X, Expected.EXPIT_Y, PURE_R, PURE_A),
                      scalar1_nf("expit", Special.expit, Expected.EXPIT_NF_X, Expected.EXPIT_NF_Y, PURE_R, PURE_A))


def log_expit_cases():
    return listConcat(scalar1("log_expit", Special.log_expit, Expected.LOG_EXPIT_X, Expected.LOG_EXPIT_Y, PURE_R, PURE_A),
                      scalar1_nf("log_expit", Special.log_expit, Expected.LOG_EXPIT_NF_X, Expected.LOG_EXPIT_NF_Y, PURE_R, PURE_A))


def logit_cases():
    return listConcat(scalar1("logit", Special.logit, Expected.LOGIT_X, Expected.LOGIT_Y, PURE_R, PURE_A),
                      scalar1_nf("logit", Special.logit, Expected.LOGIT_NF_X, Expected.LOGIT_NF_Y, PURE_R, PURE_A))


def xlogy_cases():
    return listConcat(scalar2("xlogy", Special.xlogy, Expected.XLOGY_X, Expected.XLOGY_Y, Expected.XLOGY_Z, PURE_R, PURE_A),
                      scalar2_nf("xlogy", Special.xlogy, Expected.XLOGY_NF_X, Expected.XLOGY_NF_Y, Expected.XLOGY_NF_Z, PURE_R, PURE_A))


def xlog1py_cases():
    return listConcat(scalar2("xlog1py", Special.xlog1py, Expected.XLOG1PY_X, Expected.XLOG1PY_Y, Expected.XLOG1PY_Z, PURE_R, PURE_A),
                      scalar2_nf("xlog1py", Special.xlog1py, Expected.XLOG1PY_NF_X, Expected.XLOG1PY_NF_Y, Expected.XLOG1PY_NF_Z, PURE_R, PURE_A))


def entr_cases():
    return listConcat(scalar1("entr", Special.entr, Expected.ENTR_X, Expected.ENTR_Y, PURE_R, PURE_A),
                      scalar1_nf("entr", Special.entr, Expected.ENTR_NF_X, Expected.ENTR_NF_Y, PURE_R, PURE_A))


def rel_entr_cases():
    return listConcat(scalar2("rel_entr", Special.rel_entr, Expected.REL_ENTR_X, Expected.REL_ENTR_Y, Expected.REL_ENTR_Z, PURE_R, PURE_A),
                      scalar2_nf("rel_entr", Special.rel_entr, Expected.REL_ENTR_NF_X, Expected.REL_ENTR_NF_Y, Expected.REL_ENTR_NF_Z, PURE_R, PURE_A))


def kl_div_cases():
    return listConcat(scalar2("kl_div", Special.kl_div, Expected.KL_DIV_X, Expected.KL_DIV_Y, Expected.KL_DIV_Z, PURE_R, PURE_A),
                      scalar2_nf("kl_div", Special.kl_div, Expected.KL_DIV_NF_X, Expected.KL_DIV_NF_Y, Expected.KL_DIV_NF_Z, PURE_R, PURE_A))


def huber_cases():
    return listConcat(scalar2("huber", Special.huber, Expected.HUBER_D, Expected.HUBER_R, Expected.HUBER_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("huber", Special.huber, Expected.HUBER_NF_D, Expected.HUBER_NF_R, Expected.HUBER_NF_Y, PURE_R, PURE_A))


def pseudo_huber_cases():
    return listConcat(scalar2("pseudo_huber", Special.pseudo_huber, Expected.PHUBER_D, Expected.PHUBER_R, Expected.PHUBER_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("pseudo_huber", Special.pseudo_huber, Expected.PHUBER_NF_D, Expected.PHUBER_NF_R, Expected.PHUBER_NF_Y, PURE_R, PURE_A))


def boxcox_cases():
    return listConcat(scalar2("boxcox", Special.boxcox, Expected.BOXCOX_X, Expected.BOXCOX_L, Expected.BOXCOX_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("boxcox", Special.boxcox, Expected.BOXCOX_NF_X, Expected.BOXCOX_NF_L, Expected.BOXCOX_NF_Y, PURE_R, PURE_A))


def inv_boxcox_cases():
    return listConcat(scalar2("inv_boxcox", Special.inv_boxcox, Expected.IBOX_X, Expected.IBOX_L, Expected.IBOX_Y, PURE_R, PURE_A),
                      scalar2_nf_fin("inv_boxcox", Special.inv_boxcox, Expected.IBOX_NF_X, Expected.IBOX_NF_L, Expected.IBOX_NF_Y, PURE_R, PURE_A))


def lse_cases():
    return listConcat(
        toList(map(range(len(Expected.LSE_Y)), lambda i: Check.close(f"logsumexp[{i}]",
                                                                     Special.logsumexp(unwrap_or(listGet(Expected.LSE_IN, i), [])),
                                                                     unwrap_or(listGet(Expected.LSE_Y, i), 0.0), PURE_R, PURE_A))),
        toList(map(range(len(Expected.LSE_NF_Y)), lambda i: Check.close(
            f"logsumexp.nf[{i}]",
            Special.logsumexp(mapList(unwrap_or(listGet(Expected.LSE_NF_IN, i), []), Check.float_of)),
            Check.float_of(unwrap_or(listGet(Expected.LSE_NF_Y, i), "")), PURE_R, PURE_A))))


def lse_b_cases():
    return toList(map(range(len(Expected.LSEB_Y)), lambda i: Check.close(
        f"logsumexp_b[{i}]",
        Special.logsumexp_b(unwrap_or(listGet(Expected.LSEB_A, i), []), unwrap_or(listGet(Expected.LSEB_B, i), [])),
        unwrap_or(listGet(Expected.LSEB_Y, i), 0.0), PURE_R, PURE_A)))


def softmax_cases():
    return toList(map(range(len(Expected.SOFT_Y)), lambda i: Check.close_all(
        f"softmax[{i}]", Special.softmax(unwrap_or(listGet(Expected.SOFT_IN, i), [])),
        unwrap_or(listGet(Expected.SOFT_Y, i), []), LIST_R, LIST_A)))


def log_softmax_cases():
    return toList(map(range(len(Expected.LOGSOFT_Y)), lambda i: Check.close_all(
        f"log_softmax[{i}]", Special.log_softmax(unwrap_or(listGet(Expected.LOGSOFT_IN, i), [])),
        unwrap_or(listGet(Expected.LOGSOFT_Y, i), []), LIST_R, LIST_A)))


def all_cases():
    groups = [gamma_cases(), gammaln_cases(), loggamma_cases(), rgamma_cases(),
              digamma_cases(), poly_cases(False), poly_cases(True), zeta_cases(),
              zetaq_cases(), beta_cases(), betaln_cases(), factorial_cases(),
              factorial_exact_cases(), comb_cases(), comb_exact_cases(),
              perm_cases(), perm_exact_cases(), binom_cases(), poch_cases(),
              expit_cases(), log_expit_cases(), logit_cases(), xlogy_cases(),
              xlog1py_cases(), entr_cases(), rel_entr_cases(), kl_div_cases(),
              huber_cases(), pseudo_huber_cases(), boxcox_cases(),
              inv_boxcox_cases(), lse_cases(), lse_b_cases(), softmax_cases(),
              log_softmax_cases()]
    return foldList(groups, [], lambda acc, g: listConcat(acc, g))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
