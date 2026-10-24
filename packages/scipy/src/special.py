# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=scipy
# osprey: module=Special
"""scipy.special's scalar functions, re-exported under one module.

The port is split across `scipy.Gamma`, `scipy.Zeta`, `scipy.Beta`,
`scipy.Combinatorics`, `scipy.Logistic`, `scipy.Information` and `scipy.Boxcox`
so each file stays small; this facade re-declares every public function under
the single `scipy.Special` name scipy.special presents. Each wrapper delegates
to the module that owns the algorithm. Implements [PORT-TEST-ORACLE].
"""
from scipy import Boxcox, Beta, Combinatorics, Gamma, Information, Logistic, Zeta

__all__ = ["gamma", "gammaln", "loggamma", "digamma", "polygamma", "rgamma",
           "psi", "zeta", "zeta_q", "zetac", "beta", "betaln", "lbeta",
           "factorial", "factorial_exact", "comb", "comb_exact", "perm",
           "perm_exact", "binom", "poch", "expit", "logit", "log_expit",
           "xlogy", "xlog1py", "entr", "rel_entr", "kl_div", "huber",
           "pseudo_huber", "logsumexp", "logsumexp_b", "softmax",
           "log_softmax", "boxcox", "inv_boxcox"]


def gamma(x: float) -> float:
    """scipy.special.gamma(x)."""
    return Gamma.gamma(x)


def gammaln(x: float) -> float:
    """scipy.special.gammaln(x)."""
    return Gamma.gammaln(x)


def loggamma(x: float) -> float:
    """scipy.special.loggamma(x)."""
    return Gamma.loggamma(x)


def digamma(x: float) -> float:
    """scipy.special.digamma(x)."""
    return Gamma.digamma(x)


def polygamma(n: int, x: float) -> float:
    """scipy.special.polygamma(n, x)."""
    return Gamma.polygamma(n, x)


def rgamma(x: float) -> float:
    """scipy.special.rgamma(x)."""
    return Gamma.rgamma(x)


def psi(x: float) -> float:
    """scipy.special.psi(x)."""
    return Gamma.psi(x)


def zeta(x: float) -> float:
    """scipy.special.zeta(x)."""
    return Zeta.zeta(x)


def zeta_q(x: float, q: float) -> float:
    """scipy.special.zeta(x, q)."""
    return Zeta.zeta_q(x, q)


def zetac(x: float) -> float:
    """scipy.special.zetac(x)."""
    return Zeta.zetac(x)


def beta(a: float, b: float) -> float:
    """scipy.special.beta(a, b)."""
    return Beta.beta(a, b)


def betaln(a: float, b: float) -> float:
    """scipy.special.betaln(a, b)."""
    return Beta.betaln(a, b)


def lbeta(a: float, b: float) -> float:
    """scipy.special.betaln(a, b), the Cephes log-beta."""
    return Beta.lbeta(a, b)


def factorial(x: float) -> float:
    """scipy.special.factorial(n, exact=False)."""
    return Combinatorics.factorial(x)


def factorial_exact(n: int) -> int:
    """scipy.special.factorial(n, exact=True) as a 64-bit int."""
    return Combinatorics.factorial_exact(n)


def comb(n: float, k: float) -> float:
    """scipy.special.comb(N, k, exact=False)."""
    return Combinatorics.comb(n, k)


def comb_exact(n: int, k: int) -> int:
    """scipy.special.comb(N, k, exact=True) as a 64-bit int."""
    return Combinatorics.comb_exact(n, k)


def perm(n: float, k: float) -> float:
    """scipy.special.perm(N, k, exact=False)."""
    return Combinatorics.perm(n, k)


def perm_exact(n: int, k: int) -> int:
    """scipy.special.perm(N, k, exact=True) as a 64-bit int."""
    return Combinatorics.perm_exact(n, k)


def binom(n: float, k: float) -> float:
    """scipy.special.binom(n, k)."""
    return Combinatorics.binom(n, k)


def poch(a: float, m: float) -> float:
    """scipy.special.poch(a, m)."""
    return Combinatorics.poch(a, m)


def expit(x: float) -> float:
    """scipy.special.expit(x)."""
    return Logistic.expit(x)


def logit(x: float) -> float:
    """scipy.special.logit(x)."""
    return Logistic.logit(x)


def log_expit(x: float) -> float:
    """scipy.special.log_expit(x)."""
    return Logistic.log_expit(x)


def xlogy(x: float, y: float) -> float:
    """scipy.special.xlogy(x, y)."""
    return Information.xlogy(x, y)


def xlog1py(x: float, y: float) -> float:
    """scipy.special.xlog1py(x, y)."""
    return Information.xlog1py(x, y)


def entr(x: float) -> float:
    """scipy.special.entr(x)."""
    return Information.entr(x)


def rel_entr(x: float, y: float) -> float:
    """scipy.special.rel_entr(x, y)."""
    return Information.rel_entr(x, y)


def kl_div(x: float, y: float) -> float:
    """scipy.special.kl_div(x, y)."""
    return Information.kl_div(x, y)


def huber(delta: float, r: float) -> float:
    """scipy.special.huber(delta, r)."""
    return Information.huber(delta, r)


def pseudo_huber(delta: float, r: float) -> float:
    """scipy.special.pseudo_huber(delta, r)."""
    return Information.pseudo_huber(delta, r)


def logsumexp(a) -> float:
    """scipy.special.logsumexp(a)."""
    return Information.logsumexp(a)


def logsumexp_b(a, b) -> float:
    """scipy.special.logsumexp(a, b=b)."""
    return Information.logsumexp_b(a, b)


def softmax(xs) -> list[float]:
    """scipy.special.softmax(x)."""
    return Information.softmax(xs)


def log_softmax(xs) -> list[float]:
    """scipy.special.log_softmax(x)."""
    return Information.log_softmax(xs)


def boxcox(x: float, lmbda: float) -> float:
    """scipy.special.boxcox(x, lmbda)."""
    return Boxcox.boxcox(x, lmbda)


def inv_boxcox(x: float, lmbda: float) -> float:
    """scipy.special.inv_boxcox(x, lmbda)."""
    return Boxcox.inv_boxcox(x, lmbda)
