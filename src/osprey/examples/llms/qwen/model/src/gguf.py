# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=Qwen3_5Gguf
"""A `Qwen3_5TextModel` straight from a GGUF file: weights mapped, never expanded.

`open_file` parses the header with `@osprey/gguf` and refuses any architecture but
`qwen35`; `config_of` reads `Qwen3_5TextConfig` from the `qwen35.*` keys the way
llama.cpp's `convert_hf_to_gguf.py` wrote them (`block_count` less the
`nextn_predict_layers` the text model never runs, `full_attention_interval` naming
every fourth layer `full_attention`, `ssm.group_count`/`time_step_rank`/`state_size`/
`inner_size` as the linear-attention head geometry); `load` maps every tensor through
`gpuMapFile` by its stored type. A quantized matrix stays in its blocks as a `Quant`
weight; an `F32` matrix is transposed on the device to the `[in, out]` layout
`gpuMatMul` reads. Norms arrive already `1 + weight`, `ssm_a` already `-exp(A_log)`,
value heads in the converter's tiled order (`tiled = 1`), so nothing is converted
here. The embedding is `Mapped` when quantized, so a 248,320-row table is read one row
at a time; `output.weight` is the head when present, else the embedding is tied.

A missing tensor, a tensor of the wrong shape, a type the device does not decode, a
truncated file and a foreign architecture each end the load as the named `Error`,
before any token runs ([FAILURE-EXPLICIT]). Implements [LLM-WEIGHTS-MAPPED],
[LLM-WEIGHTS-QUANT], [LLM-MODEL-ORACLE].
"""

from gguf import Chain, Quant, Reader, Tensors
from qwen import Qwen3_5Attention as Attn
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5DeltaNet as DeltaNet
from qwen import Qwen3_5Mlp as Mlp
from qwen import Qwen3_5Model as Model
from qwen import Qwen3_5Shapes as Shapes
from qwen import Qwen3_5Device as Device
from qwen.Qwen3_5Device import at, divider, grid, must, quot, rem, strictly

__all__ = ["ARCHITECTURE", "HEADER_READ", "Source", "Refuse", "refusing", "open_file", "config_of", "load", "load_path"]

ARCHITECTURE = "qwen35"
"""The `general.architecture` llama.cpp's converter writes for `Qwen3_5ForCausalLM`."""
HEADER_READ = 1048576
"""Bytes of the first header read; `Reader.read_path` doubles it until the tensor infos fit."""


class Source:
    """An opened file: its path, for mapping, and its parsed header."""
    path: str
    file: Reader.GGUFFile


@effect
class Refuse:
    """A file the loader cannot serve ends the load: its arm answers the whole region."""
    @control
    def info(message: str) -> Tensors.TensorInfo: ...
    @control
    def number(message: str) -> int: ...
    @control
    def real(message: str) -> float: ...
    @control
    def blocks(message: str) -> GpuBuffer[uint8]: ...


@handler(Refuse)
class refusing:
    """The refusal becomes the `Error` of the load's `Result`."""

    def info(message):
        return Error(message=message)

    def number(message):
        return Error(message=message)

    def real(message):
        return Error(message=message)

    def blocks(message):
        return Error(message=message)


# --- the file -------------------------------------------------------------

def open_file(path) -> Result[Source, str]:
    """The parsed header of the `qwen35` file at `path`, or what is wrong with it."""
    match fileSize(path):
        case Error(message=message):
            return Error(message=f"qwen gguf: {path}: {toString(message)}")
        case Success(value=size):
            return parsed(path, Reader.read_path(path, size, HEADER_READ))


def parsed(path, read) -> Result[Source, str]:
    match read:
        case Error(message=message):
            return Error(message=f"qwen gguf: {path}: {message}")
        case Success(value=file):
            return Chain.then(architecture(file), lambda _name: Success(value=Source(path=path, file=file)))


def architecture(file) -> Result[str, str]:
    """The file's architecture when it is `qwen35`."""
    match Chain.then(Reader.field(file, "general.architecture"), Reader.field_string):
        case Error(message=message):
            return Error(message=f"qwen gguf: {message}")
        case Success(value=name):
            return Success(value=name) if name == ARCHITECTURE else Error(message=f"qwen gguf: architecture `{name}` is not `{ARCHITECTURE}`")


# --- the config -----------------------------------------------------------

def int_of(read):
    match read:
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Refuse.number(f"qwen gguf: {message}"))


def key_int(file, key):
    """The integer under `key`, or the refusal naming what is missing or mistyped."""
    return int_of(Chain.then(Reader.field(file, key), Reader.field_int))


def key_int_or(file, key, fallback):
    """The integer under `key`, or `fallback` when the file has no such key."""
    match Reader.field(file, key):
        case Error():
            return fallback
        case Success(value=field):
            return int_of(Reader.field_int(field))


def key_float(file, key):
    match Chain.then(Reader.field(file, key), Reader.field_float):
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Refuse.real(f"qwen gguf: {message}"))


def positive(key, value):
    """`value` when it is at least one; a width or count of zero divides nothing."""
    return value if value > 0 else perform(Refuse.number(f"qwen gguf: {key} is {value}, not positive"))


def width(file, key):
    """A positive integer under `key`: every width and count the kernels divide by."""
    return positive(key, key_int(file, key))


def layer_types(layers, interval):
    """`full_attention` every `interval`th layer, `linear_attention` between."""
    return toList(map(range(layers), lambda i: C.FULL if (i + 1) % interval == 0 else C.LINEAR))


def vocab_of(file):
    """The vocabulary size: the embedding's row count."""
    return positive("token_embd.weight rows", unwrap_or(listGet(info_of(file, "token_embd.weight").dims, 1), 0))


def config_of(file) -> C.Config:
    """`Qwen3_5TextConfig` from the `qwen35.*` keys; refuses a missing, mistyped or non-positive key."""
    layers = key_int(file, "qwen35.block_count") - key_int_or(file, "qwen35.nextn_predict_layers", 0)
    head_dim = width(file, "qwen35.attention.key_length")
    v_heads = width(file, "qwen35.ssm.time_step_rank")
    return C.Config(
        hidden_size=width(file, "qwen35.embedding_length"),
        intermediate_size=width(file, "qwen35.feed_forward_length"),
        vocab_size=vocab_of(file),
        num_hidden_layers=layers,
        num_attention_heads=width(file, "qwen35.attention.head_count"),
        num_key_value_heads=width(file, "qwen35.attention.head_count_kv"),
        head_dim=head_dim,
        partial_rotary_factor=toFloat(key_int(file, "qwen35.rope.dimension_count")) / toFloat(head_dim),
        rope_theta=key_float(file, "qwen35.rope.freq_base"),
        rms_norm_eps=key_float(file, "qwen35.attention.layer_norm_rms_epsilon"),
        linear_num_key_heads=width(file, "qwen35.ssm.group_count"),
        linear_num_value_heads=v_heads,
        linear_key_head_dim=width(file, "qwen35.ssm.state_size"),
        linear_value_head_dim=positive("qwen35.ssm.inner_size / qwen35.ssm.time_step_rank", width(file, "qwen35.ssm.inner_size") // v_heads),
        linear_conv_kernel_dim=width(file, "qwen35.ssm.conv_kernel"),
        layer_types=layer_types(layers, width(file, "qwen35.full_attention_interval")),
    )


# --- the tensors ----------------------------------------------------------

def info_of(file, name):
    """The tensor named `name`, or the refusal naming the missing one."""
    match listGet(filterList(file.tensors, lambda info: info.name == name), 0):
        case Success(value=info):
            return info
        case Error():
            return perform(Refuse.info(f"qwen gguf: missing tensor `{name}`"))


def has_tensor(file, name):
    return len(filterList(file.tensors, lambda info: info.name == name)) > 0


def dims_text(dims):
    return "[" + join(mapList(dims, toString), ", ") + "]"


def tensor(source, name, dims):
    """The tensor `name` with exactly `dims` (innermost first) in a type the device decodes."""
    info = info_of(source.file, name)
    if not structuralEquals(info.dims, dims):
        return perform(Refuse.info(f"qwen gguf: tensor `{name}` is {dims_text(info.dims)}, not {dims_text(dims)}"))
    elif listContains(Quant.SUPPORTED, info.type):
        return info
    else:
        return perform(Refuse.info(f"qwen gguf: tensor `{name}` is {info.type}, which the device does not decode; expected one of {join(Quant.SUPPORTED, ', ')}"))


def mapped_floats(source, info):
    """An `F32` tensor's values, mapped from the file."""
    return must(gpuMapFile(source.path, info.data_offset, info.n_elements))


def mapped_bytes(source, info):
    """A quantized tensor's blocks, mapped from the file."""
    match gpuMapFile(source.path, info.data_offset, info.n_bytes):
        case Success(value=blocks):
            return blocks
        case Error(message=message):
            return perform(Refuse.blocks(toString(message)))


def transposed(w, rows, cols):
    """`[rows, cols]` row-major on the device as `[cols, rows]` row-major."""
    d = divider(rows)
    mul = d.mul
    shift = d.shift
    return must(gpuMap(grid(rows * cols), lambda j: at(w, wrapAdd(wrapMul(rem(j, mul, shift, rows), cols), quot(j, mul, shift)))))


def linear(source, name, inner, out):
    """An `nn.Linear` weight stored `[out, in]`: blocks as they are, or `F32` as `[in, out]`."""
    info = tensor(source, name, [inner, out])
    if info.type == "F32":
        return Device.Dense(values=transposed(mapped_floats(source, info), out, inner))
    else:
        return Device.Quant(blocks=mapped_bytes(source, info), type=info.type)


def vector(source, name, dims):
    """A parameter the kernels read element-wise, which the file stores as `F32`."""
    info = tensor(source, name, dims)
    if info.type == "F32":
        return mapped_floats(source, info)
    else:
        return perform(Device.Fault.halt(f"qwen gguf: tensor `{name}` is {info.type}, not F32"))


def blk(i, rest):
    return f"blk.{i}.{rest}"


def mlp_of(source, i, config) -> Mlp.Mlp:
    hidden = config.hidden_size
    inner = config.intermediate_size
    return Mlp.Mlp(
        gate=linear(source, blk(i, "ffn_gate.weight"), hidden, inner),
        up=linear(source, blk(i, "ffn_up.weight"), hidden, inner),
        down=linear(source, blk(i, "ffn_down.weight"), inner, hidden),
    )


def delta_of(source, i, config) -> DeltaNet.Layer:
    hidden = config.hidden_size
    heads = config.linear_num_value_heads
    return DeltaNet.Layer(
        qkv=linear(source, blk(i, "attn_qkv.weight"), hidden, C.conv_dim(config)),
        z=linear(source, blk(i, "attn_gate.weight"), hidden, C.value_dim(config)),
        b=linear(source, blk(i, "ssm_beta.weight"), hidden, heads),
        a=linear(source, blk(i, "ssm_alpha.weight"), hidden, heads),
        out=linear(source, blk(i, "ssm_out.weight"), C.value_dim(config), hidden),
        conv=vector(source, blk(i, "ssm_conv1d.weight"), [config.linear_conv_kernel_dim, C.conv_dim(config)]),
        a_neg=vector(source, blk(i, "ssm_a"), [heads]),
        dt_bias=vector(source, blk(i, "ssm_dt.bias"), [heads]),
        norm=vector(source, blk(i, "ssm_norm.weight"), [config.linear_value_head_dim]),
    )


def attention_of(source, i, config) -> Attn.Layer:
    hidden = config.hidden_size
    q_width = config.num_attention_heads * config.head_dim
    kv_width = config.num_key_value_heads * config.head_dim
    return Attn.Layer(
        q=linear(source, blk(i, "attn_q.weight"), hidden, 2 * q_width),
        k=linear(source, blk(i, "attn_k.weight"), hidden, kv_width),
        v=linear(source, blk(i, "attn_v.weight"), hidden, kv_width),
        o=linear(source, blk(i, "attn_output.weight"), q_width, hidden),
        q_norm=vector(source, blk(i, "attn_q_norm.weight"), [config.head_dim]),
        k_norm=vector(source, blk(i, "attn_k_norm.weight"), [config.head_dim]),
    )


def mixer_of(source, i, config):
    if C.is_full(config, i):
        return Model.Full(layer=attention_of(source, i, config))
    else:
        return Model.Linear(layer=delta_of(source, i, config))


def layer_of(source, i, config) -> Model.Layer:
    hidden = config.hidden_size
    return Model.Layer(
        input_norm=vector(source, blk(i, "attn_norm.weight"), [hidden]),
        post_norm=vector(source, blk(i, "post_attention_norm.weight"), [hidden]),
        mixer=mixer_of(source, i, config),
        mlp=mlp_of(source, i, config),
    )


def embedding(source, config):
    """`token_embd.weight`: a dense table when `F32`, else rows read from the mapping."""
    hidden = config.hidden_size
    info = tensor(source, "token_embd.weight", [hidden, config.vocab_size])
    if info.type == "F32":
        return Model.Table(values=mapped_floats(source, info))
    else:
        return Model.Mapped(path=source.path, offset=info.data_offset, row_bytes=info.n_bytes // config.vocab_size, type=info.type, basis=Model.identity(hidden))


def head(source, config):
    """`output.weight`, or the embedding when the file ties them."""
    name = "output.weight" if has_tensor(source.file, "output.weight") else "token_embd.weight"
    return linear(source, name, config.hidden_size, config.vocab_size)


def build(source, config) -> Model.Model:
    return Model.Model(
        config=config,
        shapes=Shapes.shapes_of(config),
        embedding=embedding(source, config),
        layers=toList(map(range(config.num_hidden_layers), lambda i: layer_of(source, i, config))),
        final_norm=vector(source, "output_norm.weight", [config.hidden_size]),
        head=head(source, config),
        tiled=1,
    )


def load(source) -> Result[Model.Model, str]:
    """The model on the device, or the first thing the file lacks."""
    with strictly:
        with refusing:
            return Success(value=build(source, config_of(source.file)))


def load_path(path) -> Result[Model.Model, str]:
    """`open_file` then `load`."""
    return Chain.then(open_file(path), load)
