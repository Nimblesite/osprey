# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Attention
"""`Qwen3_5Attention` and `Qwen3_5DecoderLayer` for a full-attention layer.

`q_proj` writes two head-blocks per head — the query first, the output gate second —
and each query is RMS-normalized over its own `head_dim` with `q_norm`, so the gate and
the norm are per head, not per hidden row. Keys get `k_norm` the same way, values do
not, and both are rotated by the partial rotary table before the new position joins the
cache. `repeat_kv` maps query head `h` onto key/value head `h // (heads / kv_heads)`.
Attention is the eager form: `softmax(q · k * head_dim ** -0.5)` over the cached
positions, then the weighted sum of their value vectors. The heads' outputs are
scaled by `sigmoid(gate)` before `o_proj`, and the decoder layer wraps the block in the
usual pair of residuals with a post-attention norm into the SwiGLU MLP.

Every step reads the layer input's hidden states, the parameters by name, the number of
positions already cached (`start`) and the owned `KvCache`, and returns the layer's
output rows and the grown cache. A batched call appends one position per row in order,
so row `t` attends to positions `0 ..< start + t + 1`. Implements [LLM-MODEL-ATTENTION],
[LLM-MODEL-CACHE], [FAILURE-EXPLICIT].
"""

__all__ = ["Heads", "Projections", "self_attention", "feed_forward", "layer", "projections", "heads_of", "gated_head", "kv_head", "softmax", "weighted_values"]

from transformers.Qwen3_5Mlp import swiglu
from transformers.Qwen3_5Norm import rms_norm, rms_rows
from transformers.Qwen3_5Rope import rotate, tables
from transformers.Qwen3_5Tensor import KvCache, Matrix, Step, add_rows, at, dot, flatten_rows, linear, matrix, slice_of, then, vector


class Projections:
    """One attention block's shape-checked weights: the four projections and the two
    per-head norm weights."""
    q_proj: Matrix
    k_proj: Matrix
    v_proj: Matrix
    o_proj: Matrix
    q_norm: list[float32]
    k_norm: list[float32]


class Heads:
    """One token's projected heads, head-major: `queries[h]` and `keys[h]` are
    normalized and rotated, `values[h]` is the raw value projection, and `gate[h]` is
    `q_proj`'s second half for that head."""
    queries: list[list[float32]]
    keys: list[list[float32]]
    values: list[list[float32]]
    gate: list[list[float32]]


def self_attention(rows, params, config, index, start, cache):
    """Layer `index`'s gated attention over `rows`, whose first row is at position
    `start`: the mixed output rows and the cache with every row's keys and values."""
    return then(tables(start, len(rows), config), lambda table: then(projections(params, config, index), lambda weights: Success(value=attend(project(rows, weights, config, table), weights.o_proj, config, cache))))


def projections(params, config, index):
    """Layer `index`'s attention weights, every shape checked against the config."""
    prefix = "layers." + toString(index) + ".self_attn."
    queries = config.num_attention_heads * config.head_dim
    shared = config.num_key_value_heads * config.head_dim
    return then(matrix(params, prefix + "q_proj.weight", 2 * queries, config.hidden_size), lambda q_proj: then(matrix(params, prefix + "k_proj.weight", shared, config.hidden_size), lambda k_proj: then(matrix(params, prefix + "v_proj.weight", shared, config.hidden_size), lambda v_proj: then(matrix(params, prefix + "o_proj.weight", config.hidden_size, queries), lambda o_proj: then(vector(params, prefix + "q_norm.weight", config.head_dim), lambda q_norm: then(vector(params, prefix + "k_norm.weight", config.head_dim), lambda k_norm: Success(value=Projections(q_proj=q_proj, k_proj=k_proj, v_proj=v_proj, o_proj=o_proj, q_norm=q_norm, k_norm=k_norm))))))))



def project(rows, weights, config, table):
    """Every row's heads, with position `t`'s rotary row."""
    return toList(map(range(len(rows)), lambda t: heads_of(unwrap_or(listGet(rows, t), []), weights, config, unwrap_or(listGet(table.cos, t), []), unwrap_or(listGet(table.sin, t), []))))


def heads_of(row, weights, config, cos, sin) -> Heads:
    """`torch.chunk(q_proj(x).view(heads, 2 * head_dim), 2)` and the three
    projections, normalized and rotated exactly as `Qwen3_5Attention.forward` does."""
    gated = linear(row, weights.q_proj)
    keys = linear(row, weights.k_proj)
    values = linear(row, weights.v_proj)
    return Heads(
        queries=toList(map(range(config.num_attention_heads), lambda h: query_head(gated, h, weights, config, cos, sin))),
        keys=toList(map(range(config.num_key_value_heads), lambda h: rotate(rms_norm(slice_of(keys, h * config.head_dim, config.head_dim), weights.k_norm, config.rms_norm_eps), cos, sin))),
        values=toList(map(range(config.num_key_value_heads), lambda h: slice_of(values, h * config.head_dim, config.head_dim))),
        gate=toList(map(range(config.num_attention_heads), lambda h: slice_of(gated, h * 2 * config.head_dim + config.head_dim, config.head_dim))),
    )


def query_head(gated, h, weights, config, cos, sin):
    """One head's query: `q_norm` over its own slice, then the partial rotation."""
    return rotate(rms_norm(slice_of(gated, h * 2 * config.head_dim, config.head_dim), weights.q_norm, config.rms_norm_eps), cos, sin)


def attend(heads_list, o_proj, config, cache) -> Step:
    """Fold the batch into the cache, one token at a time, mixing each token's heads
    over every position cached so far."""
    folded = fold(range(len(heads_list)), Step(hidden=[], cache=cache), lambda acc, t: append_token(acc, unwrap_or(listGet(heads_list, t), no_heads()), o_proj, config))
    return folded


def no_heads() -> Heads:
    """A token with no heads, read only for an index past the batch."""
    return Heads(queries=[], keys=[], values=[], gate=[])


def append_token(acc, heads, o_proj, config) -> Step:
    """Append this token's keys and values, then mix its heads against the grown cache."""
    grown = KvCache(keys=append_heads(acc.cache.keys, heads.keys), values=append_heads(acc.cache.values, heads.values))
    mixed = flatten_rows(toList(map(range(len(heads.queries)), lambda h: gated_head(heads, h, grown, config))))
    return Step(hidden=listAppend(acc.hidden, linear(mixed, o_proj)), cache=grown)


def append_heads(per_head, fresh):
    """Each head's positions with this token's vector appended."""
    return toList(map(range(len(per_head)), lambda h: listAppend(unwrap_or(listGet(per_head, h), []), unwrap_or(listGet(fresh, h), []))))


def gated_head(heads, h, cache, config):
    """One head's attended vector scaled by `sigmoid(gate)`: `attn_output * sigmoid(gate)`
    before `o_proj` sees the concatenation."""
    kv = kv_head(h, config)
    query = unwrap_or(listGet(heads.queries, h), [])
    weights = softmax(scores(query, unwrap_or(listGet(cache.keys, kv), []), config))
    attended = weighted_values(weights, unwrap_or(listGet(cache.values, kv), []))
    gate = unwrap_or(listGet(heads.gate, h), [])
    return toList(map(range(len(attended)), lambda i: at(attended, i) * sigmoid(at(gate, i))))


def kv_head(h, config) -> int:
    """`repeat_kv`'s mapping: query head `h` reads key/value head
    `h // (num_attention_heads / num_key_value_heads)`."""
    return h // (config.num_attention_heads // config.num_key_value_heads)


def scores(query, keys, config):
    """`q · k * head_dim ** -0.5` for every cached key vector."""
    scale = pow(toFloat32(config.head_dim), toFloat32(0.0) - toFloat32(0.5))
    return mapList(keys, lambda key: dot(query, key, 0) * scale)


def softmax(scores_row):
    """`F.softmax(scores, dim=-1)` over one row: subtract the maximum, exponentiate,
    divide by the sum, so an all-equal row is uniform and no exponent overflows."""
    if len(scores_row) == 0:
        return scores_row
    else:
        top = foldList(scores_row, at(scores_row, 0), lambda best, x: x if x > best else best)
        exps = mapList(scores_row, lambda x: exp(x - top))
        total = foldList(exps, toFloat32(0.0), lambda acc, e: acc + e)
        return mapList(exps, lambda e: e / total)


def weighted_values(weights, values):
    """The weighted sum of the cached value vectors, one output component at a time."""
    width = len(unwrap_or(listGet(values, 0), []))
    return toList(map(range(width), lambda i: fold(range(len(weights)), toFloat32(0.0), lambda acc, j: acc + at(weights, j) * at(unwrap_or(listGet(values, j), []), i))))


def sigmoid(z):
    """`1 / (1 + exp(-z))`, binary32, as torch's `sigmoid` computes it."""
    return toFloat32(1.0) / (toFloat32(1.0) + exp(toFloat32(0.0) - z))


def feed_forward(rows, params, config, index):
    """The decoder layer's second half: post-attention norm, SwiGLU MLP, residual."""
    return then(norm_rows_for(rows, params, config, "post_attention_layernorm.weight", index), lambda normed: then(swiglu(normed, params, config, index), lambda out: Success(value=add_rows(rows, out))))


def layer(rows, params, config, index, start, cache):
    """One full-attention decoder layer: input norm, gated attention, residual, then the
    post-attention norm and MLP with their residual. Returns the output rows and the
    grown cache [LLM-MODEL-CACHE]."""
    return then(norm_rows_for(rows, params, config, "input_layernorm.weight", index), lambda normed: then(self_attention(normed, params, config, index, start, cache), lambda mixed: then(feed_forward(add_rows(rows, mixed.hidden), params, config, index), lambda out: Success(value=Step(hidden=out, cache=mixed.cache)))))


def norm_rows_for(rows, params, config, field, index):
    """A decoder layer's RMSNorm over a batch: `self.<field>` of layer `index`."""
    return then(vector(params, "layers." + toString(index) + "." + field, config.hidden_size), lambda weight: Success(value=rms_rows(rows, weight, config.rms_norm_eps)))
