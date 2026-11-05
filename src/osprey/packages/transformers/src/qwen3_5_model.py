# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Model
"""The `qwen3_5` text model's embedding, final norm and per-layer dispatch.

`embed` is `nn.Embedding`'s lookup: one row of `embed_tokens.weight` per token id, and a
token id outside the vocabulary is an `Error` rather than a wrapped or zero row. The
final norm is the same zero-centered RMSNorm the layers use, over the last layer's
output. `layer` dispatches on `config.layer_types` exactly as `Qwen3_5DecoderLayer`
does: the `full_attention` entries run `Qwen3_5Attention.layer`, and the
`linear_attention` entries (Gated DeltaNet) belong to another slice of the port, so they
report that they are not ported instead of returning an activation that would only be
wrong. Implements [LLM-MODEL-ATTENTION], [LLM-MODEL-CACHE], [FAILURE-EXPLICIT].
"""

__all__ = ["embed", "final_norm", "layer", "row_of"]

from transformers import Qwen3_5Attention
from transformers.Qwen3_5Norm import rms_rows
from transformers.Qwen3_5Tensor import Matrix, Step, gather, matrix, slice_of, then, vector


def embed(params, config, tokens):
    """`embed_tokens(input_ids)`: one embedding row per token id, in order."""
    return then(matrix(params, "embed_tokens.weight", config.vocab_size, config.hidden_size), lambda table: embed_rows(table, tokens, config))


def embed_rows(table, tokens, config):
    """One embedding row per token, in order, or the first token id's `Error`."""
    return fold(range(len(tokens)), Success(value=[]), lambda acc, t: gather(acc, row_of(table, unwrap_or(listGet(tokens, t), 0), config)))


def row_of(table, token, config):
    """The embedding row for one token id, or an `Error` naming an id the vocabulary
    does not hold."""
    if token < 0 or token >= config.vocab_size:
        return Error(message=f"token id {token} outside the {config.vocab_size}-token vocabulary")
    else:
        return Success(value=slice_of(table.weight, token * config.hidden_size, config.hidden_size))


def final_norm(rows, params, config):
    """`self.norm` over the last layer's output rows."""
    return then(vector(params, "norm.weight", config.hidden_size), lambda weight: Success(value=rms_rows(rows, weight, config.rms_norm_eps)))


def layer(rows, params, config, index, start, cache):
    """Decoder layer `index` over `rows`, whose first row is at position `start`, with
    the owned KV cache. A `linear_attention` layer returns an `Error`: the Gated
    DeltaNet block is not part of this slice of the port."""
    kind = unwrap_or(listGet(config.layer_types, index), "")
    if kind == "full_attention":
        return Qwen3_5Attention.layer(rows, params, config, index, start, cache)
    elif kind == "linear_attention":
        return Error(message="layer " + toString(index) + ": linear_attention is not ported")
    else:
        return Error(message="layer " + toString(index) + ": unknown layer type " + kind)
