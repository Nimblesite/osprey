# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Attention
"""`Qwen3_5Attention` for one token: gated grouped-query attention over the cache.

As `Qwen3_5Attention.forward`: `q_proj` yields each head's query and its gate side by
side; the queries are normed per head, keys likewise, both rotated; the new key and
value rows join the cache; every query head attends over all cached positions with
the key/value head `h // groups`, scores scaled by `head_dim^-0.5`, softmax in
`float32`; the output is gated by `sigmoid(gate)` and projected by `o_proj`. Scores
are laid out `[position, head]`, so no kernel divides by the context length.
Implements [LLM-MODEL-ATTENTION], [LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Cache as Cache
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5Norm as Norm
from qwen import Qwen3_5Rope as Rope
from qwen.Qwen3_5Device import Weight, at, grid, linear, must, quot, rem, sigmoid

__all__ = ["Geometry", "Layer", "Mixed", "mix"]


class Layer:
    """One full-attention layer's projections and head norms (`1 + weight`)."""
    q: Weight
    k: Weight
    v: Weight
    o: Weight
    q_norm: GpuBuffer[float32]
    k_norm: GpuBuffer[float32]


class Geometry:
    """The head widths and the split/shift pairs a cell index is addressed with."""
    heads: int
    width: int
    kv_width: int
    hm: int
    hs: int
    gm: int
    gs: int


class Mixed:
    """The layer's output row and the cache slot it leaves."""
    hidden: GpuBuffer[float32]
    slot: Cache.AttentionSlot


def half(both, offset, cells, shapes, width) -> GpuBuffer[float32]:
    """The query (`offset` 0) or gate (`offset` `head_dim`) half of every head of `q_proj`'s row."""
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    pair = 2 * width
    return must(gpuMap(cells, lambda i: at(both, wrapAdd(wrapMul(quot(i, mul, shift), pair), wrapAdd(offset, rem(i, mul, shift, width))))))


def appended(old, count, width, fresh) -> GpuBuffer[float32]:
    """`count` cached rows of `width` with `fresh` as row `count`."""
    edge = count * width
    return must(gpuMap(grid(edge + width), lambda i: at(old, i) if i < edge else at(fresh, wrapSub(i, edge))))


def score(q, keys, h, t, width, kv_width, kvh, scale: float32) -> float32:
    """`q_h · k_{t, kvh}` times `head_dim^-0.5`."""
    return fold(range(0, width), toFloat32(0.0), lambda a, d: a + at(q, wrapAdd(wrapMul(h, width), d)) * at(keys, wrapAdd(wrapMul(t, kv_width), wrapAdd(wrapMul(kvh, width), d)))) * scale


def geometry(config, shapes) -> Geometry:
    """The head arithmetic both attention kernels address cells with: widths, and the
    split/shift pairs that turn a flat cell index into a head and its offset."""
    width = config.head_dim
    return Geometry(heads=config.num_attention_heads, width=width, kv_width=config.num_key_value_heads * width, hm=shapes.attention_heads.mul, hs=shapes.attention_heads.shift, gm=shapes.kv_groups.mul, gs=shapes.kv_groups.shift)


def scores(q, keys, n, config, shapes) -> GpuBuffer[float32]:
    """Every head's score against every cached position, `[position, head]`."""
    g = geometry(config, shapes)
    scale = toFloat32(1.0 / sqrt(toFloat(g.width)))
    return must(gpuMap(grid(g.heads * n), lambda i: score(q, keys, rem(i, g.hm, g.hs, g.heads), quot(i, g.hm, g.hs), g.width, g.kv_width, quot(rem(i, g.hm, g.hs, g.heads), g.gm, g.gs), scale)))


def peak(s, h, n, heads) -> float32:
    return fold(range(0, n), at(s, h), lambda a, t: a if a > at(s, wrapAdd(wrapMul(t, heads), h)) else at(s, wrapAdd(wrapMul(t, heads), h)))


def total(raw, h, n, heads) -> float32:
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + at(raw, wrapAdd(wrapMul(t, heads), h)))


def weights(s, n, config, shapes) -> GpuBuffer[float32]:
    """`softmax` over positions for every head: `exp(s - peak) / total`, as unnormalised
    weights times the per-head reciprocal, which the output kernel applies."""
    heads = config.num_attention_heads
    hm = shapes.attention_heads.mul
    hs = shapes.attention_heads.shift
    peaks = must(gpuMap(shapes.q_heads, lambda h: peak(s, h, n, heads)))
    raw = must(gpuMap(grid(heads * n), lambda i: exp(at(s, i) - at(peaks, rem(i, hm, hs, heads)))))
    inverse = must(gpuMap(shapes.q_heads, lambda h: exp(toFloat32(0.0) - ln(total(raw, h, n, heads)))))
    return must(gpuMap(grid(heads * n), lambda i: at(raw, i) * at(inverse, rem(i, hm, hs, heads))))


def attended(w, values, n, h, d, heads, width, kv_width, kvh) -> float32:
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + at(w, wrapAdd(wrapMul(t, heads), h)) * at(values, wrapAdd(wrapMul(t, kv_width), wrapAdd(wrapMul(kvh, width), d))))


def output(w, values, n, config, shapes) -> GpuBuffer[float32]:
    """`softmax(scores) @ values` for every head, heads side by side."""
    g = geometry(config, shapes)
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    return must(gpuMap(shapes.q_cells, lambda i: attended(w, values, n, quot(i, mul, shift), rem(i, mul, shift, g.width), g.heads, g.width, g.kv_width, quot(quot(i, mul, shift), g.gm, g.gs))))


def mix(x, layer, slot, config, shapes) -> Mixed:
    """`Qwen3_5Attention.forward` for the normed row `x` at position `slot.positions`."""
    hidden = config.hidden_size
    width = config.head_dim
    eps = toFloat32(config.rms_norm_eps)
    q_width = config.num_attention_heads * width
    kv_width = config.num_key_value_heads * width
    both = linear(x, layer.q, 1, hidden, 2 * q_width)
    t = Rope.tables(slot.positions, config)
    q = Rope.rotated(Norm.rms_heads(half(both, 0, shapes.q_cells, shapes, width), layer.q_norm, shapes.q_heads, shapes.q_cells, shapes.head_dim, width, eps), t, shapes.q_cells, shapes, config)
    k = Rope.rotated(Norm.rms_heads(linear(x, layer.k, 1, hidden, kv_width), layer.k_norm, shapes.kv_heads, shapes.kv_cells, shapes.head_dim, width, eps), t, shapes.kv_cells, shapes, config)
    keys = appended(slot.keys, slot.positions, kv_width, k)
    values = appended(slot.values, slot.positions, kv_width, linear(x, layer.v, 1, hidden, kv_width))
    n = slot.positions + 1
    out = output(weights(scores(q, keys, n, config, shapes), n, config, shapes), values, n, config, shapes)
    gate = sigmoid(half(both, width, shapes.q_cells, shapes, width))
    gated = must(gpuMap(shapes.q_cells, lambda i: at(out, i) * at(gate, i)))
    return Mixed(hidden=linear(gated, layer.o, 1, q_width, hidden), slot=Cache.AttentionSlot(keys=keys, values=values, positions=n))
