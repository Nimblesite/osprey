# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Model
"""`Qwen3_5TextModel` and the causal head, one token per step, on device buffers.

A `Layer` is `Qwen3_5DecoderLayer`: `input_layernorm`, the token mixer its
`layer_types` entry names, a residual, `post_attention_layernorm`, `Qwen3_5MLP`, a
residual. `forward` threads one hidden row and the cache through every layer and
keeps each layer's output (`output_hidden_states`); `normed` is the final norm,
`logits` the tied or separate `lm_head`, `greedy` the first index of the largest
logit. Every GPU failure ends the step as the `Error` of its `Result` through the
shared `Fault` handler. The embedding row comes from a dense table or straight from a
mapped quantized file, so the 248,320-row table of a real checkpoint is never
expanded. Implements [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE],
[LLM-MODEL-CACHE].
"""

from transformers import Qwen3_5Attention as Attn
from transformers import Qwen3_5Cache as Cache
from transformers import Qwen3_5Config as C
from transformers import Qwen3_5DeltaNet as DeltaNet
from transformers import Qwen3_5Mlp as Mlp
from transformers import Qwen3_5Norm as Norm
from transformers import Qwen3_5Shapes as Shapes
from transformers.Qwen3_5Device import Fault, Weight, added, at, grid, linear, must, slice_of, strictly, wrapping

__all__ = ["Mixer", "Embedding", "Layer", "Model", "Walk", "Next", "identity", "embed", "forward", "normed", "logits", "greedy", "step", "decode"]


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
    """The hidden row after the layers so far, the cache they left, and each layer's output."""
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


def mapped_row(path, start, row_bytes, type, basis, hidden) -> GpuBuffer[float32]:
    match gpuMapFile(path, start, row_bytes):
        case Success(value=row):
            return must(gpuMatMulQuant(basis, row, type, hidden, hidden, 1))
        case Error(message=message):
            return perform(Fault.halt(toString(message)))


def mixed(model, x, mixer, slot, index) -> Result[Step, str]:
    """The mixer's output for the normed row `x`, given the slot of its own kind."""
    match mixer:
        case Linear(layer=layer):
            match slot:
                case Cache.Delta(held=held):
                    return Success(value=delta_step(DeltaNet.mix(x, layer, held, model.tiled, model.config, model.shapes)))
                case Cache.Attention(held=_):
                    return Error(message=f"layer {index}: a linear_attention layer was given an attention cache slot")
        case Full(layer=layer):
            match slot:
                case Cache.Attention(held=held):
                    return Success(value=attention_step(Attn.mix(x, layer, held, model.config, model.shapes)))
                case Cache.Delta(held=_):
                    return Error(message=f"layer {index}: a full_attention layer was given a linear cache slot")


def delta_step(r) -> Step:
    return Step(hidden=r.hidden, slot=Cache.Delta(held=r.slot))


def attention_step(r) -> Step:
    return Step(hidden=r.hidden, slot=Cache.Attention(held=r.slot))


def layered(model, walk, layer, slot, index) -> Result[Walk, str]:
    """`Qwen3_5DecoderLayer.forward` on the walk's row."""
    config = model.config
    shapes = model.shapes
    match mixed(model, Norm.rms_row(walk.hidden, layer.input_norm, config, shapes), layer.mixer, slot, index):
        case Error(message=message):
            return Error(message=message)
        case Success(value=step):
            y = added(walk.hidden, step.hidden, shapes.hidden)
            out = added(y, Mlp.swiglu(Norm.rms_row(y, layer.post_norm, config, shapes), layer.mlp, config, shapes), shapes.hidden)
            return Success(value=Walk(hidden=out, cache=Cache.grown(walk.cache, step.slot), stages=listAppend(walk.stages, out)))


def with_slot(model, walk, layer, cache, index) -> Result[Walk, str]:
    match Cache.slot_at(cache, index):
        case Success(value=slot):
            return layered(model, walk, layer, slot, index)
        case Error(message=message):
            return Error(message=f"layer {index}: no cache slot: {toString(message)}")


def advance(model, cache, acc, index) -> Result[Walk, str]:
    match acc:
        case Error(message=message):
            return Error(message=message)
        case Success(value=walk):
            match listGet(model.layers, index):
                case Success(value=layer):
                    return with_slot(model, walk, layer, cache, index)
                case Error(message=message):
                    return Error(message=f"layer {index}: {toString(message)}")


def forward(model, cache, row) -> Result[Walk, str]:
    """Every layer over the embedded row, reading `cache` and building the next one."""
    start: Result[Walk, str] = Success(value=Walk(hidden=row, cache=Cache.Cache(slots=[]), stages=[]))
    return fold(range(model.config.num_hidden_layers), start, lambda acc, i: advance(model, cache, acc, i))


def normed(model, hidden) -> GpuBuffer[float32]:
    """The final `norm`."""
    return Norm.rms_row(hidden, model.final_norm, model.config, model.shapes)


def logits(model, x) -> GpuBuffer[float32]:
    """`lm_head(x)`: one logit per vocabulary entry."""
    return linear(x, model.head, 1, model.config.hidden_size, model.config.vocab_size)


def chunk_best(values, c, chunk) -> float32:
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


def step(model, cache, row) -> Result[Walk, str]:
    """`forward` with every device failure caught as the `Error`."""
    with strictly:
        with wrapping:
            return forward(model, cache, row)


def decoded(model, cache, token) -> Result[Next, str]:
    row = embed(model, token)
    match forward(model, cache, row):
        case Success(value=walk):
            return Success(value=finished(model, row, walk))
        case Error(message=message):
            return Error(message=message)


def finished(model, row, walk) -> Next:
    x = normed(model, walk.hidden)
    values = logits(model, x)
    return Next(row=row, walk=walk, normed=x, logits=values, best=greedy(model, values))


def decode(model, cache, token) -> Result[Next, str]:
    """One token in: the next cache, the logits of the token after it and their argmax."""
    with strictly:
        with wrapping:
            return decoded(model, cache, token)
