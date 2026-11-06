# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5DeltaNet
"""`Qwen3_5GatedDeltaNet` for one cached token: the recurrent gated delta rule.

As `Qwen3_5GatedDeltaNet.forward` on the single-token path: `in_proj_qkv` feeds a
depthwise causal convolution over the last `conv_kernel` inputs of every channel
(`causal_conv1d_update`) followed by `silu`; the result splits into queries, keys and
values; `beta = sigmoid(in_proj_b(x))`; the decay is `exp(-exp(A_log) *
softplus(in_proj_a(x) + dt_bias))`; queries and keys are `l2norm`ed per head and each
key head serves `num_v_heads / num_k_heads` value heads (`repeat_interleave`), the
query also scaled by `head_k_dim^-0.5`; `torch_recurrent_gated_delta_rule` updates the
`[num_v_heads, head_k_dim, head_v_dim]` state (`gpuDeltaRule`); the output is gated
by `Qwen3_5RMSNormGated` with `in_proj_z(x)` and projected by `out_proj`.

A GGUF lists the value heads in llama.cpp's tiled order, where value head `t` pairs
with key head `t % num_k_heads`; a checkpoint in Transformers' order pairs `t` with
`t // ratio`. `tiled` is that choice, made once by the loader. Implements
[LLM-MODEL-DELTANET], [LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Cache as Cache
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5Norm as Norm
from qwen.Qwen3_5Device import Weight, at, linear, must, quot, rem, rsqrt, sigmoid, slice_of, softplus, squares

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
    """The layer's output row and the cache slot it leaves."""
    hidden: GpuBuffer[float32]
    slot: Cache.DeltaSlot


def history(qkv, old, config, shapes) -> GpuBuffer[float32]:
    """`conv_states` rolled one step with `qkv` as every channel's newest input."""
    kernel = config.linear_conv_kernel_dim
    mul = shapes.conv_kernel.mul
    shift = shapes.conv_kernel.shift
    last = kernel - 1
    return must(gpuMap(shapes.conv_history, lambda i: at(qkv, quot(i, mul, shift)) if rem(i, mul, shift, kernel) == last else at(old, wrapAdd(i, 1))))


def tap(w, hist, c, kernel) -> float32:
    """Channel `c`'s depthwise convolution over its `kernel` newest inputs."""
    return fold(range(0, kernel), toFloat32(0.0), lambda a, j: a + at(w, wrapAdd(wrapMul(c, kernel), j)) * at(hist, wrapAdd(wrapMul(c, kernel), j)))


def convolved(w, hist, config, shapes) -> GpuBuffer[float32]:
    """`silu(conv1d(history))` for every channel."""
    kernel = config.linear_conv_kernel_dim
    tapped = must(gpuMap(shapes.conv, lambda c: tap(w, hist, c, kernel)))
    gate = sigmoid(tapped)
    return must(gpuMap(shapes.conv, lambda c: at(tapped, c) * at(gate, c)))


def unit_scales(mixed, first, config, shapes) -> GpuBuffer[float32]:
    """`l2norm`'s `rsqrt(sum(x²) + 1e-6)` for every key head from `first`."""
    width = config.linear_key_head_dim
    eps = toFloat32(0.000001)
    return must(gpuMap(shapes.linear_k_heads, lambda h: rsqrt(squares(mixed, wrapAdd(first, wrapMul(h, width)), width) + eps)))


def key_head(t, tiled, km, ks, k_heads, rm, rs):
    """The key head value head `t` reads: `t % num_k_heads` tiled, else `t // ratio`."""
    return rem(t, km, ks, k_heads) if tiled == 1 else quot(t, rm, rs)


def spread_cell(mixed, scales, first, i, kh, d, width, factor: float32) -> float32:
    return at(mixed, wrapAdd(first, wrapAdd(wrapMul(kh, width), d))) * at(scales, kh) * factor


def spread(mixed, first, factor: float32, tiled, config, shapes) -> GpuBuffer[float32]:
    """Unit-length key-head rows from `first`, one per value head, times `factor`."""
    width = config.linear_key_head_dim
    k_heads = config.linear_num_key_heads
    scales = unit_scales(mixed, first, config, shapes)
    dm = shapes.linear_k_dim.mul
    ds = shapes.linear_k_dim.shift
    km = shapes.linear_k_heads_div.mul
    ks = shapes.linear_k_heads_div.shift
    rm = shapes.linear_ratio.mul
    rs = shapes.linear_ratio.shift
    return must(gpuMap(shapes.value, lambda i: spread_cell(mixed, scales, first, i, key_head(quot(i, dm, ds), tiled, km, ks, k_heads, rm, rs), rem(i, dm, ds, width), width, factor)))


def decay_of(a, layer, shapes) -> GpuBuffer[float32]:
    """`exp(-exp(A_log) * softplus(a + dt_bias))` per value head."""
    a_neg = layer.a_neg
    dt = layer.dt_bias
    return must(gpuMap(shapes.linear_v_heads, lambda h: exp(at(a_neg, h) * softplus(at(a, h) + at(dt, h)))))


def recurred(mixed, old, b, a, layer, tiled, config, shapes) -> GpuBuffer[float32]:
    """The state after `torch_recurrent_gated_delta_rule` on this token, outputs appended."""
    key_dim = C.key_dim(config)
    scale = toFloat32(1.0 / sqrt(toFloat(config.linear_key_head_dim)))
    queries = spread(mixed, 0, scale, tiled, config, shapes)
    keys = spread(mixed, key_dim, toFloat32(1.0), tiled, config, shapes)
    values = slice_of(mixed, 2 * key_dim, shapes.value)
    return must(gpuDeltaRule(old, queries, keys, values, sigmoid(b), decay_of(a, layer, shapes), config.linear_num_value_heads, config.linear_key_head_dim))


def mix(x, layer, slot, tiled, config, shapes) -> Mixed:
    """`Qwen3_5GatedDeltaNet.forward` for the normed row `x` with the slot's state."""
    hidden = config.hidden_size
    heads = config.linear_num_value_heads
    qkv = linear(x, layer.qkv, 1, hidden, C.conv_dim(config))
    z = linear(x, layer.z, 1, hidden, C.value_dim(config))
    b = linear(x, layer.b, 1, hidden, heads)
    a = linear(x, layer.a, 1, hidden, heads)
    hist = history(qkv, slot.conv, config, shapes)
    state = recurred(convolved(layer.conv, hist, config, shapes), slot.state, b, a, layer, tiled, config, shapes)
    out = slice_of(state, C.state_cells(config), shapes.value)
    gated = Norm.gated(out, z, layer.norm, shapes.linear_v_heads, shapes.value, shapes.linear_v_dim, config.linear_value_head_dim, toFloat32(config.rms_norm_eps))
    return Mixed(hidden=linear(gated, layer.out, 1, C.value_dim(config), hidden), slot=Cache.DeltaSlot(conv=hist, state=state))
