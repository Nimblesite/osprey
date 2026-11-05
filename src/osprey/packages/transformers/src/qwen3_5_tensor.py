# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Tensor
"""The `qwen3_5` text model's data: parameter tensors, config, KV cache and Result plumbing.

A checkpoint is a flat list of `Tensor` records carrying the fixture's own name,
shape and row-major float32 values, so a layer asks for `layers.3.self_attn.q_proj.weight`
exactly as the real model does. `matrix` and `vector` check the recorded shape against
the width the model expects and return an `Error` when they disagree, so every product
that follows reads whole rows and no result is silently truncated or padded.

`then` and `collect` sequence the `Result`-returning steps a layer is built from, and
`flatten_rows`, `slice_of`, `add_rows` and `dot` are the list operations those steps
share, so no module carries a private copy. Implements [FAILURE-EXPLICIT],
[LLM-MODEL-ATTENTION]'s data, [PORT-TEST-ORACLE].
"""

__all__ = ["Tensor", "Params", "Config", "Matrix", "KvCache", "Step", "shape_size", "shape_text", "tensor", "params_of", "param", "matrix", "vector", "linear", "dot", "at", "slice_of", "flatten_rows", "add_rows", "then", "gather", "empty_cache"]


class Tensor:
    """One parameter: its checkpoint name, its row-major shape and its float32 values."""
    name: str
    shape: list[int]
    values: list[float32]


class Params:
    """Every parameter of the model, in the checkpoint's order."""
    tensors: list[Tensor]


class Config:
    """The `Qwen3_5TextConfig` fields the text model reads: the layer widths, the
    attention head counts (`num_attention_heads` over `num_key_value_heads`, one
    `head_dim` each), the norm's `eps` and the rope base, the partial rotary fraction
    and each layer's `layer_types` entry."""
    hidden_size: int
    num_attention_heads: int
    num_key_value_heads: int
    head_dim: int
    intermediate_size: int
    vocab_size: int
    rms_norm_eps: float32
    rope_theta: float32
    partial_rotary_factor: float32
    layer_types: list[str]


class Matrix:
    """A weight matrix in `nn.Linear`'s `[out, in]` layout: `rows` rows of `cols`
    values, row-major, so `linear` makes one dot product per row."""
    weight: list[float32]
    rows: int
    cols: int


class KvCache:
    """The KV cache, owned and plain: `keys[h]` and `values[h]` hold one head's
    position-major vectors, each `head_dim` values, in the order they were appended."""
    keys: list[list[list[float32]]]
    values: list[list[list[float32]]]


class Step:
    """One layer step's result: its output hidden states and the cache it leaves."""
    hidden: list[list[float32]]
    cache: KvCache


def shape_size(shape) -> int:
    """The element count of one row-major shape."""
    return foldList(shape, 1, lambda total, extent: total * extent)


def shape_text(shape) -> str:
    """A shape as `[64, 128]`, for an error message."""
    return "[" + join(mapList(shape, toString), ", ") + "]"


def tensor(name, shape, values):
    """A tensor whose values fill its shape, or an `Error`: a short or long value list
    is never truncated or padded silently."""
    if shape_size(shape) == len(values):
        return Success(value=Tensor(name=name, shape=shape, values=values))
    else:
        return Error(message=f"tensor {name}: {len(values)} values do not fill shape {shape_text(shape)}")


def params_of(names, shapes, values):
    """A parameter set from three parallel manifests (the fixture's `MODEL_PARAM_*`):
    the checkpoint's names, shapes and float32 values, in its own order."""
    if len(names) != len(shapes) or len(names) != len(values):
        return Error(message=f"parameter manifests differ in length: {len(names)}, {len(shapes)}, {len(values)}")
    else:
        return then(built_from(names, shapes, values), lambda tensors: Success(value=Params(tensors=tensors)))


def built_from(names, shapes, values):
    """The tensors the three manifests describe, in order, or the first `Error`."""
    return fold(range(len(names)), Success(value=[]), lambda acc, k: gather(acc, tensor(unwrap_or(listGet(names, k), ""), unwrap_or(listGet(shapes, k), []), mapList(unwrap_or(listGet(values, k), []), toFloat32))))


def param(ps, name):
    """The parameter called `name`, or an `Error` naming the one the checkpoint lacks."""
    hit = fold(range(len(ps.tensors)), -1, lambda found, i: i if found < 0 and named(ps, i) == name else found)
    if hit < 0:
        return Error(message="missing parameter: " + name)
    else:
        return Success(value=nth(ps, hit))


def named(ps, k):
    """The `k`-th parameter's name."""
    return nth(ps, k).name


def nth(ps, k):
    """The `k`-th parameter, or an empty placeholder read only by `named` for an index
    past the manifest."""
    return unwrap_or(listGet(ps.tensors, k), Tensor(name="", shape=[], values=[]))


def matrix(ps, name, rows, cols):
    """The `[rows, cols]` weight called `name`; a recorded shape that disagrees is an
    `Error` naming both shapes, never a product over the wrong width."""
    match param(ps, name):
        case Success(value=t):
            if structuralEquals(t.shape, [rows, cols]):
                return Success(value=Matrix(weight=t.values, rows=rows, cols=cols))
            else:
                return Error(message="weight " + name + ": shape " + shape_text(t.shape) + ", expected " + shape_text([rows, cols]))
        case Error(message=message):
            return Error(message=message)


def vector(ps, name, width):
    """The `[width]` parameter called `name` (an RMSNorm weight), or an `Error`."""
    match param(ps, name):
        case Success(value=t):
            if structuralEquals(t.shape, [width]):
                return Success(value=t.values)
            else:
                return Error(message="weight " + name + ": shape " + shape_text(t.shape) + ", expected " + shape_text([width]))
        case Error(message=message):
            return Error(message=message)


def at(xs, i) -> float32:
    """`xs[i]`, or 0.0 past the end: the total read the port's flat lists use."""
    return unwrap_or(listGet(xs, i), toFloat32(0.0))


def slice_of(xs, start, width):
    """The `width` values of `xs` starting at `start`."""
    return toList(map(range(width), lambda i: at(xs, start + i)))


def flatten_rows(rows):
    """The rows of a table concatenated in row-major order."""
    return foldList(rows, [], lambda acc, row: listConcat(acc, row))


def dot(x, w, offset) -> float32:
    """One binary32 dot product of `x` with `w` from `offset`, summed left to right."""
    return fold(range(len(x)), toFloat32(0.0), lambda acc, i: acc + at(x, i) * at(w, offset + i))


def linear(x, m):
    """`x * Wᵀ` in `nn.Linear`'s layout: one dot product per row of `m`, whose rows are
    `m.cols` wide and whose `x` carries the `m.cols` values its recorded shape declares."""
    return toList(map(range(m.rows), lambda r: dot(x, m.weight, r * m.cols)))


def add_rows(a, b):
    """Row-wise `a + b`, the residual the decoder adds around each block."""
    return toList(map(range(len(a)), lambda r: add_row(unwrap_or(listGet(a, r), []), unwrap_or(listGet(b, r), []))))


def add_row(x, y):
    """Element-wise `x + y` at binary32."""
    return toList(map(range(len(x)), lambda i: at(x, i) + at(y, i)))


def then(source, next):
    """`next` over a `Success` value, or the same `Error`: the step chain a layer is."""
    match source:
        case Success(value=value):
            return next(value)
        case Error(message=message):
            return Error(message=message)


def gather(built, item):
    """Append one step's `Success` value to the collected list, or keep the first
    `Error`: how a layer's row-by-row steps become one list without a `Result` ever
    sitting in a list."""
    match built:
        case Error(message=message):
            return Error(message=message)
        case Success(value=values):
            match item:
                case Success(value=value):
                    return Success(value=listAppend(values, value))
                case Error(message=message):
                    return Error(message=message)


def empty_cache(config) -> KvCache:
    """A cache with no positions yet: one empty head list per key/value head."""
    return KvCache(keys=empty_heads(config.num_key_value_heads), values=empty_heads(config.num_key_value_heads))


def empty_heads(count):
    """`count` heads, each holding no positions."""
    return toList(map(range(count), lambda _h: empty_head()))


def empty_head() -> list[list[float32]]:
    """One head with no positions stored."""
    return []
