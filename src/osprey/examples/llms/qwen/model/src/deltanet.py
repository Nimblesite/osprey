# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5DeltaNet
"""`Qwen3_5GatedDeltaNet` over stacked rows: every stage over all the positions at once.

As `Qwen3_5GatedDeltaNet.forward`: `in_proj_qkv` feeds a depthwise causal convolution
over the last `conv_kernel` inputs of every channel (`causal_conv1d_update`) followed
by `silu`; the result splits into queries, keys and values; `beta = sigmoid(in_proj_b(x))`;
the decay is `exp(-exp(A_log) * softplus(in_proj_a(x) + dt_bias))`; queries and keys
are `l2norm`ed per head and each key head serves `num_v_heads / num_k_heads` value
heads (`repeat_interleave`), the query also scaled by `head_k_dim^-0.5`;
`torch_recurrent_gated_delta_rule` updates the `[num_v_heads, head_k_dim, head_v_dim]`
state; the output is gated by `Qwen3_5RMSNormGated` with `in_proj_z(x)` and projected
by `out_proj`.

[LLM-PREFILL] A chunk of T positions runs each stage as one dispatch over all T rows: the
convolution reads a position's `conv_kernel` newest inputs from the stacked `in_proj_qkv`
rows and, before the chunk, from the cached `conv_states`; one `gpuDeltaRule` steps the
recurrent rule through the T positions and returns the T outputs with the final state;
the gated norm and `out_proj` take the T rows together.

A GGUF lists the value heads in llama.cpp's tiled order, where value head `t` pairs
with key head `t % num_k_heads`; a checkpoint in Transformers' order pairs `t` with
`t // ratio`. `tiled` is that choice, made once by the loader. Implements
[LLM-MODEL-DELTANET], [LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Cache as Cache
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5Norm as Norm
from qwen.Qwen3_5Device import Div, Weight, at, divider, grid, linear, must, quot, rem, rsqrt, sigmoid, slice_of, softplus, squares

__all__ = ["Layer", "Mixed", "mix"]


class Layer:
    """One linear-attention layer's projections and per-head parameters."""
    qkv: Weight
    z: Weight
    b: Weight
    a: Weight
    out: Weight
    conv: GpuBuffer[float32]
    a_neg: GpuBuffer[float32]
    dt_bias: GpuBuffer[float32]
    norm: GpuBuffer[float32]


class Mixed:
    """The layer's output rows and the cache slot it leaves."""
    hidden: GpuBuffer[float32]
    slot: Cache.DeltaSlot


class Rows:
    """The grids of `count` positions: one thread per convolution channel, value cell,
    key head or value head of every position, and the reciprocals that find a thread's
    position in them."""
    count: int
    conv: GpuBuffer[int]
    value: GpuBuffer[int]
    k_heads: GpuBuffer[int]
    v_heads: GpuBuffer[int]
    conv_div: Div
    value_div: Div
    v_heads_div: Div


def rows_of(count, config, shapes) -> Rows:
    """The grids for `count` positions; one position reuses the model's own."""
    conv_div = divider(C.conv_dim(config))
    value_div = divider(C.value_dim(config))
    v_heads_div = divider(config.linear_num_value_heads)
    if count == 1:
        return Rows(count=1, conv=shapes.conv, value=shapes.value, k_heads=shapes.linear_k_heads, v_heads=shapes.linear_v_heads, conv_div=conv_div, value_div=value_div, v_heads_div=v_heads_div)
    else:
        return Rows(count=count, conv=grid(count * C.conv_dim(config)), value=grid(count * C.value_dim(config)), k_heads=grid(count * config.linear_num_key_heads), v_heads=grid(count * config.linear_num_value_heads), conv_div=conv_div, value_div=value_div, v_heads_div=v_heads_div)


def source(qkv, old, t, c, j, conv_dim, kernel):
    """Channel `c`'s input `kernel - 1 - j` positions before position `t`: a stacked
    row of `qkv`, or before the chunk the cached `conv_states`, whose newest input is
    the position before the chunk."""
    p = wrapSub(wrapAdd(t, j), kernel - 1)
    return at(qkv, wrapAdd(wrapMul(p, conv_dim), c)) if p >= 0 else at(old, wrapAdd(wrapMul(c, kernel), wrapAdd(t, wrapAdd(j, 1))))


def window(qkv, old, count, config, shapes):
    """`conv_states` after the chunk: every channel's `conv_kernel` newest inputs, newest last."""
    kernel = config.linear_conv_kernel_dim
    km = shapes.conv_kernel.mul
    ks = shapes.conv_kernel.shift
    conv_dim = C.conv_dim(config)
    return must(gpuMap(shapes.conv_history, lambda i: source(qkv, old, count - 1, quot(i, km, ks), rem(i, km, ks, kernel), conv_dim, kernel)))


def tap(w, qkv, old, t, c, conv_dim, kernel):
    """Channel `c`'s depthwise convolution at position `t` over its `kernel` newest inputs."""
    return fold(range(0, kernel), toFloat32(0.0), lambda a, j: a + at(w, wrapAdd(wrapMul(c, kernel), j)) * source(qkv, old, t, c, j, conv_dim, kernel))


def convolved(w, qkv, old, rows, config):
    """`silu(conv1d(inputs))` for every channel of every position."""
    kernel = config.linear_conv_kernel_dim
    conv_dim = C.conv_dim(config)
    cm = rows.conv_div.mul
    cs = rows.conv_div.shift
    tapped = must(gpuMap(rows.conv, lambda i: tap(w, qkv, old, quot(i, cm, cs), rem(i, cm, cs, conv_dim), conv_dim, kernel)))
    gate = sigmoid(tapped)
    return must(gpuMap(rows.conv, lambda c: at(tapped, c) * at(gate, c)))


def unit_scales(mixed, first, rows, config, shapes):
    """`l2norm`'s `rsqrt(sum(x²) + 1e-6)` for every key head from `first` of every position."""
    width = config.linear_key_head_dim
    k_heads = config.linear_num_key_heads
    conv_dim = C.conv_dim(config)
    eps = toFloat32(0.000001)
    km = shapes.linear_k_heads_div.mul
    ks = shapes.linear_k_heads_div.shift
    return must(gpuMap(rows.k_heads, lambda h: rsqrt(squares(mixed, wrapAdd(wrapMul(quot(h, km, ks), conv_dim), wrapAdd(first, wrapMul(rem(h, km, ks, k_heads), width))), width) + eps)))


def key_head(t, tiled, km, ks, k_heads, rm, rs):
    """The key head value head `t` reads: `t % num_k_heads` tiled, else `t // ratio`."""
    return rem(t, km, ks, k_heads) if tiled == 1 else quot(t, rm, rs)


def spread_cell(mixed, scales, first, t, kh, d, k_heads, conv_dim, width, factor):
    """Cell `d` of key head `kh` at position `t`, scaled to unit length, times `factor`."""
    return at(mixed, wrapAdd(wrapMul(t, conv_dim), wrapAdd(first, wrapAdd(wrapMul(kh, width), d)))) * at(scales, wrapAdd(wrapMul(t, k_heads), kh)) * factor


def spread_at(mixed, scales, first, i, tiled, factor, k_heads, conv_dim, width, value_dim, vm, vs, dm, ds, km, ks, rm, rs):
    """Value cell `i` of the stacked `[count, value_dim]` spread: its position, value head and cell."""
    t = quot(i, vm, vs)
    cell = rem(i, vm, vs, value_dim)
    kh = key_head(quot(cell, dm, ds), tiled, km, ks, k_heads, rm, rs)
    return spread_cell(mixed, scales, first, t, kh, rem(cell, dm, ds, width), k_heads, conv_dim, width, factor)


def spread(mixed, first, factor, tiled, rows, config, shapes):
    """Unit-length key-head rows from `first`, one per value head of every position, times `factor`."""
    width = config.linear_key_head_dim
    k_heads = config.linear_num_key_heads
    conv_dim = C.conv_dim(config)
    value_dim = C.value_dim(config)
    scales = unit_scales(mixed, first, rows, config, shapes)
    vm = rows.value_div.mul
    vs = rows.value_div.shift
    dm = shapes.linear_k_dim.mul
    ds = shapes.linear_k_dim.shift
    km = shapes.linear_k_heads_div.mul
    ks = shapes.linear_k_heads_div.shift
    rm = shapes.linear_ratio.mul
    rs = shapes.linear_ratio.shift
    return must(gpuMap(rows.value, lambda i: spread_at(mixed, scales, first, i, tiled, factor, k_heads, conv_dim, width, value_dim, vm, vs, dm, ds, km, ks, rm, rs)))


def values_of(mixed, rows, config):
    """Every position's values, the last third of its convolved row, as `[count, value_dim]`."""
    offset = 2 * C.key_dim(config)
    value_dim = C.value_dim(config)
    conv_dim = C.conv_dim(config)
    vm = rows.value_div.mul
    vs = rows.value_div.shift
    return must(gpuMap(rows.value, lambda i: at(mixed, wrapAdd(wrapMul(quot(i, vm, vs), conv_dim), wrapAdd(offset, rem(i, vm, vs, value_dim))))))


def decay_cell(a, a_neg, dt, h, head):
    """`exp(-exp(A_log) * softplus(a + dt_bias))` for value head `head` of row `h`."""
    return exp(at(a_neg, head) * softplus(at(a, h) + at(dt, head)))


def decay_of(a, layer, rows, config):
    """The decay of every value head of every position."""
    heads = config.linear_num_value_heads
    hm = rows.v_heads_div.mul
    hs = rows.v_heads_div.shift
    a_neg = layer.a_neg
    dt = layer.dt_bias
    return must(gpuMap(rows.v_heads, lambda h: decay_cell(a, a_neg, dt, h, rem(h, hm, hs, heads))))


def recurred(mixed, slot, b, a, layer, tiled, rows, config, shapes):
    """The state after `torch_recurrent_gated_delta_rule` over every position in order,
    then every position's outputs: one kernel steps through them ([LLM-DELTA-RULE])."""
    key_dim = C.key_dim(config)
    scale = toFloat32(1.0 / sqrt(toFloat(config.linear_key_head_dim)))
    queries = spread(mixed, 0, scale, tiled, rows, config, shapes)
    keys = spread(mixed, key_dim, toFloat32(1.0), tiled, rows, config, shapes)
    return must(gpuDeltaRule(slot.state, queries, keys, values_of(mixed, rows, config), sigmoid(b), decay_of(a, layer, rows, config), config.linear_num_value_heads, config.linear_key_head_dim))


def mix(x, layer, slot, tiled, config, shapes, count) -> Mixed:
    """`Qwen3_5GatedDeltaNet.forward` for the `count` normed rows `x` with the slot's
    state: every projection reads its weight once for all the rows and every stage runs
    over all the positions in one dispatch ([LLM-PREFILL])."""
    hidden = config.hidden_size
    heads = config.linear_num_value_heads
    value_dim = C.value_dim(config)
    rows = rows_of(count, config, shapes)
    qkv = linear(x, layer.qkv, count, hidden, C.conv_dim(config))
    state = recurred(convolved(layer.conv, qkv, slot.conv, rows, config), slot, linear(x, layer.b, count, hidden, heads), linear(x, layer.a, count, hidden, heads), layer, tiled, rows, config, shapes)
    out = slice_of(state, C.state_cells(config), rows.value)
    gated = Norm.gated(out, linear(x, layer.z, count, hidden, value_dim), layer.norm, rows.v_heads, rows.value, shapes.linear_v_dim, config.linear_value_head_dim, toFloat32(config.rms_norm_eps))
    return Mixed(hidden=linear(gated, layer.out, count, value_dim, hidden), slot=Cache.DeltaSlot(conv=window(qkv, slot.conv, count, config, shapes), state=state))
