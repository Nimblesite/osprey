# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=TiledCases
"""The GGUF value-head order, and every way a load or a step is refused: a missing or
mis-sized parameter, a slot of the wrong kind, a missing layer, a missing mapped file,
an undecodable head, a NaN logit row ([LLM-MODEL-CACHE], [LLM-ARITH-IEEE])."""
from testing import Check
from qwen import Qwen3_5Attention, Qwen3_5Cache, Qwen3_5Config, Qwen3_5DeltaNet, Qwen3_5Device, Qwen3_5Hf, Qwen3_5Mlp, Qwen3_5Model, Qwen3_5Norm, Qwen3_5Rope, Qwen3_5Shapes
from qwen_tests.ModelCases import SINGLE, fixture_params, input_tokens, model_config, model_with, name_at, refusal, stage_cases, started, token_in

__all__ = ["model_fault_cases", "refusal_cases", "tiled_cases"]


def tiled_cases(model, parameters):
    """The GGUF value-head order: the same checkpoint with every per-value-head
    parameter permuted into llama.cpp's tiled order, loaded as `tiled`, yields the same
    one-token activations as Transformers' grouped order."""
    match Qwen3_5Hf.load(tiled_params(model.config, parameters), model.config):
        case Error(message=message):
            return [Check.failed("model_tiled_load", message)]
        case Success(value=grouped):
            return tiled_run(Qwen3_5Model.Model(config=grouped.config, shapes=grouped.shapes, embedding=grouped.embedding, layers=grouped.layers, final_norm=grouped.final_norm, head=grouped.head, tiled=1))


def tiled_run(model):
    return started(model, "model_tiled", lambda cache: tiled_decode(model, cache))


def tiled_decode(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_tiled", message)]
        case Success(value=next):
            return stage_cases("model_tiled", next, SINGLE, 0)


def tiled_params(cfg, parameters):
    """`fixture_params` with the linear-attention value heads in tiled order, as
    `convert_hf_to_gguf.py`'s `_reorder_v_heads` writes them."""
    values = toList(map(range(len(parameters.names)), lambda k: tiled_param(cfg, name_at(parameters.names, k), unwrap_or(listGet(parameters.values, k), []))))
    return Qwen3_5Hf.Params(names=parameters.names, values=values)


def tiled_param(cfg, name, values):
    hk = cfg.linear_num_key_heads
    r = cfg.linear_num_value_heads // hk
    dv = cfg.linear_value_head_dim
    hidden = cfg.hidden_size
    if contains(name, "in_proj_qkv") or contains(name, "conv1d"):
        return reordered_rows(values, 2 * Qwen3_5Config.key_dim(cfg), hk, r, dv, len(values) // Qwen3_5Config.conv_dim(cfg))
    elif contains(name, "in_proj_z"):
        return reordered_rows(values, 0, hk, r, dv, hidden)
    elif contains(name, "in_proj_b") or contains(name, "in_proj_a"):
        return reordered_rows(values, 0, hk, r, 1, hidden)
    elif contains(name, "A_log") or contains(name, "dt_bias"):
        return reordered_rows(values, 0, hk, r, 1, 1)
    elif contains(name, "out_proj"):
        return reordered_columns(values, hk, r, dv)
    else:
        return values


def grouped_of(tiled, hk, r):
    """The grouped value head that sits at tiled position `tiled`: `(t % hk) * r + t // hk`."""
    return (tiled % hk) * r + tiled // hk


def reordered_rows(values, skip, hk, r, dim, width):
    """Rows of `width` from `skip` on, grouped by value head of `dim` rows, in tiled order."""
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, j if j < skip * width else source_row(j - skip * width, hk, r, dim, width) + skip * width), toFloat32(0.0))))


def source_row(j, hk, r, dim, width):
    row = j // width
    return (grouped_of(row // dim, hk, r) * dim + row % dim) * width + j % width


def reordered_columns(values, hk, r, dim):
    """Rows of `hk * r * dim` columns, the columns in tiled value-head order."""
    cols = hk * r * dim
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, (j // cols) * cols + grouped_of((j % cols) // dim, hk, r) * dim + j % dim), toFloat32(0.0))))


def refusal_cases():
    """A checkpoint missing a parameter, or holding one of the wrong size, is refused by name."""
    cfg = model_config()
    return [
        refusal("model_load_missing", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["norm.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight is not in the checkpoint"),
        refusal("model_load_sized", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["embed_tokens.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight holds 1 values, not 8192"),
    ]


def model_fault_cases(model):
    """Every way a step is refused: a missing or wrong-kind cache slot, a missing layer, a missing mapped file, an undecodable head, a NaN logit row."""
    return listConcat([
        refusal("model_decode_missing_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[]), 0), "layer 0: no cache slot: index out of bounds"),
        nan_logits(model),
        refusal("model_step_missing_slot", step_missing_slot(model), "layer 0: no cache slot: index out of bounds"),
        refusal("model_fault_negative_grid", negative_grid(), "gpu: buffer allocation: a negative element count"),
        refusal("model_fault_missing_values", missing_values(), "index out of bounds"),
        refusal("model.Fault.halt_ints", explicit_device_fault(), "test device fault"),
    ], started(model, "model_wrong_slots", lambda cache: wrong_slot_cases(model, cache)))


def nan_logits(model):
    """[LLM-ARITH-IEEE] A head whose weights hold a NaN makes every logit NaN; `decode`
    refuses the token instead of picking an index from garbage."""
    width = model.config.hidden_size * model.config.vocab_size
    match toGpu(toList(map(range(width), lambda i: toFloat32(sqrt(-1.0)) if i == 7 else toFloat32(0.5)))):
        case Error(message=message):
            return Check.failed("model_decode_nan_logits", toString(message))
        case Success(value=values):
            return nan_decode(model_with(model, model.layers, model.embedding, Qwen3_5Device.Dense(values=values)))


def nan_decode(model):
    match Qwen3_5Cache.empty(model.config, model.shapes):
        case Error(message=message):
            return Check.failed("model_decode_nan_logits", message)
        case Success(value=cache):
            return refusal("model_decode_nan_logits", Qwen3_5Model.decode(model, cache, 0), "decode: step logits are not finite")


def step_missing_slot(model):
    with Qwen3_5Device.strictly:
        return Qwen3_5Model.step(model, Qwen3_5Cache.Cache(slots=[]), Qwen3_5Model.embed(model, 0), 1)


def wrong_slot_cases(model, cache):
    match Qwen3_5Cache.slot_at(cache, 0):
        case Error(message=message):
            return [Check.failed("model_wrong_slots", toString(message))]
        case Success(value=delta):
            match Qwen3_5Cache.slot_at(cache, 3):
                case Error(message=message):
                    return [Check.failed("model_wrong_slots", toString(message))]
                case Success(value=attention):
                    return mismatch_cases(model, cache, delta, attention)


def mismatch_cases(model, cache, delta, attention):
    return [
        refusal("model_linear_wrong_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[attention]), 0), "layer 0: a linear_attention layer was given an attention cache slot"),
        refusal("model_full_wrong_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[delta, delta, delta, delta]), 0), "layer 3: a full_attention layer was given a linear cache slot"),
        refusal("model_decode_missing_layer", Qwen3_5Model.decode(model_with(model, [], model.embedding, model.head), cache, 0), "layer 0: index out of bounds"),
        mapped_missing_file(model, cache),
        quantized_head_fault(model, cache),
    ]


def mapped_missing_file(model, cache):
    path = "/nonexistent_osprey_dir/missing-embedding.gguf"
    match mapped_decode(model, cache, path):
        case Error(message=message):
            return Check.truth("model_mapped_missing_file", startsWith(message, "gpuMapFile: " + path + ":"), message)
        case Success():
            return Check.failed("model_mapped_missing_file", "decoded a missing embedding file")


def mapped_decode(model, cache, path):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            mapped = Qwen3_5Model.Mapped(path=path, offset=0, row_bytes=144, type="Q4_K", basis=Qwen3_5Model.identity(model.config.hidden_size))
            return Qwen3_5Model.decode(model_with(model, model.layers, mapped, model.head), cache, 0)


def quantized_head_fault(model, cache):
    match toGpu(toList(map(range(144), lambda _i: toUint8(0)))):
        case Error(message=message):
            return Check.failed("model_quantized_head_fault", toString(message))
        case Success(value=bytes):
            head = Qwen3_5Device.Quant(blocks=bytes, type="Q2_K")
            return refusal("model_quantized_head_fault", Qwen3_5Model.decode(model_with(model, model.layers, model.embedding, head), cache, 0), "gpuMatMulQuant: weights: unsupported tensor type; expected Q4_K, Q8_0, Q6_K, Q5_K, Q3_K, IQ4_NL, IQ4_XS, IQ3_S")


def negative_grid():
    with Qwen3_5Device.strictly:
        return Success(value=Qwen3_5Device.grid(-1))


def missing_values():
    with Qwen3_5Device.strictly:
        return Success(value=Qwen3_5Device.must_values(listGet([[toFloat32(1.0)]], 4)))


def explicit_device_fault():
    with Qwen3_5Device.strictly:
        return Success(value=perform(Qwen3_5Device.Fault.halt_ints("test device fault")))
