# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=ModelCases
"""Shared ground for the device-model cases: the fixture's config and rows, the
comparison helpers and the one-token / cached-token drivers the fixture, GGUF and
tiled suites build on ([LLM-MODEL-ORACLE])."""
from testing import Check
from qwen_expected import Expected
from qwen import Qwen3_5Attention, Qwen3_5Cache, Qwen3_5Config, Qwen3_5DeltaNet, Qwen3_5Device, Qwen3_5Hf, Qwen3_5Mlp, Qwen3_5Model, Qwen3_5Norm, Qwen3_5Rope, Qwen3_5Shapes
from transformers.Math import argmax

__all__ = ["CACHED", "MODEL_ATOL", "MODEL_RTOL", "SINGLE", "cache_value", "close_list", "compare", "compare_heads", "compare_rows", "fixture_params", "fixture_row", "flatten", "greedy_case", "hidden_row", "hook_cases", "hook_row", "input_tokens", "model_config", "model_with", "name_at", "refusal", "shape_size", "shape_total", "stage_cases", "stages_against", "started", "to_floats", "token_in"]


def close_list(name, got, want, rtol, atol):
    """Float32 lists compared by exact widening to float64."""
    return Check.close_all(name, mapList(got, toFloat), mapList(want, toFloat), rtol, atol)


def flatten(lists):
    """One list of every outcome in a list of outcome lists."""
    return foldList(lists, [], lambda acc, outcomes: listConcat(acc, outcomes))


# --- the tiny Qwen3_5TextConfig checkpoint: [LLM-MODEL-ORACLE] ---
def shape_size(shape):
    """The element count of one row-major shape."""
    return foldList(shape, 1, lambda total, extent: total * extent)


def shape_total(shapes):
    """The element count of every shape in a manifest."""
    return foldList(shapes, 0, lambda total, shape: total + shape_size(shape))


def name_at(names, k):
    """Name `k` of a fixture name list, or ""."""
    return unwrap_or(listGet(names, k), "")


# --- the qwen3_5 model on device buffers: [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE] ---
MODEL_RTOL = 1e-5
"""The relative half of the port's tolerance against the fixture; see `MODEL_ATOL`."""


MODEL_ATOL = 1e-6
"""The port's tolerance against the fixture: the path runs binary32 arithmetic whose
dot products reduce in a different order from torch's CPU GEMM and whose `exp`/`ln`
come from the device library, so [PORT-TEST-ORACLE]'s exact-arithmetic tolerances do
not apply."""


SINGLE = 0
"""`MODEL_HIDDEN_VALUES` index of the one-token input's embedding; its stages follow."""


CACHED = 10
"""`MODEL_HIDDEN_VALUES` index of the cached run's embeddings; its stages follow."""


def index_in(names, name):
    """The position of `name` in a fixture name list, or -1 when it is absent."""
    return fold(range(len(names)), -1, lambda hit, i: i if hit < 0 and unwrap_or(listGet(names, i), "") == name else hit)


def param_shape(name):
    """One recorded parameter's shape, or `[]` when the manifest has no such name."""
    return unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, index_in(Expected.MODEL_PARAM_NAMES, name)), [])


def extent(shape, axis):
    return unwrap_or(listGet(shape, axis), 0)


def model_config():
    """The fixture's `Qwen3_5TextConfig`: the attention and MLP widths come from the
    recorded parameter shapes; the linear-attention head split (two key heads of eight,
    four value heads of eight) and the rope settings from the config the oracle's
    docstring records, since `[64, 64]` alone does not say how `key_dim` splits."""
    head_dim = extent(param_shape("layers.3.self_attn.q_norm.weight"), 0)
    conv = param_shape("layers.0.linear_attn.conv1d.weight")
    return Qwen3_5Config.Config(
        hidden_size=Expected.MODEL_HIDDEN,
        intermediate_size=extent(param_shape("layers.3.mlp.gate_proj.weight"), 0),
        vocab_size=Expected.MODEL_VOCAB,
        num_hidden_layers=Expected.MODEL_LAYERS,
        num_attention_heads=extent(param_shape("layers.3.self_attn.q_proj.weight"), 0) // (2 * head_dim),
        num_key_value_heads=extent(param_shape("layers.3.self_attn.k_proj.weight"), 0) // head_dim,
        head_dim=head_dim,
        partial_rotary_factor=0.25,
        rope_theta=10000.0,
        rms_norm_eps=0.000001,
        linear_num_key_heads=2,
        linear_num_value_heads=extent(param_shape("layers.0.linear_attn.A_log"), 0),
        linear_key_head_dim=8,
        linear_value_head_dim=extent(param_shape("layers.0.linear_attn.norm.weight"), 0),
        linear_conv_kernel_dim=extent(conv, 2),
        layer_types=Expected.MODEL_LAYER_TYPES,
    )


def to_floats(xs):
    """A numeric `MODEL_*` row as float32 (the fixture writes the shortest decimal that
    reads back as the same binary32)."""
    return mapList(xs, toFloat32)


def fixture_params(parameter_values):
    """The fixture's parameters as the Transformers loader takes them."""
    return Qwen3_5Hf.Params(names=Expected.MODEL_PARAM_NAMES, values=mapList(parameter_values, to_floats))


def hidden_row(k, t):
    """Row `t` of `MODEL_HIDDEN_VALUES[k]`."""
    return fixture_row(Expected.MODEL_HIDDEN_VALUES, k, t)


def fixture_row(table, k, t):
    """Row `t` of hidden-row table `table[k]`, `MODEL_HIDDEN` wide."""
    return take_row(to_floats(unwrap_or(listGet(table, k), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def hook_row(name, t):
    """Row `t` of one `MODEL_HOOK_*` activation, or `[]` when the fixture has no such hook."""
    return take_row(to_floats(unwrap_or(listGet(Expected.MODEL_HOOK_VALUES, index_in(Expected.MODEL_HOOK_NAMES, name)), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def cache_value(name):
    """One `MODEL_CACHE_*` state tensor, flattened, or `[]` when the cache has no such name."""
    return to_floats(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, index_in(Expected.MODEL_CACHE_NAMES, name)), []))


def take_row(xs, start, width):
    return toList(map(range(width), lambda i: unwrap_or(listGet(xs, start + i), toFloat32(0.0))))


def compare(name, buffer, want):
    """A device buffer read back and compared with a fixture row."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, got, want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def compare_rows(name, buffer, start, count, want):
    """`count` values of a buffer from `start` against a fixture tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, take_row(got, start, count), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def token_in(tokens, t):
    """Token `t` of an input, or 0 past its end."""
    return unwrap_or(listGet(tokens, t), 0)


def input_tokens(i):
    """`MODEL_INPUTS[i]`."""
    return unwrap_or(listGet(Expected.MODEL_INPUTS, i), [])


def started(model, label, run):
    """`run` over a fresh cache, or the failure to allocate one."""
    match Qwen3_5Cache.empty(model.config, model.shapes):
        case Success(value=cache):
            return run(cache)
        case Error(message=message):
            return [Check.failed(f"{label}_cache", message)]


def stage_cases(label, next, base, t):
    """The embedding, the three recorded layer outputs and the final norm of row `t`."""
    return stages_against(label, next, lambda k: hidden_row(base + k, t))


def stages_against(label, next, row_of):
    """The embedding, the recorded layer outputs and the final norm of one decoded
    token against `row_of(k)`, `k` counting the stages from the embedding."""
    return listConcat(
        [compare(f"{label}_embed", next.row, row_of(0))],
        listConcat(
            toList(map(range(len(Expected.MODEL_HIDDEN_STAGES) - 2), lambda i: stage_case(label, next, row_of, i))),
            [compare(f"{label}_final_norm", next.normed, row_of(len(Expected.MODEL_HIDDEN_STAGES) - 1))],
        ),
    )


def stage_case(label, next, row_of, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"{label}_layer{i}", stage, row_of(1 + i))
        case Error(message=message):
            return Check.failed(f"{label}_layer{i}", toString(message))


def greedy_case(label, next):
    """The device argmax agrees with the host's over the same logits."""
    match fromGpu(next.logits):
        case Success(value=values):
            return Check.equal(f"{label}_greedy", next.best, argmax(values))
        case Error(message=message):
            return Check.failed(f"{label}_greedy", toString(message))


def hook_cases(next, t):
    """At position 0 the batched run and the cached run agree, so the first token also
    checks every layer's output hook, including layer 3's, which no hidden stage records."""
    if t == 0:
        return toList(map(range(Expected.MODEL_LAYERS), lambda i: hook_case(next, i)))
    else:
        return []


def hook_case(next, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"model_hook_layer{i}", stage, hook_row(f"layers.{i}", 0))
        case Error(message=message):
            return Check.failed(f"model_hook_layer{i}", toString(message))


def compare_heads(name, buffer, positions, cfg, want):
    """A position-major KV buffer against the fixture's `[heads, positions, head_dim]` tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, head_major(got, positions, cfg.num_key_value_heads, cfg.head_dim), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def head_major(flat, positions, heads, width):
    return toList(map(range(heads * positions * width), lambda j: unwrap_or(listGet(flat, ((j // width) % positions) * heads * width + (j // (positions * width)) * width + j % width), toFloat32(0.0))))


def refusal(name, result, want):
    """`result` must be the `Error` whose message is exactly `want`."""
    match result:
        case Success(value=_):
            return Check.failed(name, "loaded a checkpoint that should have been refused")
        case Error(message=message):
            return Check.equal(name, message, want)


def model_with(model, layers, embedding, head):
    """`model` with other layers, embedding and head."""
    return Qwen3_5Model.Model(config=model.config, shapes=model.shapes, embedding=embedding, layers=layers, final_norm=model.final_norm, head=head, tiled=model.tiled)
