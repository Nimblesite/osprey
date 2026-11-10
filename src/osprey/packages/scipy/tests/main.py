# osprey: flavor=python
"""@osprey/scipy's suite: every ported `scipy.Special` function against values
the real scipy 1.18.1 computed (src/osprey/packages/scipy/oracle/scipy.cpython).

The shared writer writes each constant that holds a NaN/inf with all its floats
as repr strings; those are decoded here with `Check.float_of`. Tolerances come
from a CPython prototype's measured worst relative error against scipy: the
Cephes rational paths agree to ~1e-13 (FMA-level spread), the gamma Stirling
reflection and beta to ~1e-12, and gammaln near its zeros at x=1,2 needs the
absolute term."""
from testing import Check
from scipy_expected import Expected
from scipy import Beta, Gamma, Special, Support, Zeta

PURE_R = 1e-12
PURE_A = 0.0
LGAM_R = 1e-13
LGAM_A = 1e-13
ROOT_R = 1e-12
ROOT_A = 1e-12
LIST_R = 1e-12
LIST_A = 1e-12


def at(xs, i, default):
    """Element `i` of a list, or `default` when it has no such element."""
    return unwrap_or(listGet(xs, i), default)


def decoded(xs):
    """A column whose NaN/inf arrive as repr strings, decoded to floats."""
    return mapList(xs, Check.float_of)


def over(name, wants, step):
    """Every case of a table: `step` names and checks case `i` of `wants`."""
    return toList(map(range(len(wants)), lambda i: step(f"{name}[{i}]", i)))


def concat_cases(groups):
    """Every group of cases, in the order the suite reports them."""
    return foldList(groups, [], listConcat)


def scalar1(name, fn, xs, ys, rtol, atol):
    """One-argument scalar function over finite float lists."""
    return over(name, ys, lambda tag, i: Check.close(tag, fn(at(xs, i, 0.0)), at(ys, i, 0.0), rtol, atol))


def scalar1_nf(name, fn, xs, ys, rtol, atol):
    """One-argument scalar function; inputs and outputs decoded from text."""
    return scalar1(name + ".nf", fn, decoded(xs), decoded(ys), rtol, atol)


def scalar2(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function over finite float lists."""
    return over(name, zs, lambda tag, i: Check.close(tag, fn(at(xs, i, 0.0), at(ys, i, 0.0)), at(zs, i, 0.0), rtol, atol))


def scalar2_nf(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function; both operands and the result decoded from text."""
    return scalar2(name + ".nf", fn, decoded(xs), decoded(ys), decoded(zs), rtol, atol)


def scalar2_nf_fin(name, fn, xs, ys, zs, rtol, atol):
    """Two-argument scalar function: finite operands, text-decoded result."""
    return scalar2(name + ".nf", fn, xs, ys, decoded(zs), rtol, atol)


def scalar1_both(name, fn, xs, ys, nfx, nfy, rtol, atol):
    """A one-argument scalar function: the finite cases, then the text-decoded ones."""
    return listConcat(scalar1(name, fn, xs, ys, rtol, atol),
                      scalar1_nf(name, fn, nfx, nfy, rtol, atol))


def scalar2_both(runner, name, fn, xs, ys, zs, nfx, nfy, nfz, rtol, atol):
    """A two-argument scalar function: its finite cases, then its text-decoded ones."""
    return listConcat(scalar2(name, fn, xs, ys, zs, rtol, atol),
                      runner(name, fn, nfx, nfy, nfz, rtol, atol))


def lists1(name, fn, xs, ys, rtol, atol):
    """A list-to-scalar function, one check per case."""
    return over(name, ys, lambda tag, i: Check.close(tag, fn(at(xs, i, [])), at(ys, i, 0.0), rtol, atol))


def lists2(name, fn, xs, ys, zs, rtol, atol):
    """A two-list-to-scalar function, one check per case."""
    return over(name, zs, lambda tag, i: Check.close(tag, fn(at(xs, i, []), at(ys, i, [])), at(zs, i, 0.0), rtol, atol))


def list_cases(name, fn, xs, ys):
    """A list-to-list function, compared elementwise with the list tolerances."""
    return over(name, ys, lambda tag, i: Check.close_all(tag, fn(at(xs, i, [])), at(ys, i, []), LIST_R, LIST_A))


def ints1(name, fn, xs, ys):
    """One-argument int function, exact equality."""
    return over(name, ys, lambda tag, i: Check.equal(tag, fn(at(xs, i, 0)), at(ys, i, 0)))


def ints2(name, fn, xs, ys, zs):
    """Two-argument int function, exact equality."""
    return over(name, zs, lambda tag, i: Check.equal(tag, fn(at(xs, i, 0), at(ys, i, 0)), at(zs, i, 0)))


def poly_cases(nf):
    """polygamma: finite floats, or its x operand and result decoded from text."""
    if nf:
        return over("polygamma.nf", Expected.POLY_NF_Y, lambda tag, i: Check.close(
            tag,
            Special.polygamma(at(Expected.POLY_NF_N, i, 0), Check.float_of(at(Expected.POLY_NF_X, i, ""))),
            Check.float_of(at(Expected.POLY_NF_Y, i, "")), ROOT_R, ROOT_A))
    else:
        return over("polygamma", Expected.POLY_Y, lambda tag, i: Check.close(
            tag,
            Special.polygamma(at(Expected.POLY_N, i, 0), at(Expected.POLY_X, i, 0.0)),
            at(Expected.POLY_Y, i, 0.0), ROOT_R, ROOT_A))


def special_cases():
    """The gamma and digamma family, each named as the suite reports it."""
    return concat_cases([
        scalar1_both("gamma", Special.gamma, Expected.GAMMA_X, Expected.GAMMA_Y, Expected.GAMMA_NF_X, Expected.GAMMA_NF_Y, PURE_R, PURE_A),
        scalar1_both("gammaln", Special.gammaln, Expected.GAMMALN_X, Expected.GAMMALN_Y, Expected.GAMMALN_NF_X, Expected.GAMMALN_NF_Y, LGAM_R, LGAM_A),
        scalar1_both("loggamma", Special.loggamma, Expected.LOGGAMMA_X, Expected.LOGGAMMA_Y, Expected.LOGGAMMA_NF_X, Expected.LOGGAMMA_NF_Y, LGAM_R, LGAM_A),
        scalar1_both("rgamma", Special.rgamma, Expected.RGAMMA_X, Expected.RGAMMA_Y, Expected.RGAMMA_NF_X, Expected.RGAMMA_NF_Y, PURE_R, PURE_A),
        scalar1_both("digamma", Special.digamma, Expected.DIGAMMA_X, Expected.DIGAMMA_Y, Expected.DIGAMMA_NF_X, Expected.DIGAMMA_NF_Y, ROOT_R, ROOT_A),
    ])


def analytic_cases():
    """The analytic, logistic, information and loss functions, in report order."""
    return concat_cases([
        scalar1_both("zeta", Special.zeta, Expected.ZETA_X, Expected.ZETA_Y, Expected.ZETA_NF_X, Expected.ZETA_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "zeta_q", Special.zeta_q, Expected.ZETAQ_X, Expected.ZETAQ_Q, Expected.ZETAQ_Y, Expected.ZETAQ_NF_X, Expected.ZETAQ_NF_Q, Expected.ZETAQ_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "beta", Special.beta, Expected.BETA_A, Expected.BETA_B, Expected.BETA_Y, Expected.BETA_NF_A, Expected.BETA_NF_B, Expected.BETA_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "betaln", Special.betaln, Expected.BETALN_A, Expected.BETALN_B, Expected.BETALN_Y, Expected.BETALN_NF_A, Expected.BETALN_NF_B, Expected.BETALN_NF_Y, PURE_R, PURE_A),
        scalar1_both("factorial", Special.factorial, Expected.FACT_X, Expected.FACT_Y, Expected.FACT_NF_X, Expected.FACT_NF_Y, PURE_R, PURE_A),
        ints1("factorial_exact", Special.factorial_exact, Expected.FACT_EX_N, Expected.FACT_EX_Y),
        scalar2_both(scalar2_nf_fin, "comb", Special.comb, Expected.COMB_N, Expected.COMB_K, Expected.COMB_Y, Expected.COMB_NF_N, Expected.COMB_NF_K, Expected.COMB_NF_Y, PURE_R, PURE_A),
        ints2("comb_exact", Special.comb_exact, Expected.COMB_EX_N, Expected.COMB_EX_K, Expected.COMB_EX_Y),
        scalar2_both(scalar2_nf_fin, "perm", Special.perm, Expected.PERM_N, Expected.PERM_K, Expected.PERM_Y, Expected.PERM_NF_N, Expected.PERM_NF_K, Expected.PERM_NF_Y, PURE_R, PURE_A),
        ints2("perm_exact", Special.perm_exact, Expected.PERM_EX_N, Expected.PERM_EX_K, Expected.PERM_EX_Y),
        scalar2_both(scalar2_nf_fin, "binom", Special.binom, Expected.BINOM_N, Expected.BINOM_K, Expected.BINOM_Y, Expected.BINOM_NF_N, Expected.BINOM_NF_K, Expected.BINOM_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "poch", Special.poch, Expected.POCH_A, Expected.POCH_M, Expected.POCH_Y, Expected.POCH_NF_A, Expected.POCH_NF_M, Expected.POCH_NF_Y, PURE_R, PURE_A),
        scalar1_both("expit", Special.expit, Expected.EXPIT_X, Expected.EXPIT_Y, Expected.EXPIT_NF_X, Expected.EXPIT_NF_Y, PURE_R, PURE_A),
        scalar1_both("log_expit", Special.log_expit, Expected.LOG_EXPIT_X, Expected.LOG_EXPIT_Y, Expected.LOG_EXPIT_NF_X, Expected.LOG_EXPIT_NF_Y, PURE_R, PURE_A),
        scalar1_both("logit", Special.logit, Expected.LOGIT_X, Expected.LOGIT_Y, Expected.LOGIT_NF_X, Expected.LOGIT_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf, "xlogy", Special.xlogy, Expected.XLOGY_X, Expected.XLOGY_Y, Expected.XLOGY_Z, Expected.XLOGY_NF_X, Expected.XLOGY_NF_Y, Expected.XLOGY_NF_Z, PURE_R, PURE_A),
        scalar2_both(scalar2_nf, "xlog1py", Special.xlog1py, Expected.XLOG1PY_X, Expected.XLOG1PY_Y, Expected.XLOG1PY_Z, Expected.XLOG1PY_NF_X, Expected.XLOG1PY_NF_Y, Expected.XLOG1PY_NF_Z, PURE_R, PURE_A),
        scalar1_both("entr", Special.entr, Expected.ENTR_X, Expected.ENTR_Y, Expected.ENTR_NF_X, Expected.ENTR_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf, "rel_entr", Special.rel_entr, Expected.REL_ENTR_X, Expected.REL_ENTR_Y, Expected.REL_ENTR_Z, Expected.REL_ENTR_NF_X, Expected.REL_ENTR_NF_Y, Expected.REL_ENTR_NF_Z, PURE_R, PURE_A),
        scalar2_both(scalar2_nf, "kl_div", Special.kl_div, Expected.KL_DIV_X, Expected.KL_DIV_Y, Expected.KL_DIV_Z, Expected.KL_DIV_NF_X, Expected.KL_DIV_NF_Y, Expected.KL_DIV_NF_Z, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "huber", Special.huber, Expected.HUBER_D, Expected.HUBER_R, Expected.HUBER_Y, Expected.HUBER_NF_D, Expected.HUBER_NF_R, Expected.HUBER_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "pseudo_huber", Special.pseudo_huber, Expected.PHUBER_D, Expected.PHUBER_R, Expected.PHUBER_Y, Expected.PHUBER_NF_D, Expected.PHUBER_NF_R, Expected.PHUBER_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "boxcox", Special.boxcox, Expected.BOXCOX_X, Expected.BOXCOX_L, Expected.BOXCOX_Y, Expected.BOXCOX_NF_X, Expected.BOXCOX_NF_L, Expected.BOXCOX_NF_Y, PURE_R, PURE_A),
        scalar2_both(scalar2_nf_fin, "inv_boxcox", Special.inv_boxcox, Expected.IBOX_X, Expected.IBOX_L, Expected.IBOX_Y, Expected.IBOX_NF_X, Expected.IBOX_NF_L, Expected.IBOX_NF_Y, PURE_R, PURE_A),
    ])


def vector_cases():
    """logsumexp and its B twin, then the two softmax transforms."""
    return concat_cases([
        lists1("logsumexp", Special.logsumexp, Expected.LSE_IN, Expected.LSE_Y, PURE_R, PURE_A),
        lists1("logsumexp.nf", Special.logsumexp, mapList(Expected.LSE_NF_IN, decoded), decoded(Expected.LSE_NF_Y), PURE_R, PURE_A),
        lists2("logsumexp_b", Special.logsumexp_b, Expected.LSEB_A, Expected.LSEB_B, Expected.LSEB_Y, PURE_R, PURE_A),
        list_cases("softmax", Special.softmax, Expected.SOFT_IN, Expected.SOFT_Y),
        list_cases("log_softmax", Special.log_softmax, Expected.LOGSOFT_IN, Expected.LOGSOFT_Y),
    ])


def helper_cases():
    """The support constants, non-finite helpers, arithmetic, transcendentals and aliases."""
    signed = Gamma.lgam_sgn(-0.5)
    want = Support.Signed(value=1.2655121234846454, sign=-1.0)
    return [
        Check.equal("support.MACHEP", Support.MACHEP, 1.1102230246251565e-16),
        Check.close("support.PI", Support.PI, 3.141592653589793, PURE_R, PURE_A),
        Check.close("support.LOGPI_CEPHES", Support.LOGPI_CEPHES, ln(Support.PI), PURE_R, PURE_A),
        Check.close("support.SQRT2PI", Support.SQRT2PI, sqrt(2.0 * Support.PI), PURE_R, PURE_A),
        Check.close("support.LS2PI", Support.LS2PI, ln(2.0 * Support.PI) * 0.5, PURE_R, PURE_A),
        Check.equal("support.MAXGAM", Support.MAXGAM, 171.624376956302725),
        Check.equal("support.MAXLOG", Support.MAXLOG, 709.7827128933839730962063185871),
        Check.equal("support.MAXLGM", Support.MAXLGM, 2.556348e305),
        Check.equal("support.MAXSTIR", Support.MAXSTIR, 143.01608),
        Check.equal("support.BETA_ASYMP_FACTOR", Support.BETA_ASYMP_FACTOR, 1000000.0),
        Check.close("support.SCIPY_EULER", Support.SCIPY_EULER, 0.5772156649015329, PURE_R, PURE_A),
        Check.close("support.EULER16", Support.EULER16, 0.5772156649015329, PURE_R, PURE_A),
        Check.equal("support.nan", Support.is_nan(Support.nan()), True),
        Check.equal("support.pos_inf", Support.is_inf(Support.pos_inf()), True),
        Check.equal("support.neg_inf", Support.neg_inf() < 0.0 and Support.is_inf(Support.neg_inf()), True),
        Check.equal("support.is_neg_zero", Support.is_neg_zero(Check.float_of("-0.0")), True),
        Check.equal("support.positive_zero", Support.is_neg_zero(0.0), False),
        Check.equal("support.exp_s.overflow", Support.exp_s(1000.0), Support.pos_inf()),
        Check.equal("support.log_s.zero", Support.log_s(0.0), Support.neg_inf()),
        Check.equal("support.log_s.domain", Support.is_nan(Support.log_s(-1.0)), True),
        Check.equal("support.floor", Support.floor(-1.25), -2.0),
        Check.equal("support.sign_f", Support.sign_f(-3.0), -1.0),
        Check.close("support.pow_pos", Support.pow_pos(4.0, 0.5), 2.0, PURE_R, PURE_A),
        Check.close("support.pow_int_signed", Support.pow_int_signed(-2.0, 3.0), -8.0, PURE_R, PURE_A),
        Check.equal("support.even_f", Support.even_f(4.0) and not Support.even_f(3.0), True),
        Check.equal("support.float_to_int", Support.float_to_int(7.0), 7),
        Check.equal("support.polevl", Support.polevl(2.0, [2.0, 3.0, 4.0]), 18.0),
        Check.equal("support.p1evl", Support.p1evl(2.0, [3.0, 4.0]), 14.0),
        Check.equal("support.chbevl", Support.chbevl(2.0, [2.0, 3.0], 2), 3.5),
        Check.equal("support.list_max", Support.list_max([-1.0, 2.0, 1.0]), 2.0),
        Check.close("support.sinpi", Support.sinpi(0.5), 1.0, PURE_R, PURE_A),
        Check.close("support.cot_pi", Support.cot_pi(-0.25), -1.0, PURE_R, PURE_A),
        Check.close("support.log1p", Support.log1p(0.25), 0.22314355131420976, PURE_R, PURE_A),
        Check.close("support.expm1", Support.expm1(ln(2.0)), 1.0, PURE_R, PURE_A),
        Check.close("gamma.lgam_sgn.value", signed.value, want.value, PURE_R, PURE_A),
        Check.equal("gamma.lgam_sgn.sign", signed.sign, want.sign),
        Check.close("gamma.psi", Gamma.psi(1.0), -0.5772156649015329, PURE_R, PURE_A),
        Check.close("special.psi", Special.psi(1.0), -0.5772156649015329, PURE_R, PURE_A),
        Check.close("zeta.zetac", Zeta.zetac(2.0), 0.6449340668482264, PURE_R, PURE_A),
        Check.close("special.zetac", Special.zetac(2.0), 0.6449340668482264, PURE_R, PURE_A),
        Check.close("beta.lbeta", Beta.lbeta(2.0, 3.0), -2.4849066497880004, PURE_R, PURE_A),
        Check.close("special.lbeta", Special.lbeta(2.0, 3.0), -2.4849066497880004, PURE_R, PURE_A),
    ]


def all_cases():
    return concat_cases([special_cases(), poly_cases(False), poly_cases(True), analytic_cases(),
                         vector_cases(), helper_cases()])


def main():
    return Check.run(all_cases)
