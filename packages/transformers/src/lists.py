# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=transformers
# osprey: module=Lists
"""Small shared helpers over `list[int]` used by the logits processors and the
stopping criteria. They live here so `LogitsProcess` and `StoppingCriteria` do
not each carry a private copy. All are total: out-of-range reads fall back to a
sentinel instead of failing."""

from transformers.Math import min_i

__all__ = ["contains_int", "last_n"]


def contains_int(xs, x):
    """Whether `x` occurs anywhere in `xs`."""
    return fold(range(len(xs)), False, lambda found, i: found or unwrap_or(listGet(xs, i), x) == x)


def last_n(xs, n):
    """The last `n` elements of `xs` (all of them when n exceeds the length)."""
    k = min_i(n, len(xs))
    return toList(map(range(k), lambda i: unwrap_or(listGet(xs, len(xs) - k + i), 0)))

