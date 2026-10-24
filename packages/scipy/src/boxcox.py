# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Boxcox
"""scipy.special's Box-Cox transforms, the xsf boxcox.h formulas.

boxcox(x, lmbda) is log(x) when |lmbda| < 1e-19, expm1(lmbda*log(x))/lmbda
while the exponent stays below 709.78, and the sign-split exp form above it;
inv_boxcox inverts it. Values match scipy exactly on the measured grid.
Implements [PORT-TEST-ORACLE].
"""
from scipy.Support import exp_s, log_s, log1p, expm1

__all__ = ["boxcox", "inv_boxcox"]


def boxcox(x: float, lmbda: float) -> float:
    """scipy.special.boxcox(x, lmbda): the one-parameter Box-Cox transform."""
    if abs(lmbda) < 1.0e-19:
        return log_s(x)
    elif lmbda * log_s(x) < 709.78:
        return expm1(lmbda * log_s(x)) / lmbda
    else:
        sgn = 1.0 if lmbda > 0.0 else -1.0
        return sgn * exp_s(lmbda * log_s(x) - log_s(abs(lmbda))) - 1.0 / lmbda


def inv_boxcox(x: float, lmbda: float) -> float:
    """scipy.special.inv_boxcox(x, lmbda): the inverse Box-Cox transform."""
    if lmbda == 0.0:
        return exp_s(x)
    elif lmbda * x < 1.79e308:
        return exp_s(log1p(lmbda * x) / lmbda)
    else:
        sgn = 1.0 if lmbda > 0.0 else -1.0
        return exp_s((log_s(sgn * (x + 1.0 / lmbda)) + log_s(abs(lmbda))) / lmbda)
