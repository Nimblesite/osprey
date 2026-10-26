# osprey: flavor=python
"""Hunt driver: transformers' cheap float warpers — temperature (with a
symbolic temperature, so 0.0 reaches the divide-by-zero policy), min_p and
epsilon — over a list mixing symbolic logits with the concrete special values
NaN, +inf, -inf, huge and subnormal."""
from testing import Check
from transformers.Math import neg_inf
from transformers.LogitsProcess import (
    EpsilonLogitsWarper, MinPLogitsWarper, TemperatureLogitsWarper, apply,
)



def run():
    x0 = unwrap_or(parseFloat(input()), 0.0)
    x1 = unwrap_or(parseFloat(input()), 0.0)
    t = unwrap_or(parseFloat(input()), 1.0)
    logits = listConcat([x0, x1], Check.specials())
    a = apply(TemperatureLogitsWarper(t), [], logits)
    c = apply(MinPLogitsWarper(0.1, neg_inf(), 1), [], logits)
    e = apply(EpsilonLogitsWarper(0.1, neg_inf(), 1), [], logits)
    return print(len(a) + len(c) + len(e))


def main():
    return Check.lenient(run)
