# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Model
"""`Qwen3_5TextModel` and the causal head over stacked token rows, on device buffers.

A `Layer` is `Qwen3_5DecoderLayer`: `input_layernorm`, the token mixer its
`layer_types` entry names, a residual, `post_attention_layernorm`, `Qwen3_5MLP`, a
residual. `forward` threads `count` stacked hidden rows and the cache through every
layer and keeps each layer's output at the last row (`output_hidden_states`); `decode`
is one row, `prefill` a prompt's rows ([LLM-PREFILL]); `normed` is the final norm,
`logits` the tied or separate `lm_head`, `finite` the check that no logit is NaN or
infinite, `greedy` the first index of the largest logit. Every GPU failure ends the step as the `Error` of its `Result` through the
shared `Fault` handler. The embedding row comes from a dense table or straight from a
mapped quantized file, so the 248,320-row table of a real checkpoint is never
expanded. Implements [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE],
[LLM-MODEL-CACHE].
"""

from qwen import Qwen3_5Attention as Attn
from qwen import Qwen3_5Cache as Cache
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5DeltaNet as DeltaNet
from qwen import Qwen3_5Mlp as Mlp
from qwen import Qwen3_5Norm as Norm
from qwen import Qwen3_5Shapes as Shapes
from qwen.Qwen3_5Device import Fault, Weight, added, at, grid, linear, must, row_at, slice_of, stacked, strictly, wrapping

__all__ = ["Mixer", "Embedding", "Layer", "Model", "Walk", "Next", "identity", "embed", "forward", "normed", "logits", "finite", "greedy", "step", "decode", "prefill", "prefill_by", "PREFILL_CHUNK"]


PREFILL_CHUNK = 256
"""Prompt tokens one prefill pass stacks: every weight is read once per chunk, and a
chunk's `[T, intermediate]` activations stay a few tens of megabytes on a real model."""


@union
class Mixer:
    """The token mixer a layer's type names."""
    class Linear:
        layer: DeltaNet.Layer
    class Full:
        layer: Attn.Layer


@union
class Embedding:
    """`embed_tokens`: a dense `[vocab, hidden]` table, or rows of a mapped quantized file."""
    class Table:
        values: GpuBuffer[float32]
    class Mapped:
        path: str
        offset: int
        row_bytes: int
        type: str
        basis: GpuBuffer[float32]


class Layer:
    """`Qwen3_5DecoderLayer`'s parameters; the norms hold `1 + weight`."""
    input_norm: GpuBuffer[float32]
    post_norm: GpuBuffer[float32]
    mixer: Mixer
    mlp: Mlp.Mlp


class Model:
    """`Qwen3_5TextModel` plus `lm_head`; `tiled` is the value-head order its loader found."""
    config: C.Config
    shapes: Shapes.Shapes
    embedding: Embedding
    layers: list[Layer]
    final_norm: GpuBuffer[float32]
    head: Weight
    tiled: int


class Walk:
    """The hidden rows after the layers so far, the cache they left, and each layer's
    output at the last row."""
    hidden: GpuBuffer[float32]
    cache: Cache.Cache
    stages: list[GpuBuffer[float32]]


class Step:
    hidden: GpuBuffer[float32]
    slot: Cache.Slot


class Next:
    """One decoded token: its embedding row, the walk, the final norm, the logits and their argmax."""
    row: GpuBuffer[float32]
    walk: Walk
    normed: GpuBuffer[float32]
    logits: GpuBuffer[float32]
    best: int


def identity(n) -> GpuBuffer[float32]:
    """The `n × n` identity a mapped row is read through."""
    edge = n + 1
    return must(gpuMap(grid(n * n), lambda i: toFloat32(1.0) if i % edge == 0 else toFloat32(0.0)))


def embed(model, token) -> GpuBuffer[float32]:
    """`embed_tokens(token)` as one hidden row."""
    hidden = model.config.hidden_size
    match model.embedding:
        case Table(values=values):
            return slice_of(values, token * hidden, model.shapes.hidden)
        case Mapped(path=path, offset=offset, row_bytes=row_bytes, type=type, basis=basis):
            return mapped_row(path, offset + token * row_bytes, row_bytes, type, basis, hidden)


def mapped_row(path, start, row_bytes, type, basis, hidden):
    match gpuMapFile(path, start, row_bytes):
        case Success(value=row):
            return must(gpuMatMulQuant(basis, row, type, hidden, hidden, 1))
        case Error(message=message):
            return perform(Fault.halt(toString(message)))


def mixed(model, x, mixer, slot, index, count) -> Result[Step, str]:
    """The mixer's output for the `count` normed rows `x`, given the slot of its own kind."""
    match mixer:
        case Linear(layer=layer):
            match slot:
                case Cache.Delta(held=held):
                    return Success(value=delta_step(DeltaNet.mix(x, layer, held, model.tiled, model.config, model.shapes, count)))
                case Cache.Attention(held=_):
                    return Error(message=f"layer {index}: a linear_attention layer was given an attention cache slot")
        case Full(layer=layer):
            match slot:
                case Cache.Attention(held=held):
                    return Success(value=attention_step(Attn.mix(x, layer, held, model.config, model.shapes, count)))
                case Cache.Delta(held=_):
                    return Error(message=f"layer {index}: a full_attention layer was given a linear cache slot")


def delta_step(r) -> Step:
    return Step(hidden=r.hidden, slot=Cache.Delta(held=r.slot))


def attention_step(r) -> Step:
    return Step(hidden=r.hidden, slot=Cache.Attention(held=r.slot))


def layered(model, walk, layer, slot, index, rows) -> Result[Walk, str]:
    """`Qwen3_5DecoderLayer.forward` on the walk's rows."""
    config = model.config
    shapes = model.shapes
    match mixed(model, Norm.rms_rows(walk.hidden, layer.input_norm, config, shapes, rows), layer.mixer, slot, index, rows.count):
        case Error(message=message):
            return Error(message=message)
        case Success(value=step):
            y = added(walk.hidden, step.hidden, rows.hidden)
            out = added(y, Mlp.swiglu(Norm.rms_rows(y, layer.post_norm, config, shapes, rows), layer.mlp, config, rows), rows.hidden)
            last = row_at(out, rows.count - 1, rows.count, config.hidden_size, shapes.hidden)
            return Success(value=Walk(hidden=out, cache=Cache.grown(walk.cache, step.slot), stages=listAppend(walk.stages, last)))


def with_slot(model, walk, layer, cache, index, rows):
    match Cache.slot_at(cache, index):
        case Success(value=slot):
            return layered(model, walk, layer, slot, index, rows)
        case Error(message=message):
            return Error(message=f"layer {index}: no cache slot: {toString(message)}")


def advance(model, cache, acc, index, rows):
    match acc:
        case Error(message=message):
            return Error(message=message)
        case Success(value=walk):
            match listGet(model.layers, index):
                case Success(value=layer):
                    return with_slot(model, walk, layer, cache, index, rows)
                case Error(message=message):
                    return Error(message=f"layer {index}: {toString(message)}")


def forward(model, cache, x, count) -> Result[Walk, str]:
    """Every layer over the `count` embedded rows `x`, reading `cache` and building the next one."""
    rows = Shapes.rows_of(model.config, model.shapes, count)
    start: Result[Walk, str] = Success(value=Walk(hidden=x, cache=Cache.Cache(slots=[]), stages=[]))
    return fold(range(model.config.num_hidden_layers), start, lambda acc, i: advance(model, cache, acc, i, rows))


def normed(model, hidden) -> GpuBuffer[float32]:
    """The final `norm`."""
    return Norm.rms_rows(hidden, model.final_norm, model.config, model.shapes, Shapes.rows_of(model.config, model.shapes, 1))


def logits(model, x) -> GpuBuffer[float32]:
    """`lm_head(x)`: one logit per vocabulary entry."""
    return linear(x, model.head, 1, model.config.hidden_size, model.config.vocab_size)


def chunk_best(values, c, chunk):
    """The first index of chunk `c`'s largest value, as a `float32` (exact below 2^24)."""
    first = wrapMul(c, chunk)
    peak = fold(range(0, chunk), at(values, first), lambda a, i: a if a > at(values, wrapAdd(first, i)) else at(values, wrapAdd(first, i)))
    return fold(range(0, chunk), toFloat32(0.0) - toFloat32(1.0), lambda a, i: toFloat32(wrapAdd(first, i)) if at(values, wrapAdd(first, i)) == peak and a < toFloat32(0.0) else a)


def better(values, winners, c, best):
    i = unwrap_or(floatToInt(at(winners, c)), 0)
    return i if at(values, i) > at(values, best) else best


def greedy(model, values) -> int:
    """`argmax` over the logits: a thread per chunk, then the chunk winners on the host."""
    chunk = model.shapes.vocab_chunk
    count = model.config.vocab_size // chunk
    winners = must(gpuMap(model.shapes.vocab_chunks, lambda c: chunk_best(values, c, chunk)))
    return fold(range(0, count), 0, lambda best, c: better(values, winners, c, best))


def step(model, cache, x, count) -> Result[Walk, str]:
    """`forward` with every device failure caught as the `Error`."""
    with strictly:
        with wrapping:
            return forward(model, cache, x, count)


def decoded(model, cache, token):
    row = embed(model, token)
    match forward(model, cache, row, 1):
        case Success(value=walk):
            return finished(model, row, walk)
        case Error(message=message):
            return Error(message=message)


def chunk_finite(values, c, chunk):
    """1 when every value of chunk `c` is finite, else 0: a NaN fails `x == x`, an
    infinity fails `x - x == 0`."""
    first = wrapMul(c, chunk)
    return fold(range(0, chunk), toFloat32(1.0), lambda a, i: a if at(values, wrapAdd(first, i)) == at(values, wrapAdd(first, i)) and at(values, wrapAdd(first, i)) - at(values, wrapAdd(first, i)) == toFloat32(0.0) else toFloat32(0.0))


def finite(model, values) -> bool:
    """Whether every logit is finite: a thread per chunk, the chunk flags read on the host."""
    chunk = model.shapes.vocab_chunk
    flags = must(gpuMap(model.shapes.vocab_chunks, lambda c: chunk_finite(values, c, chunk)))
    return fold(range(0, model.config.vocab_size // chunk), True, lambda ok, c: ok and at(flags, c) == toFloat32(1.0))


def finished(model, row, walk) -> Result[Next, str]:
    """The step's logits and their argmax, or the refusal when a logit is not finite:
    an argmax over NaN would pick an index from garbage ([LLM-ARITH-IEEE])."""
    x = normed(model, walk.hidden)
    values = logits(model, x)
    if finite(model, values):
        return Success(value=Next(row=row, walk=walk, normed=x, logits=values, best=greedy(model, values)))
    else:
        return Error(message="decode: step logits are not finite")


def decode(model, cache, token) -> Result[Next, str]:
    """One token in: the next cache, the logits of the token after it and their argmax."""
    with strictly:
        with wrapping:
            return decoded(model, cache, token)


def prefill(model, cache, tokens) -> Result[Next, str]:
    """[LLM-PREFILL] A prompt's tokens in one call: the `Next` after the last token,
    with the cache every token built, in chunks of `PREFILL_CHUNK`."""
    return prefill_by(model, cache, tokens, PREFILL_CHUNK)


def prefill_by(model, cache, tokens, chunk) -> Result[Next, str]:
    """`prefill` stacking `chunk` tokens per pass: each pass reads every weight once
    for all its rows, the DeltaNet steps its positions inside one kernel, and only
    attention's cache update steps a position at a time ([LLM-STATE-OWNED])."""
    if chunk <= 0:
        return Error(message="prefill: chunk must be positive")
    else:
        with strictly:
            with wrapping:
                return prefilled(model, cache, tokens, chunk)


def prefilled(model, cache, tokens, chunk):
    if len(tokens) == 0:
        return Error(message="prefill: no tokens")
    else:
        passes = (len(tokens) + chunk - 1) // chunk
        ready = fold(range(0, passes - 1), Success(value=cache), lambda acc, c: repassed(model, tokens, acc, c, chunk))
        match ready:
            case Error(message=message):
                return Error(message=message)
            case Success(value=last):
                return passed(model, last, tokens, (passes - 1) * chunk, chunk)


def repassed(model, tokens, acc, c, chunk):
    """The cache after pass `c`, given the passes before it: nothing reads its logits,
    so only the layers run and the vocabulary head waits for the last pass."""
    match acc:
        case Error(message=message):
            return Error(message=message)
        case Success(value=cache):
            return walked(model, cache, tokens, c * chunk, chunk)


def walked(model, cache, tokens, first, chunk):
    """The cache after one pass over the tokens from `first`, at most `chunk` of them."""
    count = span(tokens, first, chunk)
    match embedded(model, tokens, first, count):
        case Error(message=message):
            return Error(message=message)
        case Success(value=x):
            match forward(model, cache, x, count):
                case Error(message=message):
                    return Error(message=message)
                case Success(value=walk):
                    return Success(value=walk.cache)


def span(tokens, first, chunk):
    """How many tokens a pass from `first` takes: `chunk`, or what is left."""
    return chunk if first + chunk <= len(tokens) else len(tokens) - first


def embedded(model, tokens, first, count) -> Result[GpuBuffer[float32], str]:
    """The `[count, hidden]` embeddings of the tokens from `first`."""
    rows = fold(range(0, count), Success(value=[]), lambda acc, t: embedded_row(model, tokens, first + t, acc))
    match rows:
        case Error(message=message):
            return Error(message=message)
        case Success(value=stack):
            return Success(value=stacked(stack, model.config.hidden_size))


def embedded_row(model, tokens, index, acc):
    match acc:
        case Error(message=message):
            return Error(message=message)
        case Success(value=rows):
            match listGet(tokens, index):
                case Success(value=token):
                    return Success(value=listConcat(rows, [embed(model, token)]))
                case Error(message=message):
                    return Error(message=f"prefill: token {index}: {toString(message)}")


def passed(model, cache, tokens, first, chunk):
    """The last prefill pass over the tokens from `first`: the layers, then the
    vocabulary head at the last row."""
    count = span(tokens, first, chunk)
    hidden = model.config.hidden_size
    match embedded(model, tokens, first, count):
        case Error(message=message):
            return Error(message=message)
        case Success(value=x):
            match forward(model, cache, x, count):
                case Success(value=walk):
                    return finished(model, row_at(x, count - 1, count, hidden, model.shapes.hidden), Walk(hidden=row_at(walk.hidden, count - 1, count, hidden, model.shapes.hidden), cache=walk.cache, stages=walk.stages))
                case Error(message=message):
                    return Error(message=message)
