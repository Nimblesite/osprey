# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Attention
"""`Qwen3_5Attention` over stacked rows: gated grouped-query attention over the cache.

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
from qwen.Qwen3_5Device import Weight, at, grid, joined, linear, must, quot, rem, row_at, sigmoid, stacked

__all__ = ["Geometry", "Layer", "Mixed", "Core", "attend", "mix"]


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


def half(both, offset, cells, shapes, width):
    """The query (`offset` 0) or gate (`offset` `head_dim`) half of every head of `q_proj`'s row."""
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    pair = 2 * width
    return must(gpuMap(cells, lambda i: at(both, wrapAdd(wrapMul(quot(i, mul, shift), pair), wrapAdd(offset, rem(i, mul, shift, width))))))


def score(q, keys, h, t, width, kv_width, kvh, scale):
    """`q_h · k_{t, kvh}` times `head_dim^-0.5`."""
    return fold(range(0, width), toFloat32(0.0), lambda a, d: a + at(q, wrapAdd(wrapMul(h, width), d)) * at(keys, wrapAdd(wrapMul(t, kv_width), wrapAdd(wrapMul(kvh, width), d)))) * scale


def geometry(config, shapes) -> Geometry:
    """The head arithmetic both attention kernels address cells with: widths, and the
    split/shift pairs that turn a flat cell index into a head and its offset."""
    width = config.head_dim
    return Geometry(heads=config.num_attention_heads, width=width, kv_width=config.num_key_value_heads * width, hm=shapes.attention_heads.mul, hs=shapes.attention_heads.shift, gm=shapes.kv_groups.mul, gs=shapes.kv_groups.shift)


def scores(q, keys, n, config, shapes):
    """Every head's score against every cached position, `[position, head]`. The geometry
    is unpacked into scalars first: a kernel captures numbers, never a record."""
    g = geometry(config, shapes)
    heads = g.heads
    width = g.width
    kv_width = g.kv_width
    hm = g.hm
    hs = g.hs
    gm = g.gm
    gs = g.gs
    scale = toFloat32(1.0 / sqrt(toFloat(width)))
    return must(gpuMap(grid(heads * n), lambda i: score(q, keys, rem(i, hm, hs, heads), quot(i, hm, hs), width, kv_width, quot(rem(i, hm, hs, heads), gm, gs), scale)))


def peak(s, h, n, heads):
    return fold(range(0, n), at(s, h), lambda a, t: a if a > at(s, wrapAdd(wrapMul(t, heads), h)) else at(s, wrapAdd(wrapMul(t, heads), h)))


def total(raw, h, n, heads):
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + at(raw, wrapAdd(wrapMul(t, heads), h)))


def peaks_of(s, n, config, shapes):
    """Each head's largest score over `n` positions."""
    heads = config.num_attention_heads
    return must(gpuMap(shapes.q_heads, lambda h: peak(s, h, n, heads)))


def raw_of(s, n, peaks, config, shapes):
    """`exp(s - peak)` for every head and position."""
    heads = config.num_attention_heads
    hm = shapes.attention_heads.mul
    hs = shapes.attention_heads.shift
    return must(gpuMap(grid(heads * n), lambda i: exp(at(s, i) - at(peaks, rem(i, hm, hs, heads)))))


def scaled(raw, n, inverse, config, shapes):
    """The raw weights times their head's reciprocal total."""
    heads = config.num_attention_heads
    hm = shapes.attention_heads.mul
    hs = shapes.attention_heads.shift
    return must(gpuMap(grid(heads * n), lambda i: at(raw, i) * at(inverse, rem(i, hm, hs, heads))))


def weights(s, n, config, shapes):
    """`softmax` over positions for every head: `exp(s - peak) / total`, as unnormalised
    weights times the per-head reciprocal, which the output kernel applies."""
    heads = config.num_attention_heads
    raw = raw_of(s, n, peaks_of(s, n, config, shapes), config, shapes)
    inverse = must(gpuMap(shapes.q_heads, lambda h: exp(toFloat32(0.0) - ln(total(raw, h, n, heads)))))
    return scaled(raw, n, inverse, config, shapes)


def split_output(q, slot, n, config, shapes):
    """[LLM-STATE-OWNED] `softmax(scores) @ values` over the sealed rows and the tail as
    one softmax: one peak per head across both, one total, each segment's weighted sum."""
    heads = config.num_attention_heads
    a = slot.sealed
    b = n - a
    sa = scores(q, slot.sealed_keys, a, config, shapes)
    sb = scores(q, slot.keys, b, config, shapes)
    pa = peaks_of(sa, a, config, shapes)
    pb = peaks_of(sb, b, config, shapes)
    top = must(gpuMap(shapes.q_heads, lambda h: at(pa, h) if at(pa, h) > at(pb, h) else at(pb, h)))
    ra = raw_of(sa, a, top, config, shapes)
    rb = raw_of(sb, b, top, config, shapes)
    inverse = must(gpuMap(shapes.q_heads, lambda h: exp(toFloat32(0.0) - ln(total(ra, h, a, heads) + total(rb, h, b, heads)))))
    oa = output(scaled(ra, a, inverse, config, shapes), slot.sealed_values, a, config, shapes)
    ob = output(scaled(rb, b, inverse, config, shapes), slot.values, b, config, shapes)
    return must(gpuMap(shapes.q_cells, lambda i: at(oa, i) + at(ob, i)))


def appended(slot, key, value, kv_width, block):
    """The slot with one more position: the tail grows by a row, and a full tail is
    sealed onto the sealed rows first, so a token copies at most `block` rows."""
    tail = slot.positions - slot.sealed
    if tail == block:
        sealed_keys = slot.keys if slot.sealed == 0 else joined(slot.sealed_keys, slot.sealed * kv_width, slot.keys, tail * kv_width)
        sealed_values = slot.values if slot.sealed == 0 else joined(slot.sealed_values, slot.sealed * kv_width, slot.values, tail * kv_width)
        return Cache.AttentionSlot(keys=key, values=value, positions=slot.positions + 1, sealed_keys=sealed_keys, sealed_values=sealed_values, sealed=slot.positions)
    else:
        edge = tail * kv_width
        return update(slot, keys=joined(slot.keys, edge, key, kv_width), values=joined(slot.values, edge, value, kv_width), positions=slot.positions + 1)


def attended_all(q, slot, config, shapes):
    """Every head's attention over the slot's positions: one segment until a block is sealed."""
    n = slot.positions
    if slot.sealed == 0:
        return output(weights(scores(q, slot.keys, n, config, shapes), n, config, shapes), slot.values, n, config, shapes)
    else:
        return split_output(q, slot, n, config, shapes)


def attended(w, values, n, h, d, heads, width, kv_width, kvh):
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + at(w, wrapAdd(wrapMul(t, heads), h)) * at(values, wrapAdd(wrapMul(t, kv_width), wrapAdd(wrapMul(kvh, width), d))))


def output(w, values, n, config, shapes):
    """`softmax(scores) @ values` for every head, heads side by side."""
    g = geometry(config, shapes)
    heads = g.heads
    width = g.width
    kv_width = g.kv_width
    gm = g.gm
    gs = g.gs
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    return must(gpuMap(shapes.q_cells, lambda i: attended(w, values, n, quot(i, mul, shift), rem(i, mul, shift, width), heads, width, kv_width, quot(quot(i, mul, shift), gm, gs))))


class Core:
    """The gated outputs of the positions so far and the slot they leave."""
    gated: list[GpuBuffer[float32]]
    slot: Cache.AttentionSlot


def attend(both, base, k, v, layer, slot, config, shapes) -> Core:
    """One position: its query and gate read from `both` at `base`, its projected key
    `k` and value `v` joining the slot, and its gated heads before `o_proj`."""
    width = config.head_dim
    eps = toFloat32(config.rms_norm_eps)
    kv_width = config.num_key_value_heads * width
    t = Rope.tables(slot.positions, config)
    q = Rope.rotated(Norm.rms_heads(half(both, base, shapes.q_cells, shapes, width), layer.q_norm, shapes.q_heads, shapes.q_cells, shapes.head_dim, width, eps), t, shapes.q_cells, shapes, config)
    key = Rope.rotated(Norm.rms_heads(k, layer.k_norm, shapes.kv_heads, shapes.kv_cells, shapes.head_dim, width, eps), t, shapes.kv_cells, shapes, config)
    grown = appended(slot, key, v, kv_width, shapes.kv_block)
    out = attended_all(q, grown, config, shapes)
    gate = sigmoid(half(both, wrapAdd(base, width), shapes.q_cells, shapes, width))
    gated = must(gpuMap(shapes.q_cells, lambda i: at(out, i) * at(gate, i)))
    return Core(gated=[gated], slot=grown)


def attend_next(acc, both, ks, vs, t, count, layer, config, shapes):
    width = config.head_dim
    kv_width = config.num_key_value_heads * width
    core = attend(both, t * 2 * config.num_attention_heads * width, row_at(ks, t, count, kv_width, shapes.kv_cells), row_at(vs, t, count, kv_width, shapes.kv_cells), layer, acc.slot, config, shapes)
    return Core(gated=listConcat(acc.gated, core.gated), slot=core.slot)


def mix(x, layer, slot, config, shapes, count) -> Mixed:
    """`Qwen3_5Attention.forward` for the `count` normed rows `x` from position
    `slot.positions`: each projection reads its weight once for every row, then each
    position attends over the cache and the positions before it ([LLM-PREFILL])."""
    hidden = config.hidden_size
    q_width = config.num_attention_heads * config.head_dim
    kv_width = config.num_key_value_heads * config.head_dim
    both = linear(x, layer.q, count, hidden, 2 * q_width)
    ks = linear(x, layer.k, count, hidden, kv_width)
    vs = linear(x, layer.v, count, hidden, kv_width)
    core = fold(range(0, count), Core(gated=[], slot=slot), lambda acc, t: attend_next(acc, both, ks, vs, t, count, layer, config, shapes))
    return Mixed(hidden=linear(stacked(core.gated, q_width), layer.o, count, q_width, hidden), slot=core.slot)
