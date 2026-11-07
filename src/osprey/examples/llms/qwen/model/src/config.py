# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Config
"""`Qwen3_5TextConfig`: the fields the text model reads, and the widths derived from them.

The names follow the configuration class in `transformers`' `configuration_qwen3_5.py`,
so a loader fills them from the same source a checkpoint carries: `tokenizer.json`'s
sibling `config.json`, or the `qwen35.*` keys of a GGUF (`Qwen3_5Gguf`). The derived
widths are the ones `Qwen3_5GatedDeltaNet.__init__` and `Qwen3_5Attention.__init__`
compute: `key_dim`, `value_dim`, `conv_dim`, the kv-group size and the rotary width.
Implements [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE].
"""

__all__ = ["Config", "LINEAR", "FULL", "layer_type", "key_dim", "value_dim", "conv_dim", "state_cells", "kv_groups", "rotary_dim", "is_full"]

LINEAR = "linear_attention"
"""`Qwen3_5TextConfig.layer_types`' entry naming a linear-attention layer."""
FULL = "full_attention"
"""`Qwen3_5TextConfig.layer_types`' entry naming a full-attention layer."""


class Config:
    """`Qwen3_5TextConfig`'s text-model fields."""
    hidden_size: int
    intermediate_size: int
    vocab_size: int
    num_hidden_layers: int
    num_attention_heads: int
    num_key_value_heads: int
    head_dim: int
    partial_rotary_factor: float
    rope_theta: float
    rms_norm_eps: float
    linear_num_key_heads: int
    linear_num_value_heads: int
    linear_key_head_dim: int
    linear_value_head_dim: int
    linear_conv_kernel_dim: int
    layer_types: list[str]


def layer_type(config, index):
    """`config.layer_types[index]`, or `""` past the end."""
    return unwrap_or(listGet(config.layer_types, index), "")


def is_full(config, index):
    """Whether layer `index` is a `full_attention` layer, and everything else linear."""
    return layer_type(config, index) == FULL


def key_dim(config):
    """`head_k_dim * num_k_heads`: the width of the queries, and of the keys."""
    return config.linear_key_head_dim * config.linear_num_key_heads


def value_dim(config):
    """`head_v_dim * num_v_heads`: the width of the values, the gate and the mixer output."""
    return config.linear_value_head_dim * config.linear_num_value_heads


def conv_dim(config):
    """`key_dim * 2 + value_dim`: the channels the causal convolution runs over."""
    return key_dim(config) * 2 + value_dim(config)


def state_cells(config):
    """The recurrent state's `[num_v_heads, head_k_dim, head_v_dim]` cells."""
    return config.linear_num_value_heads * config.linear_key_head_dim * config.linear_value_head_dim


def kv_groups(config):
    """`num_attention_heads // num_key_value_heads`: query heads per key/value head."""
    return config.num_attention_heads // config.num_key_value_heads


def rotary_dim(config):
    """`int(head_dim * partial_rotary_factor)`: the rotated prefix of each head."""
    return unwrap_or(floatToInt(toFloat(config.head_dim) * config.partial_rotary_factor), 0)
