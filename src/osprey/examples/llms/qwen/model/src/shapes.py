# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=Qwen3_5Shapes
"""The index grids and reciprocals one `Qwen3_5TextConfig` needs, built once per model.

A kernel maps over an index buffer and reads its operands by position, so every
distinct extent the model loops over (`hidden_size`, the convolution channels, a
head's cells, the attention heads, …) is a `gpuIota` built here and kept on the
device, and every width a kernel divides by is a `Div` ([`Qwen3_5Device`]). Nothing
here depends on the weights. Implements [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION].
"""

from qwen import Qwen3_5Config as C
from qwen.Qwen3_5Device import Div, divider, grid

__all__ = ["Shapes", "Rows", "shapes_of", "rows_of", "NORM_CHUNK", "VOCAB_CHUNK", "chunk_of"]

NORM_CHUNK = 256
"""Values summed per thread when a norm reduces a hidden row."""
VOCAB_CHUNK = 512
"""Logits scanned per thread when the argmax reduces the vocabulary."""


class Shapes:
    """Every grid and reciprocal the layers index with."""
    one: GpuBuffer[int]
    hidden: GpuBuffer[int]
    hidden_chunks: GpuBuffer[int]
    norm_chunk: int
    intermediate: GpuBuffer[int]
    conv: GpuBuffer[int]
    conv_history: GpuBuffer[int]
    value: GpuBuffer[int]
    linear_k_heads: GpuBuffer[int]
    linear_v_heads: GpuBuffer[int]
    state: GpuBuffer[int]
    q_cells: GpuBuffer[int]
    kv_cells: GpuBuffer[int]
    q_heads: GpuBuffer[int]
    kv_heads: GpuBuffer[int]
    vocab_chunks: GpuBuffer[int]
    vocab_chunk: int
    hidden_div: Div
    head_dim: Div
    linear_k_dim: Div
    linear_v_dim: Div
    attention_heads: Div
    kv_groups: Div
    linear_k_heads_div: Div
    linear_ratio: Div
    conv_kernel: Div


class Rows:
    """[LLM-PREFILL] The grids of `count` stacked hidden rows: the per-row operations
    (norms, the MLP, residuals) map over these, so a prompt's rows share one dispatch."""
    count: int
    rows: GpuBuffer[int]
    hidden: GpuBuffer[int]
    hidden_chunks: GpuBuffer[int]
    intermediate: GpuBuffer[int]


def rows_of(config, shapes, count) -> Rows:
    """The grids for `count` rows; one row reuses the model's own."""
    if count == 1:
        return Rows(count=1, rows=shapes.one, hidden=shapes.hidden, hidden_chunks=shapes.hidden_chunks, intermediate=shapes.intermediate)
    else:
        return Rows(count=count, rows=grid(count), hidden=grid(count * config.hidden_size), hidden_chunks=grid(count * (config.hidden_size // shapes.norm_chunk)), intermediate=grid(count * config.intermediate_size))


def chunk_of(width, preferred):
    """`preferred` when it divides `width`, else the whole width in one piece."""
    return preferred if width % preferred == 0 else width


def shapes_of(config) -> Shapes:
    """The grids and reciprocals for `config`, allocated on the device."""
    norm_chunk = chunk_of(config.hidden_size, NORM_CHUNK)
    vocab_chunk = chunk_of(config.vocab_size, VOCAB_CHUNK)
    return Shapes(
        one=grid(1),
        hidden=grid(config.hidden_size),
        hidden_chunks=grid(config.hidden_size // norm_chunk),
        norm_chunk=norm_chunk,
        intermediate=grid(config.intermediate_size),
        conv=grid(C.conv_dim(config)),
        conv_history=grid(C.conv_dim(config) * config.linear_conv_kernel_dim),
        value=grid(C.value_dim(config)),
        linear_k_heads=grid(config.linear_num_key_heads),
        linear_v_heads=grid(config.linear_num_value_heads),
        state=grid(C.state_cells(config) + C.value_dim(config)),
        q_cells=grid(config.num_attention_heads * config.head_dim),
        kv_cells=grid(config.num_key_value_heads * config.head_dim),
        q_heads=grid(config.num_attention_heads),
        kv_heads=grid(config.num_key_value_heads),
        vocab_chunks=grid(config.vocab_size // vocab_chunk),
        vocab_chunk=vocab_chunk,
        hidden_div=divider(config.hidden_size),
        head_dim=divider(config.head_dim),
        linear_k_dim=divider(config.linear_key_head_dim),
        linear_v_dim=divider(config.linear_value_head_dim),
        attention_heads=divider(config.num_attention_heads),
        kv_groups=divider(C.kv_groups(config)),
        linear_k_heads_div=divider(config.linear_num_key_heads),
        linear_ratio=divider(config.linear_num_value_heads // config.linear_num_key_heads),
        conv_kernel=divider(config.linear_conv_kernel_dim),
    )
