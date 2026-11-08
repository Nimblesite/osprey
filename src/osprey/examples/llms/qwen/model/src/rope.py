# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Rope
"""`Qwen3_5TextRotaryEmbedding` and `apply_rotary_pos_emb` for a run of text positions.

The text model's three position streams carry the same id, so the multimodal
recomposition collapses to the default rope: `inv_freq[j] = theta^(-2j / rotary_dim)`
for `j < rotary_dim / 2`, the angle is `position * inv_freq`, and the `cos`/`sin`
tables are those angles twice over (`cat(freqs, freqs)`). `rotate_half` rotates the
first `rotary_dim` values of each head and passes the rest through. A chunk of
positions has one table row each, and every head of a stacked row reads its own
position's row ([LLM-PREFILL]). Implements [LLM-MODEL-ROPE].
"""

from qwen import Qwen3_5Config as C
from qwen.Qwen3_5Device import at, constant, divider, must, quot, rem

__all__ = ["Tables", "tables", "tables_of", "rotated", "rotated_rows"]


class Tables:
    """`cos` and `sin` of `count` positions' angles, `[count, rotary_dim]` each."""
    cos: GpuBuffer[float32]
    sin: GpuBuffer[float32]
    count: int


def inv_freq(config, j):
    """`1 / theta^(2j / dim)` for the `j`th pair, as `compute_default_rope_parameters`."""
    dim = C.rotary_dim(config)
    return 1.0 / pow(config.rope_theta, toFloat(2 * (j % (dim // 2))) / toFloat(dim))


def angles(first, count, config, of):
    dim = C.rotary_dim(config)
    return constant(toList(map(range(count * dim), lambda i: toFloat32(of(toFloat(first + i // dim) * inv_freq(config, i % dim))))))


def tables_of(first, count, config) -> Tables:
    """The `cos` and `sin` rows `Qwen3_5TextRotaryEmbedding` yields for positions `first` to `first + count - 1`."""
    return Tables(cos=angles(first, count, config, lambda a: cos(a)), sin=angles(first, count, config, lambda a: sin(a)), count=count)


def tables(position, config) -> Tables:
    """The tables of the one position `position`."""
    return tables_of(position, 1, config)


def turned(x, i, d, half):
    """`rotate_half` at cell `i` of a head, `d` into the head."""
    return toFloat32(0.0) - at(x, wrapAdd(i, half)) if d < half else at(x, wrapSub(i, half))


def cell(x, c, s, i, t, d, dim, half):
    """Cell `i`, `d` into its head, rotated by table row `t`."""
    return at(x, i) if d >= dim else at(x, i) * at(c, wrapAdd(wrapMul(t, dim), d)) + turned(x, i, d, half) * at(s, wrapAdd(wrapMul(t, dim), d))


def rotated_rows(x, t, row_width, cells, shapes, config) -> GpuBuffer[float32]:
    """The `[count, row_width]` rows of `x` with the first `rotary_dim` values of every
    `head_dim`-wide head rotated by the row's table."""
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    row = divider(row_width)
    rm = row.mul
    rs = row.shift
    width = config.head_dim
    dim = C.rotary_dim(config)
    half = dim // 2
    c = t.cos
    s = t.sin
    return must(gpuMap(cells, lambda i: cell(x, c, s, i, quot(i, rm, rs), rem(i, mul, shift, width), dim, half)))


def rotated(x, t, cells, shapes, config) -> GpuBuffer[float32]:
    """One row `x`, every head rotated by the single table row."""
    return rotated_rows(x, t, gpuLength(x), cells, shapes, config)
