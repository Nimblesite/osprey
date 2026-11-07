# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Hf
"""A `Qwen3_5TextModel` from parameters named as Transformers' `state_dict` names them.

The source is the parameter list of `Qwen3_5TextModel` (`embed_tokens.weight`,
`layers.N.linear_attn.*`, `layers.N.self_attn.*`, `layers.N.mlp.*`, the two layer norms,
`norm.weight`), each a row-major `float32` tensor. On the way to the device:
`nn.Linear` weights `[out, in]` are transposed to `[in, out]` for `gpuMatMul`;
zero-centred norms become `1 + weight`; `A_log` becomes `-exp(A_log)`; everything
else is copied as is, including the conv weights `[channels, 1, kernel]` and the gated
norm. Value heads keep Transformers' grouped order (`tiled = 0`). A missing or
mis-sized parameter ends the load as the `Error`. Implements [LLM-MODEL-ORACLE].
"""

from qwen import Qwen3_5Attention as Attn
from qwen import Qwen3_5Config as C
from qwen import Qwen3_5DeltaNet as DeltaNet
from qwen import Qwen3_5Mlp as Mlp
from qwen import Qwen3_5Model as Model
from qwen import Qwen3_5Shapes as Shapes
from qwen import Qwen3_5Device as Device
from qwen.Qwen3_5Device import Fault, constant, strictly

__all__ = ["Params", "load", "transposed", "plus_one", "neg_exp"]


class Params:
    """The checkpoint's tensors: `values[i]` is the row-major data of `names[i]`."""
    names: list[str]
    values: list[list[float32]]


def index_of(names, name):
    return fold(range(len(names)), -1, lambda hit, i: i if hit < 0 and unwrap_or(listGet(names, i), "") == name else hit)


def sized(params, name, count):
    """The `count` values of `name`, or the fault that ends the load."""
    match listGet(params.values, index_of(params.names, name)):
        case Error(message=_):
            return perform(Fault.halt_values(f"parameter {name} is not in the checkpoint"))
        case Success(value=values):
            if len(values) == count:
                return values
            else:
                return perform(Fault.halt_values(f"parameter {name} holds {len(values)} values, not {count}"))


def transposed(values, rows, cols):
    """`[rows, cols]` row-major as `[cols, rows]` row-major."""
    return toList(map(range(rows * cols), lambda j: unwrap_or(listGet(values, (j % rows) * cols + j // rows), toFloat32(0.0))))


def plus_one(values):
    """Every norm weight plus one: the zero-centred `weight` as the `1 + weight` the kernels multiply."""
    return mapList(values, lambda x: x + toFloat32(1.0))


def neg_exp(values):
    """Every `A_log` entry as `-exp` of itself: the decay's `-exp(A_log)` factor, pre-negated."""
    return mapList(values, lambda x: toFloat32(0.0) - exp(x))


def dense(params, name, rows, cols):
    """An `nn.Linear` weight `[rows=out, cols=in]` as a `Dense` operand."""
    return Device.Dense(values=constant(transposed(sized(params, name, rows * cols), rows, cols)))


def buffer(params, name, count):
    return constant(sized(params, name, count))


def centred(params, name, count):
    """A zero-centred norm weight as the `1 + weight` the kernels multiply by."""
    return constant(plus_one(sized(params, name, count)))


def mlp_of(params, i, config) -> Mlp.Mlp:
    hidden = config.hidden_size
    inner = config.intermediate_size
    return Mlp.Mlp(
        gate=dense(params, f"layers.{i}.mlp.gate_proj.weight", inner, hidden),
        up=dense(params, f"layers.{i}.mlp.up_proj.weight", inner, hidden),
        down=dense(params, f"layers.{i}.mlp.down_proj.weight", hidden, inner),
    )


def delta_of(params, i, config) -> DeltaNet.Layer:
    hidden = config.hidden_size
    heads = config.linear_num_value_heads
    p = f"layers.{i}.linear_attn."
    return DeltaNet.Layer(
        qkv=dense(params, p + "in_proj_qkv.weight", C.conv_dim(config), hidden),
        z=dense(params, p + "in_proj_z.weight", C.value_dim(config), hidden),
        b=dense(params, p + "in_proj_b.weight", heads, hidden),
        a=dense(params, p + "in_proj_a.weight", heads, hidden),
        out=dense(params, p + "out_proj.weight", hidden, C.value_dim(config)),
        conv=buffer(params, p + "conv1d.weight", C.conv_dim(config) * config.linear_conv_kernel_dim),
        a_neg=constant(neg_exp(sized(params, p + "A_log", heads))),
        dt_bias=buffer(params, p + "dt_bias", heads),
        norm=buffer(params, p + "norm.weight", config.linear_value_head_dim),
    )


def attention_of(params, i, config) -> Attn.Layer:
    hidden = config.hidden_size
    q_width = config.num_attention_heads * config.head_dim
    kv_width = config.num_key_value_heads * config.head_dim
    p = f"layers.{i}.self_attn."
    return Attn.Layer(
        q=dense(params, p + "q_proj.weight", 2 * q_width, hidden),
        k=dense(params, p + "k_proj.weight", kv_width, hidden),
        v=dense(params, p + "v_proj.weight", kv_width, hidden),
        o=dense(params, p + "o_proj.weight", hidden, q_width),
        q_norm=centred(params, p + "q_norm.weight", config.head_dim),
        k_norm=centred(params, p + "k_norm.weight", config.head_dim),
    )


def mixer_of(params, i, config):
    if C.is_full(config, i):
        return Model.Full(layer=attention_of(params, i, config))
    else:
        return Model.Linear(layer=delta_of(params, i, config))


def layer_of(params, i, config) -> Model.Layer:
    hidden = config.hidden_size
    return Model.Layer(
        input_norm=centred(params, f"layers.{i}.input_layernorm.weight", hidden),
        post_norm=centred(params, f"layers.{i}.post_attention_layernorm.weight", hidden),
        mixer=mixer_of(params, i, config),
        mlp=mlp_of(params, i, config),
    )


def build(params, config) -> Model.Model:
    """The model with `lm_head` tied to `embed_tokens`."""
    hidden = config.hidden_size
    vocab = config.vocab_size
    return Model.Model(
        config=config,
        shapes=Shapes.shapes_of(config),
        embedding=Model.Table(values=buffer(params, "embed_tokens.weight", vocab * hidden)),
        layers=toList(map(range(config.num_hidden_layers), lambda i: layer_of(params, i, config))),
        final_norm=centred(params, "norm.weight", hidden),
        head=dense(params, "embed_tokens.weight", vocab, hidden),
        tiled=0,
    )


def load(params, config) -> Result[Model.Model, str]:
    """The model on the device, or the first parameter that was missing or mis-sized."""
    with strictly:
        return Success(value=build(params, config))
