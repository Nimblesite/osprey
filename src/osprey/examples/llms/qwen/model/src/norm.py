# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Norm
"""`Qwen3_5RMSNorm` and `Qwen3_5RMSNormGated` on device buffers, over stacked rows.

`Qwen3_5RMSNorm` is zero-centred: `x * rsqrt(mean(x²) + eps) * (1 + weight)`. The
weights a model holds are already `1 + weight` (a GGUF stores them that way, and
`Qwen3_5Hf` adds the one), so every kernel multiplies by the stored value. The gated
norm of the DeltaNet is `x * rsqrt(mean(x²) + eps) * weight * silu(gate)`, per value
head, with the raw weight. A hidden row reduces in `norm_chunk` pieces, one thread
each, then one thread per row sums its pieces; a head reduces in one thread. Implements
[LLM-MODEL-ATTENTION], [LLM-MODEL-DELTANET].
"""

from qwen.Qwen3_5Device import at, inverse, must, quot, rem, rsqrt, sigmoid, squares

__all__ = ["rms_rows", "rms_heads", "gated"]


def rms_rows(x, w, config, shapes, rows) -> GpuBuffer[float32]:
    """`Qwen3_5RMSNorm` over each of the `rows.count` hidden rows of `x`."""
    eps = toFloat32(config.rms_norm_eps)
    chunk = shapes.norm_chunk
    hidden = config.hidden_size
    count = hidden // chunk
    mean = inverse(hidden)
    hm = shapes.hidden_div.mul
    hs = shapes.hidden_div.shift
    pieces = must(gpuMap(rows.hidden_chunks, lambda c: squares(x, wrapMul(c, chunk), chunk)))
    scale = must(gpuMap(rows.rows, lambda r: rsqrt(total(pieces, wrapMul(r, count), count) * mean + eps)))
    return must(gpuMap(rows.hidden, lambda i: at(x, i) * at(scale, quot(i, hm, hs)) * at(w, rem(i, hm, hs, hidden))))


def total(pieces, first, count):
    return fold(range(0, count), toFloat32(0.0), lambda a, c: a + at(pieces, wrapAdd(first, c)))


def head_scales(x, heads, width, eps):
    """`rsqrt(mean(x²) + eps)` of every `width`-wide head of `x`, one thread per head."""
    mean = inverse(width)
    return must(gpuMap(heads, lambda h: rsqrt(squares(x, wrapMul(h, width), width) * mean + eps)))


def rms_heads(x, w, heads, cells, div, width, eps: float32) -> GpuBuffer[float32]:
    """`Qwen3_5RMSNorm` over each head of `x`: `w` is `1 + weight`, `width` long."""
    mul = div.mul
    shift = div.shift
    scales = head_scales(x, heads, width, eps)
    return must(gpuMap(cells, lambda i: at(x, i) * at(scales, quot(i, mul, shift)) * at(w, rem(i, mul, shift, width))))


def gated(x, z, w, heads, cells, div, width, eps: float32) -> GpuBuffer[float32]:
    """`Qwen3_5RMSNormGated`: each head of `x` normed, times `w`, times `silu(z)`."""
    mul = div.mul
    shift = div.shift
    scales = head_scales(x, heads, width, eps)
    gate = sigmoid(z)
    return must(gpuMap(cells, lambda i: at(x, i) * at(scales, quot(i, mul, shift)) * at(w, rem(i, mul, shift, width)) * (at(z, i) * at(gate, i))))
