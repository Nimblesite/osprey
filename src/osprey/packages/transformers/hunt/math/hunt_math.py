# osprey: flavor=python
"""Hunt driver: transformers' shared numerics — softmax, log_softmax, argmax
and multinomial — over a list mixing two symbolic logits with `Check.specials()`
(+inf, -inf, NaN, huge and subnormal), plus a symbolic uniform draw, so the
non-finite paths are explored on every run."""
from testing import Check
from transformers.Math import argmax, log_softmax, multinomial, softmax



def run():
    x0 = unwrap_or(parseFloat(input()), 0.0)
    x1 = unwrap_or(parseFloat(input()), 0.0)
    u = unwrap_or(parseFloat(input()), 0.5)
    logits = listConcat([x0, x1], Check.specials())
    probs = softmax(logits)
    logprobs = log_softmax(logits)
    best = argmax(logits)
    drawn = multinomial(probs, u)
    return print(len(probs) + len(logprobs) + best + drawn)


def main():
    return Check.lenient(run)
