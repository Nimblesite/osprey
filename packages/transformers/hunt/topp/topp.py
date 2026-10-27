# osprey: flavor=python
"""Hunt driver: transformers' sort-and-cumsum warpers — top_p (with a symbolic
top_p), typical and eta — over a list mixing symbolic logits with the concrete
special values NaN, +inf, -inf, huge and subnormal."""
from testing import Check
from transformers.Math import neg_inf
from transformers.LogitsProcess import (
    EtaLogitsWarper, TopPLogitsWarper, TypicalLogitsWarper, apply,
)



def run():
    x0 = unwrap_or(parseFloat(input()), 0.0)
    x1 = unwrap_or(parseFloat(input()), 0.0)
    p = unwrap_or(parseFloat(input()), 0.9)
    logits = listConcat([x0, x1], Check.specials())
    b = apply(TopPLogitsWarper(p, neg_inf(), 1), [], logits)
    d = apply(TypicalLogitsWarper(0.9, neg_inf(), 1), [], logits)
    f = apply(EtaLogitsWarper(0.1, neg_inf(), 1), [], logits)
    return print(len(b) + len(d) + len(f))


def main():
    return Check.lenient(run)
