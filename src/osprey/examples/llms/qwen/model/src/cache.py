# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Cache
"""`Qwen3_5DynamicCache`: the per-layer state one token leaves for the next.

A `linear_attention` layer keeps its `conv_states` (the last `conv_kernel` inputs of
every channel, newest last, as `causal_conv1d_update` rolls them) and its
`recurrent_states` (`[num_v_heads, head_k_dim, head_v_dim]`, followed by the step's
outputs as `gpuDeltaRule` lays them out). A `full_attention` layer keeps its keys and
values, position-major, `num_key_value_heads * head_dim` wide per position. One slot
per layer, in layer order; the cache is a plain value a step returns grown, and the
caller threads it, until [LLM-STATE-OWNED] gives it a single owner. Implements
[LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Config as C
from qwen.Qwen3_5Device import strictly, zeros

__all__ = ["DeltaSlot", "AttentionSlot", "Slot", "Cache", "empty", "slot_at", "grown"]


class DeltaSlot:
    """One Gated DeltaNet layer's `conv_states` and `recurrent_states`."""
    conv: GpuBuffer[float32]
    state: GpuBuffer[float32]


class AttentionSlot:
    """One attention layer's cached keys and values, `positions` of them: the first
    `sealed` rows in `sealed_keys`/`sealed_values`, rewritten once per `kv_block`
    positions, and the rest in the tail `keys`/`values` a token rewrites, so a token
    copies at most a block of rows, not the whole cache ([LLM-STATE-OWNED])."""
    keys: GpuBuffer[float32]
    values: GpuBuffer[float32]
    positions: int
    sealed_keys: GpuBuffer[float32]
    sealed_values: GpuBuffer[float32]
    sealed: int


@union
class Slot:
    """A layer's state, of the kind its layer type keeps."""
    class Delta:
        held: DeltaSlot
    class Attention:
        held: AttentionSlot


class Cache:
    """Every layer's slot, in layer order."""
    slots: list[Slot]


def empty(config, shapes) -> Result[Cache, str]:
    """The cache before the first token: zero states, no positions."""
    with strictly:
        return Success(value=fresh(config, shapes))


def fresh(config, shapes) -> Cache:
    conv = zeros(shapes.conv_history)
    state = zeros(shapes.state)
    none = zeros(shapes.one)
    return Cache(slots=toList(map(range(config.num_hidden_layers), lambda i: fresh_slot(config, i, conv, state, none))))


def fresh_slot(config, index, conv, state, none):
    if C.is_full(config, index):
        return Attention(held=AttentionSlot(keys=none, values=none, positions=0, sealed_keys=none, sealed_values=none, sealed=0))
    else:
        return Delta(held=DeltaSlot(conv=conv, state=state))


def slot_at(cache, index) -> Result[Slot, Error]:
    """Layer `index`'s slot."""
    return listGet(cache.slots, index)


def grown(cache, slot) -> Cache:
    """The cache with the next layer's slot appended, layers arriving in order."""
    return Cache(slots=listAppend(cache.slots, slot))
