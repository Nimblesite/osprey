# osprey: flavor=python
# osprey: namespace=scipy
# osprey: module=Logistic
"""scipy.special's logistic functions, the xsf log_exp.h formulas.

expit is 1/(1+exp(-x)); logit uses the two-sided log1p form near 0.5 for
accuracy; log_expit is x - log1p(exp(x)) below zero and -log1p(exp(-x)) above.
Poles and edges reproduce scipy: logit(1)=inf, logit outside (0,1) is NaN,
expit(+-inf) is 1 and 0. Values match scipy to ~1e-16 relative. Implements
[PORT-TEST-ORACLE].
"""
from scipy.Support import pos_inf, neg_inf, exp_s, log_s, log1p

__all__ = ["expit", "logit", "log_expit"]


def expit(x: float) -> float:
    """scipy.special.expit(x): the logistic sigmoid 1/(1+exp(-x))."""
    return 1.0 / (1.0 + exp_s(0.0 - x))


def logit(x: float) -> float:
    """scipy.special.logit(x): ln(x/(1-x)), the two-sided log1p form near 0.5."""
    if x < 0.3 or x > 0.65:
        if x == 1.0:
            return pos_inf()
        else:
            return log_s(x / (1.0 - x))
    else:
        s = 2.0 * (x - 0.5)
        return log1p(s) - log1p(0.0 - s)


def log_expit(x: float) -> float:
    """scipy.special.log_expit(x): ln(expit(x)), the xsf log_exp.h formula."""
    if x < 0.0:
        return x - log1p(exp_s(x))
    else:
        return 0.0 - log1p(exp_s(0.0 - x))
