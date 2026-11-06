# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=ModelCases
"""The device model against the tiny `Qwen3_5TextConfig` checkpoint: every activation
and cache state transformers 5.20.2 + torch 2.14.1 computed (oracle/qwen3_5.cpython),
loaded through the Transformers-layout loader ([LLM-MODEL-ORACLE], [LLM-MODEL-DELTANET],
[LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE])."""
from testing import Check
from qwen_expected import Expected
from qwen import Qwen3_5Attention, Qwen3_5Cache, Qwen3_5Config, Qwen3_5DeltaNet, Qwen3_5Device, Qwen3_5Hf, Qwen3_5Mlp, Qwen3_5Model, Qwen3_5Norm, Qwen3_5Rope, Qwen3_5Shapes
from transformers.Math import argmax

__all__ = ["MODEL_RTOL", "MODEL_ATOL", "SINGLE", "CACHED", "model_fixture_cases", "flatten", "close_list", "model_config", "fixture_params", "hidden_row", "fixture_row", "compare", "started", "stages_against", "hook_cases", "greedy_case", "token_in", "input_tokens", "refusal"]


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


def manifest_cases(parameter_values):
    """The fixture's own counts: the declared parameter count is the sum of the recorded
    shapes, and every tensor has one manifest row per value list (so the fixture cannot
    describe a shape it does not carry)."""
    return [
        Check.equal("model_param_count", shape_total(Expected.MODEL_PARAM_SHAPES), Expected.MODEL_PARAM_COUNT),
        Check.equal("model_param_tensors", len(Expected.MODEL_PARAM_NAMES), len(Expected.MODEL_PARAM_SHAPES)),
        Check.equal("model_param_dtypes", len(Expected.MODEL_PARAM_DTYPES), len(parameter_values)),
        Check.equal("model_cache_tensors", len(Expected.MODEL_CACHE_NAMES), len(Expected.MODEL_CACHE_VALUES)),
        Check.equal("model_cache_dtypes", len(Expected.MODEL_CACHE_DTYPES), len(Expected.MODEL_CACHE_SHAPES)),
        Check.equal("model_hidden_stages", len(Expected.MODEL_HIDDEN_STAGES), Expected.MODEL_LAYERS + 1),
        Check.equal("model_hidden_states", len(Expected.MODEL_HIDDEN_VALUES), len(Expected.MODEL_INPUT_TOKENS) * len(Expected.MODEL_HIDDEN_STAGES)),
        Check.equal("model_inputs", len(Expected.MODEL_INPUTS), len(Expected.MODEL_INPUT_TOKENS)),
        Check.equal("model_layer_types", len(Expected.MODEL_LAYER_TYPES), Expected.MODEL_LAYERS),
        Check.equal("model_layer_pattern", join(Expected.MODEL_LAYER_TYPES, ","), "linear_attention,linear_attention,linear_attention,full_attention"),
        Check.equal("model_hook_names", len(Expected.MODEL_HOOK_NAMES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_hook_dtypes", len(Expected.MODEL_HOOK_DTYPES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_dtype_path", len(Expected.MODEL_DTYPE_PATH), 8),
    ]


def param_cases(parameter_values):
    """One assertion per recorded parameter tensor: its values fill its shape."""
    return toList(map(range(len(parameter_values)), lambda k: Check.equal(
        "model_param[" + name_at(Expected.MODEL_PARAM_NAMES, k) + "]",
        len(unwrap_or(listGet(parameter_values, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, k), [])),
    )))


def hidden_cases():
    """One assertion per recorded hidden state: one value per token per hidden channel."""
    return toList(map(range(len(Expected.MODEL_HIDDEN_VALUES)), lambda k: Check.equal(
        "model_hidden[" + name_at(Expected.MODEL_HIDDEN_STAGES, k % len(Expected.MODEL_HIDDEN_STAGES)) + "]",
        len(unwrap_or(listGet(Expected.MODEL_HIDDEN_VALUES, k), [])),
        unwrap_or(listGet(Expected.MODEL_HIDDEN_TOKENS, k), 0) * Expected.MODEL_HIDDEN,
    )))


def cache_cases():
    """One assertion per cache state tensor: its values fill its shape."""
    return toList(map(range(len(Expected.MODEL_CACHE_VALUES)), lambda k: Check.equal(
        "model_cache[" + name_at(Expected.MODEL_CACHE_NAMES, k) + "]",
        len(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_CACHE_SHAPES, k), [])),
    )))


def name_at(names, k):
    return unwrap_or(listGet(names, k), "")


def model_fixture_cases():
    """[LLM-MODEL-ORACLE] The tiny `Qwen3_5TextConfig` fixture transformers wrote is
    self-consistent (its counts and shapes), and the device model built from its
    parameters reproduces its activations and cache."""
    parameter_values = Expected.MODEL_PARAM_VALUES
    return listConcat(manifest_cases(parameter_values), listConcat(param_cases(parameter_values), listConcat(hidden_cases(), listConcat(cache_cases(), model_layer_cases(fixture_params(parameter_values))))))


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


def model_layer_cases(parameters):
    """The device model from the fixture's parameters: the one-token input and the
    eight tokens fed one at a time through the cache reproduce every recorded hidden
    stage, the first cached token every layer hook, the final cache every state tensor,
    and the loader refuses a missing or mis-sized parameter."""
    match Qwen3_5Hf.load(parameters, model_config()):
        case Error(message=message):
            return [Check.failed("model_load", message)]
        case Success(value=model):
            return flatten([[Check.passed("model_load")], model_geometry_cases(model), checkpoint_conversion_cases(), checked_device_cases(model.config), single_cases(model), model_interface_cases(model), cached_cases(model), tiled_cases(model, parameters), refusal_cases(), model_fault_cases(model)])


def model_geometry_cases(model):
    cfg = model.config
    return [
        Check.equal("model.config.linear", Qwen3_5Config.LINEAR, "linear_attention"),
        Check.equal("model.config.full", Qwen3_5Config.FULL, "full_attention"),
        Check.truth("model.config.layer_type", structuralEquals(toList(map(range(5), lambda i: Qwen3_5Config.layer_type(cfg, i))), listAppend(Expected.MODEL_LAYER_TYPES, "")), "layer kinds must match the fixture and end at index 4"),
        Check.truth("model.config.is_full", structuralEquals(toList(map(range(5), lambda i: Qwen3_5Config.is_full(cfg, i))), [False, False, False, True, False]), "only layer 3 is full attention"),
        Check.equal("model.config.value_dim", Qwen3_5Config.value_dim(cfg), 32),
        Check.equal("model.config.kv_groups", Qwen3_5Config.kv_groups(cfg), 1),
        Check.equal("model.config.rotary_dim", Qwen3_5Config.rotary_dim(cfg), 4),
        Check.equal("model.shapes.norm_chunk", model.shapes.norm_chunk, 64),
        Check.equal("model.shapes.vocab_chunk", model.shapes.vocab_chunk, 128),
        Check.ints_equal("model.shapes.chunk_of", [Qwen3_5Shapes.chunk_of(1024, Qwen3_5Shapes.NORM_CHUNK), Qwen3_5Shapes.chunk_of(1024, Qwen3_5Shapes.VOCAB_CHUNK), Qwen3_5Shapes.chunk_of(511, Qwen3_5Shapes.VOCAB_CHUNK)], [256, 512, 511]),
    ]


def checkpoint_conversion_cases():
    row = to_floats([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    return [
        close_list("model.checkpoint.transpose", Qwen3_5Hf.transposed(row, 2, 3), to_floats([1.0, 4.0, 2.0, 5.0, 3.0, 6.0]), 0.0, 0.0),
        close_list("model.checkpoint.centered", Qwen3_5Hf.plus_one(to_floats([-1.0, 0.0, 0.5])), to_floats([0.0, 1.0, 1.5]), 0.0, 0.0),
        close_list("model.checkpoint.decay", Qwen3_5Hf.neg_exp(to_floats([-1.0, 0.0, 1.0])), to_floats([-0.36787944, -1.0, -2.71828183]), MODEL_RTOL, MODEL_ATOL),
    ]


def guarded_cases(label, run):
    """A run's check list, or one failed case naming the run's label."""
    match run():
        case Success(value=checks):
            return checks
        case Error(message=message):
            return [Check.failed(label, message)]


def checked_device_cases(cfg):
    return guarded_cases("model.device", lambda: run_device_cases(cfg))


def run_device_cases(cfg):
    with Qwen3_5Device.strictly:
        shapes = Qwen3_5Shapes.shapes_of(cfg)
        return Success(value=flatten([divider_cases(), buffer_cases(), shape_grid_cases(shapes), rope_cases(cfg, shapes), head_norm_cases()]))


def divider_cases():
    divisor = Qwen3_5Device.divider(3)
    expected = Qwen3_5Device.Div(mul=5726623062, shift=34)
    return [
        Check.ints_equal("model.device.divider", [divisor.mul, divisor.shift], [expected.mul, expected.shift]),
        Check.ints_equal("model.device.quot", mapList([0, 2, 3, 4, 17, 65535], lambda i: Qwen3_5Device.quot(i, divisor.mul, divisor.shift)), [0, 0, 1, 1, 5, 21845]),
        Check.ints_equal("model.device.rem", mapList([0, 2, 3, 4, 17, 65535], lambda i: Qwen3_5Device.rem(i, divisor.mul, divisor.shift, 3)), [0, 2, 0, 1, 2, 0]),
        Check.close("model.device.inverse", toFloat(Qwen3_5Device.inverse(8)), 0.125, 0.0, 0.0),
        Check.close("model.device.rsqrt", toFloat(Qwen3_5Device.rsqrt(toFloat32(4.0))), 0.5, MODEL_RTOL, MODEL_ATOL),
        Check.close("model.device.softplus", toFloat(Qwen3_5Device.softplus(toFloat32(0.0))), 0.69314718, MODEL_RTOL, MODEL_ATOL),
    ]


def buffer_cases():
    indices = Qwen3_5Device.must_ints(gpuIota(2))
    a = Qwen3_5Device.constant(to_floats([1.0, -2.0, 3.0]))
    b = Qwen3_5Device.constant(to_floats([4.0, 2.0]))
    return [
        compare("model.device.zeros", Qwen3_5Device.zeros(indices), to_floats([0.0, 0.0])),
        compare("model.device.added", Qwen3_5Device.added(a, b, indices), to_floats([5.0, 0.0])),
        compare("model.device.slice", Qwen3_5Device.slice_of(a, 1, indices), to_floats([-2.0, 3.0])),
        Check.close("model.device.squares", toFloat(Qwen3_5Device.squares(a, 1, 2)), 13.0, 0.0, 0.0),
        compare("model.device.sigmoid", Qwen3_5Device.sigmoid(Qwen3_5Device.constant(to_floats([-1.0, 0.0, 1.0]))), to_floats([0.26894142, 0.5, 0.73105858])),
        compare("model.device.Weight.dense", Qwen3_5Device.linear(b, Qwen3_5Device.Dense(values=b), 1, 2, 1), to_floats([20.0])),
    ]


def shape_grid_cases(shapes):
    return [
        Check.ints_equal("model.device.Shapes.grids", [gpuLength(shapes.hidden), gpuLength(shapes.conv), gpuLength(shapes.value), gpuLength(shapes.state)], [64, 64, 32, 288]),
    ]


def rope_cases(cfg, shapes):
    angles = Qwen3_5Rope.tables(1, cfg)
    head = Qwen3_5Device.constant(toList(map(range(1, 17), lambda i: toFloat32(i))))
    return [
        compare("model.rope.Tables.cos", angles.cos, to_floats([0.54030231, 0.99995, 0.54030231, 0.99995])),
        compare("model.rope.Tables.sin", angles.sin, to_floats([0.84147098, 0.00999983, 0.84147098, 0.00999983])),
        compare_rows("model.rope.rotated", Qwen3_5Rope.rotated(head, angles, Qwen3_5Device.grid(16), shapes, cfg), 0, 6, to_floats([-1.98411065, 1.95990067, 2.46237790, 4.01979967, 5.0, 6.0])),
    ]


def head_norm_cases():
    x = Qwen3_5Device.constant(to_floats([1.0, 1.0, 2.0, 2.0]))
    z = Qwen3_5Device.constant(to_floats([1.0, 1.0, 1.0, 1.0]))
    w = Qwen3_5Device.constant(to_floats([2.0, 3.0]))
    heads = Qwen3_5Device.grid(2)
    cells = Qwen3_5Device.grid(4)
    div = Qwen3_5Device.divider(2)
    return [
        compare("model.norm.rms_heads", Qwen3_5Norm.rms_heads(x, w, heads, cells, div, 2, toFloat32(0.0)), to_floats([2.0, 3.0, 2.0, 3.0])),
        compare("model.norm.gated", Qwen3_5Norm.gated(x, z, w, heads, cells, div, 2, toFloat32(0.0)), to_floats([1.46211716, 2.19317574, 1.46211716, 2.19317574])),
    ]


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


def single_cases(model):
    """`MODEL_INPUTS[0]`, one token at position 0, from a fresh cache."""
    return started(model, "model_single", lambda cache: single_run(model, cache))


def single_run(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_single", message)]
        case Success(value=next):
            return listConcat(stage_cases("model_single", next, SINGLE, 0), [greedy_case("model_single", next), public_greedy_case(model, next)])


def public_greedy_case(model, next):
    match public_greedy(model, next.logits):
        case Success(value=best):
            return Check.equal("model.Next.greedy", best, next.best)
        case Error(message=message):
            return Check.failed("model.Next.greedy", message)


def public_greedy(model, logits):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            return Success(value=Qwen3_5Model.greedy(model, logits))


def model_interface_cases(model):
    return guarded_cases("model.interfaces", lambda: run_model_interfaces(model))


def run_model_interfaces(model):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            return Success(value=started(model, "model_interfaces", lambda cache: model_interface_run(model, cache)))


def model_interface_run(model, cache):
    row = Qwen3_5Model.embed(model, token_in(input_tokens(2), 0))
    match Qwen3_5Model.forward(model, cache, row):
        case Error(message=message):
            return [Check.failed("model.Walk.forward", message)]
        case Success(value=walk):
            return flatten([[compare("model.Embedding.row", row, hidden_row(CACHED, 0)), compare("model.Walk.forward", walk.hidden, hook_row("layers.3", 0))], layer_interface_cases(model, cache, row, 0), layer_interface_cases(model, cache, Qwen3_5Device.constant(hook_row("layers.2", 0)), 3)])


def layer_interface_cases(model, cache, row, index):
    match listGet(model.layers, index):
        case Error(message=message):
            return [Check.failed(f"model.Layer.{index}", toString(message))]
        case Success(value=layer):
            match Qwen3_5Cache.slot_at(cache, index):
                case Error(message=message):
                    return [Check.failed(f"model.Slot.{index}", toString(message))]
                case Success(value=slot):
                    return layer_hook_cases(model, row, index, layer, slot)


def layer_hook_cases(model, row, index, layer, slot):
    cfg = model.config
    shapes = model.shapes
    normed = Qwen3_5Norm.rms_row(row, layer.input_norm, cfg, shapes)
    return flatten([
        [compare(f"model.Layer.input_norm.{index}", normed, hook_row(f"layers.{index}.input_layernorm", 0))],
        mixer_cases(model, layer.mixer, slot, normed),
        [compare(f"model.Mlp.swiglu.{index}", Qwen3_5Mlp.swiglu(Qwen3_5Device.constant(hook_row(f"layers.{index}.post_attention_layernorm", 0)), layer.mlp, cfg, shapes), hook_row(f"layers.{index}.mlp", 0))],
    ])


def mixer_cases(model, mixer, slot, row):
    match mixer:
        case Qwen3_5Model.Linear(layer=layer):
            match slot:
                case Qwen3_5Cache.Delta(held=held):
                    return delta_mixer_cases(model, layer, row, held)
                case Qwen3_5Cache.Attention():
                    return [Check.failed("model.Mixer.linear", "expected the delta slot")]
        case Qwen3_5Model.Full(layer=layer):
            match slot:
                case Qwen3_5Cache.Attention(held=held):
                    return attention_mixer_cases(model, layer, row, held)
                case Qwen3_5Cache.Delta():
                    return [Check.failed("model.Mixer.attention", "expected the attention slot")]


def delta_mixer_cases(model, layer, row, held):
    mixed = Qwen3_5DeltaNet.mix(row, layer, held, model.tiled, model.config, model.shapes)
    slot = Qwen3_5Cache.Delta(held=Qwen3_5Cache.DeltaSlot(conv=mixed.slot.conv, state=mixed.slot.state))
    grown = Qwen3_5Cache.grown(Qwen3_5Cache.Cache(slots=[]), slot)
    return [
        compare("model.Mixer.linear.Mixed", mixed.hidden, hook_row("layers.0.linear_attn", 0)),
        Check.equal("model.DeltaSlot.grown", len(grown.slots), 1),
    ]


def attention_mixer_cases(model, layer, row, held):
    mixed = Qwen3_5Attention.mix(row, layer, held, model.config, model.shapes)
    return [
        compare("model.Mixer.attention.Mixed", mixed.hidden, hook_row("layers.3.self_attn", 0)),
        Check.equal("model.AttentionSlot.positions", mixed.slot.positions, 1),
    ]


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


def cached_cases(model):
    """`MODEL_INPUTS[2]` fed one token at a time, each through the cache the earlier tokens left."""
    return started(model, "model_cached", lambda cache: walk_tokens(model, input_tokens(2), 0, cache, []))


def walk_tokens(model, tokens, t, cache, acc):
    if t == len(tokens):
        return listConcat(acc, cache_tensor_cases(model, cache))
    else:
        match Qwen3_5Model.decode(model, cache, token_in(tokens, t)):
            case Error(message=message):
                return listAppend(acc, Check.failed(f"model_cached{t}", message))
            case Success(value=next):
                return walk_tokens(model, tokens, t + 1, next.walk.cache, listConcat(acc, listConcat(stage_cases(f"model_cached{t}", next, CACHED, t), hook_cases(next, t))))


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


def cache_tensor_cases(model, cache):
    """Every slot of the final cache against the fixture's `layers.N.*` state tensors."""
    return flatten(toList(map(range(model.config.num_hidden_layers), lambda i: slot_cases(model, cache, i))))


def slot_cases(model, cache, i):
    match Qwen3_5Cache.slot_at(cache, i):
        case Error(message=message):
            return [Check.failed(f"model_cache_slot{i}", toString(message))]
        case Success(value=slot):
            return held_cases(model, slot, i)


def held_cases(model, slot, i):
    match slot:
        case Qwen3_5Cache.Delta(held=held):
            return [
                compare(f"model_cache{i}_conv_states", held.conv, cache_value(f"layers.{i}.conv_states.0")),
                compare_rows(f"model_cache{i}_recurrent_states", held.state, 0, Qwen3_5Config.state_cells(model.config), cache_value(f"layers.{i}.recurrent_states.0")),
            ]
        case Qwen3_5Cache.Attention(held=held):
            return [
                compare_heads(f"model_cache{i}_keys", held.keys, held.positions, model.config, cache_value(f"layers.{i}.keys.0")),
                compare_heads(f"model_cache{i}_values", held.values, held.positions, model.config, cache_value(f"layers.{i}.values.0")),
            ]


def compare_heads(name, buffer, positions, cfg, want):
    """A position-major KV buffer against the fixture's `[heads, positions, head_dim]` tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, head_major(got, positions, cfg.num_key_value_heads, cfg.head_dim), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def head_major(flat, positions, heads, width):
    return toList(map(range(heads * positions * width), lambda j: unwrap_or(listGet(flat, ((j // width) % positions) * heads * width + (j // (positions * width)) * width + j % width), toFloat32(0.0))))


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
        return reordered_columns(values, hk, r, dv, hidden)
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


def reordered_columns(values, hk, r, dim, rows):
    """`[rows, hk * r * dim]` with its columns in tiled value-head order."""
    cols = hk * r * dim
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, (j // cols) * cols + grouped_of((j % cols) // dim, hk, r) * dim + j % dim), toFloat32(0.0))))


def refusal_cases():
    """A checkpoint missing a parameter, or holding one of the wrong size, is refused by name."""
    cfg = model_config()
    return [
        refusal("model_load_missing", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["norm.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight is not in the checkpoint"),
        refusal("model_load_sized", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["embed_tokens.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight holds 1 values, not 8192"),
    ]


def refusal(name, result, want):
    """`result` must be the `Error` whose message is exactly `want`."""
    match result:
        case Success(value=_):
            return Check.failed(name, "loaded a checkpoint that should have been refused")
        case Error(message=message):
            return Check.equal(name, message, want)


def model_fault_cases(model):
    return listConcat([
        refusal("model_decode_missing_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[]), 0), "layer 0: no cache slot: index out of bounds"),
        refusal("model_step_missing_slot", step_missing_slot(model), "layer 0: no cache slot: index out of bounds"),
        refusal("model_fault_negative_grid", negative_grid(), "gpu: buffer allocation: a negative element count"),
        refusal("model_fault_missing_values", missing_values(), "index out of bounds"),
        refusal("model.Fault.halt_ints", explicit_device_fault(), "test device fault"),
    ], started(model, "model_wrong_slots", lambda cache: wrong_slot_cases(model, cache)))


def step_missing_slot(model):
    with Qwen3_5Device.strictly:
        return Qwen3_5Model.step(model, Qwen3_5Cache.Cache(slots=[]), Qwen3_5Model.embed(model, 0))


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


def model_with(model, layers, embedding, head):
    return Qwen3_5Model.Model(config=model.config, shapes=model.shapes, embedding=embedding, layers=layers, final_norm=model.final_norm, head=head, tiled=model.tiled)


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
