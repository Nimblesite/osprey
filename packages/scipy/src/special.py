# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Special
"""scipy.special's scalar functions under one module.

The algorithms live in Gamma, Zeta, Beta, Combinatorics, Logistic, Information
and Boxcox. This facade exports their original callable values, preserving
signatures and call-time effects. Implements [PORT-TEST-ORACLE], [MODULES-INIT].
"""
from scipy import Boxcox, Beta, Combinatorics, Gamma, Information, Logistic, Zeta

__all__ = ["gamma", "gammaln", "loggamma", "digamma", "polygamma", "rgamma",
           "psi", "zeta", "zeta_q", "zetac", "beta", "betaln", "lbeta",
           "factorial", "factorial_exact", "comb", "comb_exact", "perm",
           "perm_exact", "binom", "poch", "expit", "logit", "log_expit",
           "xlogy", "xlog1py", "entr", "rel_entr", "kl_div", "huber",
           "pseudo_huber", "logsumexp", "logsumexp_b", "softmax",
           "log_softmax", "boxcox", "inv_boxcox"]

gamma = Gamma.gamma
"""scipy.special.gamma(x)."""

gammaln = Gamma.gammaln
"""scipy.special.gammaln(x)."""

loggamma = Gamma.loggamma
"""scipy.special.loggamma(x)."""

digamma = Gamma.digamma
"""scipy.special.digamma(x)."""

polygamma = Gamma.polygamma
"""scipy.special.polygamma(n, x)."""

rgamma = Gamma.rgamma
"""scipy.special.rgamma(x)."""

psi = Gamma.psi
"""scipy.special.psi(x)."""

zeta = Zeta.zeta
"""scipy.special.zeta(x)."""

zeta_q = Zeta.zeta_q
"""scipy.special.zeta(x, q)."""

zetac = Zeta.zetac
"""scipy.special.zetac(x)."""

beta = Beta.beta
"""scipy.special.beta(a, b)."""

betaln = Beta.betaln
"""scipy.special.betaln(a, b)."""

lbeta = Beta.lbeta
"""scipy.special.betaln(a, b), the Cephes log-beta."""

factorial = Combinatorics.factorial
"""scipy.special.factorial(n, exact=False)."""

factorial_exact = Combinatorics.factorial_exact
"""scipy.special.factorial(n, exact=True) as a 64-bit int."""

comb = Combinatorics.comb
"""scipy.special.comb(N, k, exact=False)."""

comb_exact = Combinatorics.comb_exact
"""scipy.special.comb(N, k, exact=True) as a 64-bit int."""

perm = Combinatorics.perm
"""scipy.special.perm(N, k, exact=False)."""

perm_exact = Combinatorics.perm_exact
"""scipy.special.perm(N, k, exact=True) as a 64-bit int."""

binom = Combinatorics.binom
"""scipy.special.binom(n, k)."""

poch = Combinatorics.poch
"""scipy.special.poch(a, m)."""

expit = Logistic.expit
"""scipy.special.expit(x)."""

logit = Logistic.logit
"""scipy.special.logit(x)."""

log_expit = Logistic.log_expit
"""scipy.special.log_expit(x)."""

xlogy = Information.xlogy
"""scipy.special.xlogy(x, y)."""

xlog1py = Information.xlog1py
"""scipy.special.xlog1py(x, y)."""

entr = Information.entr
"""scipy.special.entr(x)."""

rel_entr = Information.rel_entr
"""scipy.special.rel_entr(x, y)."""

kl_div = Information.kl_div
"""scipy.special.kl_div(x, y)."""

huber = Information.huber
"""scipy.special.huber(delta, r)."""

pseudo_huber = Information.pseudo_huber
"""scipy.special.pseudo_huber(delta, r)."""

logsumexp = Information.logsumexp
"""scipy.special.logsumexp(a)."""

logsumexp_b = Information.logsumexp_b
"""scipy.special.logsumexp(a, b=b)."""

softmax = Information.softmax
"""scipy.special.softmax(x)."""

log_softmax = Information.log_softmax
"""scipy.special.log_softmax(x)."""

boxcox = Boxcox.boxcox
"""scipy.special.boxcox(x, lmbda)."""

inv_boxcox = Boxcox.inv_boxcox
"""scipy.special.inv_boxcox(x, lmbda)."""
