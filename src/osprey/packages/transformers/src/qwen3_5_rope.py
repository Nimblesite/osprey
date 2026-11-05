# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=transformers
# osprey: module=Qwen3_5Rope
"""`Qwen3_5TextRotaryEmbedding` and `apply_rotary_pos_emb` for one text position.

The text model's three position streams carry the same id, so the multimodal
recomposition collapses to the default rope: `inv_freq[j] = theta^(-2j / rotary_dim)`
for `j < rotary_dim / 2`, the angle is `position * inv_freq`, and the `cos`/`sin`
tables are those angles twice over (`cat(freqs, freqs)`). `rotate_half` rotates the
first `rotary_dim` values of each head and passes the rest through. Implements
[LLM-MODEL-ROPE].
"""

from transformers import Qwen3_5Config as C
from transformers.Qwen3_5Device import at, constant, must, quot, rem

__all__ = ["Tables", "tables", "rotated"]


class Tables:
    """`cos` and `sin` of one position's angles, `rotary_dim` wide each."""
    cos: GpuBuffer[float32]
    sin: GpuBuffer[float32]


def inv_freq(config, j):
    """`1 / theta^(2j / dim)` for the `j`th pair, as `compute_default_rope_parameters`."""
    dim = C.rotary_dim(config)
    return 1.0 / pow(config.rope_theta, toFloat(2 * (j % (dim // 2))) / toFloat(dim))


def angles(position, config, of) -> GpuBuffer[float32]:
    return constant(toList(map(range(C.rotary_dim(config)), lambda j: toFloat32(of(toFloat(position) * inv_freq(config, j))))))


def tables(position, config) -> Tables:
    """The `cos` and `sin` rows `Qwen3_5TextRotaryEmbedding` yields for `position`."""
    return Tables(cos=angles(position, config, lambda a: cos(a)), sin=angles(position, config, lambda a: sin(a)))


def turned(x, i, d, half) -> float32:
    """`rotate_half` at cell `i` of a head, `d` into the head."""
    return toFloat32(0.0) - at(x, wrapAdd(i, half)) if d < half else at(x, wrapSub(i, half))


def cell(x, c, s, i, d, dim, half) -> float32:
    return at(x, i) if d >= dim else at(x, i) * at(c, d) + turned(x, i, d, half) * at(s, d)


def rotated(x, t, cells, shapes, config) -> GpuBuffer[float32]:
    """`x` with the first `rotary_dim` values of every `head_dim`-wide head rotated."""
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    width = config.head_dim
    dim = C.rotary_dim(config)
    half = dim // 2
    c = t.cos
    s = t.sin
    return must(gpuMap(cells, lambda i: cell(x, c, s, i, rem(i, mul, shift, width), dim, half)))
