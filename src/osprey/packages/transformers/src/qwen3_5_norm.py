# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Norm
"""`Qwen3_5RMSNorm`: the zero-centered RMS norm every `qwen3_5` block normalizes with.

`Qwen3_5RMSNorm.forward` divides each row by the root mean square of its own entries
plus `eps`, then scales by `1 + weight`: the weight is initialized to zeros, so a fresh
checkpoint multiplies by one and a trained one moves it away from zero. Both the mean of
squares and the scaling are binary32 and the sum runs left to right, matching the
port's other reductions. `rms_rows` applies one weight to every row of a batch.
Implements [LLM-MODEL-ATTENTION]'s norm support, [PORT-TEST-ORACLE].
"""

__all__ = ["rms_norm", "rms_rows", "rms_denominator", "mean_square"]

from transformers.Qwen3_5Tensor import at


def rms_norm(x, weight, eps):
    """`(x / sqrt(mean(x²) + eps)) * (1 + weight)` for one row."""
    denominator = rms_denominator(x, eps)
    return toList(map(range(len(x)), lambda i: (at(x, i) / denominator) * (toFloat32(1.0) + at(weight, i))))


def rms_rows(rows, weight, eps):
    """`rms_norm` over every row of a batch, with one shared weight."""
    return mapList(rows, lambda row: rms_norm(row, weight, eps))


def rms_denominator(x, eps) -> float32:
    """`sqrt(mean(x²) + eps)`, the divisor `_norm` applies to every entry."""
    return sqrt(mean_square(x) + eps)


def mean_square(x) -> float32:
    """The left-to-right binary32 mean of a row's squares (`x.pow(2).mean(-1)`)."""
    total = fold(range(len(x)), toFloat32(0.0), lambda acc, i: acc + at(x, i) * at(x, i))
    return total / toFloat32(len(x))
