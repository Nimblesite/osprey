# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Attention
"""`Qwen3_5Attention` over stacked rows: gated grouped-query attention over the cache.

As `Qwen3_5Attention.forward`: `q_proj` yields each head's query and its gate side by
side; the queries are normed per head, keys likewise, both rotated; the new key and
value rows join the cache; every query head attends over the cached positions with
the key/value head `h // groups`, scores scaled by `head_dim^-0.5`, softmax in
`float32`; the output is gated by `sigmoid(gate)` and projected by `o_proj`.

[LLM-PREFILL] A chunk of T positions runs each stage as one dispatch over all T rows:
the T query rows score every cached position and the T new ones together, laid out
`[query, position, head]`, and a query reads only the positions up to its own (the
causal mask), so the chunk leaves the cache and the outputs one position at a time
would. The cache keeps its sealed rows and its tail apart ([LLM-STATE-OWNED]), so a
chunk scores the two segments separately and joins them under one softmax: one peak
per query head across both, one total, each segment's weighted sum. Implements
[LLM-MODEL-ATTENTION], [LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Cache as Cache
from qwen import Qwen3_5Norm as Norm
from qwen import Qwen3_5Rope as Rope
from qwen.Qwen3_5Device import Div, Weight, at, divider, grid, joined, linear, must, quot, rem, sigmoid, slice_of

__all__ = ["Layer", "Mixed", "mix"]


class Layer:
    """One full-attention layer's projections and head norms (`1 + weight`)."""
    q: Weight
    k: Weight
    v: Weight
    o: Weight
    q_norm: GpuBuffer[float32]
    k_norm: GpuBuffer[float32]


class Mixed:
    """The layer's output rows and the cache slot it leaves."""
    hidden: GpuBuffer[float32]
    slot: Cache.AttentionSlot


class Rows:
    """The grids of `count` positions' query cells, query heads, key/value cells and heads."""
    count: int
    q_cells: GpuBuffer[int]
    q_heads: GpuBuffer[int]
    kv_cells: GpuBuffer[int]
    kv_heads: GpuBuffer[int]


class Segment:
    """`rows` cached positions' keys and values; query row `t` reads positions
    `j <= limit + t`. `scores` is the `[count, rows, heads]` grid and `span` divides
    an index of it into its query row."""
    keys: GpuBuffer[float32]
    values: GpuBuffer[float32]
    rows: int
    limit: int
    scores: GpuBuffer[int]
    span: Div


class Geometry:
    """The head arithmetic the kernels address cells with: widths, and the
    split/shift pairs that turn a flat cell index into a head and its offset."""
    heads: int
    width: int
    kv_width: int
    hm: int
    hs: int
    gm: int
    gs: int


def rows_of(count, config, shapes) -> Rows:
    """The grids for `count` positions; one position reuses the model's own."""
    width = config.head_dim
    if count == 1:
        return Rows(count=1, q_cells=shapes.q_cells, q_heads=shapes.q_heads, kv_cells=shapes.kv_cells, kv_heads=shapes.kv_heads)
    else:
        return Rows(count=count, q_cells=grid(count * config.num_attention_heads * width), q_heads=grid(count * config.num_attention_heads), kv_cells=grid(count * config.num_key_value_heads * width), kv_heads=grid(count * config.num_key_value_heads))


def segment(keys, values, rows, limit, count, config) -> Segment:
    heads = config.num_attention_heads
    return Segment(keys=keys, values=values, rows=rows, limit=limit, scores=grid(count * rows * heads), span=divider(rows * heads))


def geometry(config, shapes) -> Geometry:
    width = config.head_dim
    return Geometry(heads=config.num_attention_heads, width=width, kv_width=config.num_key_value_heads * width, hm=shapes.attention_heads.mul, hs=shapes.attention_heads.shift, gm=shapes.kv_groups.mul, gs=shapes.kv_groups.shift)


def half(both, offset, cells, shapes, width):
    """The query (`offset` 0) or gate (`offset` `head_dim`) half of every head of every `q_proj` row."""
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    pair = 2 * width
    return must(gpuMap(cells, lambda i: at(both, wrapAdd(wrapMul(quot(i, mul, shift), pair), wrapAdd(offset, rem(i, mul, shift, width))))))


def score(q, keys, t, j, h, kvh, width, q_width, kv_width, scale):
    """`q_{t,h} · k_{j,kvh}` times `head_dim^-0.5`."""
    return fold(range(0, width), toFloat32(0.0), lambda a, d: a + at(q, wrapAdd(wrapMul(t, q_width), wrapAdd(wrapMul(h, width), d))) * at(keys, wrapAdd(wrapMul(j, kv_width), wrapAdd(wrapMul(kvh, width), d)))) * scale


def score_at(q, keys, span_mul, span_shift, seg_rows, hm, hs, gm, gs, heads, width, kv_width, i, q_width, scale):
    """Score cell `i` of a `[count, rows, heads]` grid: its query row, position and head."""
    t = quot(i, span_mul, span_shift)
    r = rem(i, span_mul, span_shift, seg_rows * heads)
    h = rem(r, hm, hs, heads)
    return score(q, keys, t, quot(r, hm, hs), h, quot(h, gm, gs), width, q_width, kv_width, scale)


def scores(q, seg, config, shapes):
    """Every query row's score against every position of the segment for every head."""
    g = geometry(config, shapes)
    heads = g.heads
    width = g.width
    kv_width = g.kv_width
    hm = g.hm
    hs = g.hs
    gm = g.gm
    gs = g.gs
    keys = seg.keys
    seg_rows = seg.rows
    span_mul = seg.span.mul
    span_shift = seg.span.shift
    scale = toFloat32(1.0 / sqrt(toFloat(width)))
    return must(gpuMap(seg.scores, lambda i: score_at(q, keys, span_mul, span_shift, seg_rows, hm, hs, gm, gs, heads, width, kv_width, i, heads * width, scale)))


def cell_of(t, j, h, rows, heads):
    """Where `[query, position, head]` lies in a segment's scores."""
    return wrapAdd(wrapMul(wrapAdd(wrapMul(t, rows), j), heads), h)


def peak(s, t, h, rows, heads, limit):
    """Query `t`, head `h`'s largest score over the positions it reads."""
    return fold(range(0, rows), at(s, cell_of(t, 0, h, rows, heads)), lambda a, j: a if j > wrapAdd(limit, t) or a > at(s, cell_of(t, j, h, rows, heads)) else at(s, cell_of(t, j, h, rows, heads)))


def peaks_of(s, seg, rows, config, shapes):
    g = geometry(config, shapes)
    heads = g.heads
    hm = g.hm
    hs = g.hs
    seg_rows = seg.rows
    limit = seg.limit
    return must(gpuMap(rows.q_heads, lambda i: peak(s, quot(i, hm, hs), rem(i, hm, hs, heads), seg_rows, heads, limit)))


def exps_of(s, top, seg, config, shapes):
    """`exp(s - top)` of every score cell, once: the totals and the weighted sums
    both read it, so no output cell recomputes it per head offset."""
    g = geometry(config, shapes)
    heads = g.heads
    hm = g.hm
    hs = g.hs
    span_mul = seg.span.mul
    span_shift = seg.span.shift
    return must(gpuMap(seg.scores, lambda i: exp(at(s, i) - at(top, wrapAdd(wrapMul(quot(i, span_mul, span_shift), heads), rem(i, hm, hs, heads))))))


def total(e, t, h, rows, heads, limit):
    """`Σ exp(s - top)` over the positions query `t` reads, in position order."""
    return fold(range(0, rows), toFloat32(0.0), lambda a, j: a if j > wrapAdd(limit, t) else a + at(e, cell_of(t, j, h, rows, heads)))


def totals_of(e, seg, rows, config, shapes):
    g = geometry(config, shapes)
    heads = g.heads
    hm = g.hm
    hs = g.hs
    seg_rows = seg.rows
    limit = seg.limit
    return must(gpuMap(rows.q_heads, lambda i: total(e, quot(i, hm, hs), rem(i, hm, hs, heads), seg_rows, heads, limit)))


def attended(e, inv, values, t, h, d, rows, heads, limit, width, kv_width, kvh):
    """`Σ (exp(s - top) * inv) * v` over the positions query `t` reads, in order."""
    return fold(range(0, rows), toFloat32(0.0), lambda a, j: a if j > wrapAdd(limit, t) else a + at(e, cell_of(t, j, h, rows, heads)) * at(inv, wrapAdd(wrapMul(t, heads), h)) * at(values, wrapAdd(wrapMul(j, kv_width), wrapAdd(wrapMul(kvh, width), d))))


def attended_at(e, inv, values, seg_rows, seg_limit, hm, hs, gm, gs, heads, width, kv_width, mul, shift, i):
    """Output cell `i` of the stacked `[count, heads * width]` rows: its query row, head and offset."""
    hg = quot(i, mul, shift)
    h = rem(hg, hm, hs, heads)
    return attended(e, inv, values, quot(hg, hm, hs), h, rem(i, mul, shift, width), seg_rows, heads, seg_limit, width, kv_width, quot(h, gm, gs))


def output(e, inv, seg, rows, config, shapes):
    """`softmax(scores) @ values` of the segment for every query row, heads side by side."""
    g = geometry(config, shapes)
    heads = g.heads
    width = g.width
    kv_width = g.kv_width
    hm = g.hm
    hs = g.hs
    gm = g.gm
    gs = g.gs
    values = seg.values
    seg_rows = seg.rows
    seg_limit = seg.limit
    mul = shapes.head_dim.mul
    shift = shapes.head_dim.shift
    return must(gpuMap(rows.q_cells, lambda i: attended_at(e, inv, values, seg_rows, seg_limit, hm, hs, gm, gs, heads, width, kv_width, mul, shift, i)))


def reciprocal(total_a, total_b, rows):
    """`1 / (a + b)` per query head, as `exp(-ln)`, the device's own reciprocal."""
    return must(gpuMap(rows.q_heads, lambda h: exp(toFloat32(0.0) - ln(at(total_a, h) + at(total_b, h)))))


def split_output(q, a, b, rows, config, shapes):
    """Attention over the sealed rows and the tail as one softmax: one peak per query
    head across both, one total, each segment's weighted sum."""
    sa = scores(q, a, config, shapes)
    sb = scores(q, b, config, shapes)
    pa = peaks_of(sa, a, rows, config, shapes)
    pb = peaks_of(sb, b, rows, config, shapes)
    top = must(gpuMap(rows.q_heads, lambda h: at(pa, h) if at(pa, h) > at(pb, h) else at(pb, h)))
    ea = exps_of(sa, top, a, config, shapes)
    eb = exps_of(sb, top, b, config, shapes)
    inv = reciprocal(totals_of(ea, a, rows, config, shapes), totals_of(eb, b, rows, config, shapes), rows)
    oa = output(ea, inv, a, rows, config, shapes)
    ob = output(eb, inv, b, rows, config, shapes)
    return must(gpuMap(rows.q_cells, lambda i: at(oa, i) + at(ob, i)))


def attended_all(q, slot, tail_keys, tail_values, rows, config, shapes):
    """Every query row's attention over the slot's sealed rows and the grown tail."""
    tail = slot.positions - slot.sealed
    b = segment(tail_keys, tail_values, tail + rows.count, tail, rows.count, config)
    if slot.sealed == 0:
        s = scores(q, b, config, shapes)
        e = exps_of(s, peaks_of(s, b, rows, config, shapes), b, config, shapes)
        none = must(gpuMap(rows.q_heads, lambda _: toFloat32(0.0)))
        return output(e, reciprocal(totals_of(e, b, rows, config, shapes), none, rows), b, rows, config, shapes)
    else:
        return split_output(q, segment(slot.sealed_keys, slot.sealed_values, slot.sealed, slot.sealed, rows.count, config), b, rows, config, shapes)


def grown(slot, tail_keys, tail_values, count, kv_width, block):
    """The slot with `count` more positions in the grown tail, every full block of it
    sealed onto the sealed rows, so a token copies at most a block of rows and a
    chunk its own rows plus a block ([LLM-STATE-OWNED])."""
    n = slot.positions + count
    sealed = (n - 1) // block * block
    moved = sealed - slot.sealed
    if moved == 0:
        return Cache.AttentionSlot(keys=tail_keys, values=tail_values, positions=n, sealed_keys=slot.sealed_keys, sealed_values=slot.sealed_values, sealed=slot.sealed)
    else:
        rest = grid((n - sealed) * kv_width)
        return Cache.AttentionSlot(keys=slice_of(tail_keys, moved * kv_width, rest), values=slice_of(tail_values, moved * kv_width, rest), positions=n, sealed_keys=joined(slot.sealed_keys, slot.sealed * kv_width, tail_keys, moved * kv_width), sealed_values=joined(slot.sealed_values, slot.sealed * kv_width, tail_values, moved * kv_width), sealed=sealed)


def normed_rotated(x, w, t, heads, cells, row_width, config, shapes):
    """Every head of the rows of `x` normed by `w` and rotated by its position's table."""
    width = config.head_dim
    return Rope.rotated_rows(Norm.rms_heads(x, w, heads, cells, shapes.head_dim, width, toFloat32(config.rms_norm_eps)), t, row_width, cells, shapes, config)


def mix(x, layer, slot, config, shapes, count) -> Mixed:
    """`Qwen3_5Attention.forward` for the `count` normed rows `x` from position
    `slot.positions`: each projection reads its weight once for every row, and every
    stage runs over all the rows in one dispatch ([LLM-PREFILL])."""
    hidden = config.hidden_size
    width = config.head_dim
    q_width = config.num_attention_heads * width
    kv_width = config.num_key_value_heads * width
    tail = slot.positions - slot.sealed
    rows = rows_of(count, config, shapes)
    both = linear(x, layer.q, count, hidden, 2 * q_width)
    t = Rope.tables_of(slot.positions, count, config)
    q = normed_rotated(half(both, 0, rows.q_cells, shapes, width), layer.q_norm, t, rows.q_heads, rows.q_cells, q_width, config, shapes)
    keys = joined(slot.keys, tail * kv_width, normed_rotated(linear(x, layer.k, count, hidden, kv_width), layer.k_norm, t, rows.kv_heads, rows.kv_cells, kv_width, config, shapes), count * kv_width)
    values = joined(slot.values, tail * kv_width, linear(x, layer.v, count, hidden, kv_width), count * kv_width)
    gate = sigmoid(half(both, width, rows.q_cells, shapes, width))
    out = attended_all(q, slot, keys, values, rows, config, shapes)
    gated = must(gpuMap(rows.q_cells, lambda i: at(out, i) * at(gate, i)))
    return Mixed(hidden=linear(gated, layer.o, count, q_width, hidden), slot=grown(slot, keys, values, count, kv_width, shapes.kv_block))
