# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=FixtureCases
"""The device model against the tiny `Qwen3_5TextConfig` checkpoint: every activation
and cache state transformers 5.20.2 + torch 2.14.1 computed (oracle/qwen3_5.cpython),
loaded through the Transformers-layout loader ([LLM-MODEL-ORACLE], [LLM-MODEL-DELTANET],
[LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE])."""
from testing import Check
from qwen_expected import Expected
from qwen import Qwen3_5Attention, Qwen3_5Cache, Qwen3_5Config, Qwen3_5DeltaNet, Qwen3_5Device, Qwen3_5Hf, Qwen3_5Mlp, Qwen3_5Model, Qwen3_5Norm, Qwen3_5Rope, Qwen3_5Shapes
from qwen_tests.ModelCases import CACHED, MODEL_ATOL, MODEL_RTOL, SINGLE, cache_value, close_list, compare, compare_heads, compare_rows, fixture_params, flatten, greedy_case, hidden_row, hook_cases, hook_row, input_tokens, logits_row, model_config, name_at, shape_size, shape_total, stage_cases, started, to_floats, token_in
from qwen_tests.TiledCases import model_fault_cases, refusal_cases, tiled_cases

__all__ = ["model_fixture_cases"]


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


def model_fixture_cases():
    """[LLM-MODEL-ORACLE] The tiny `Qwen3_5TextConfig` fixture transformers wrote is
    self-consistent (its counts and shapes), and the device model built from its
    parameters reproduces its activations and cache."""
    parameter_values = Expected.MODEL_PARAM_VALUES
    return listConcat(manifest_cases(parameter_values), listConcat(param_cases(parameter_values), listConcat(hidden_cases(), listConcat(cache_cases(), model_layer_cases(fixture_params(parameter_values))))))


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
    return listConcat([
        compare("model.rope.Tables.cos", angles.cos, to_floats([0.54030231, 0.99995, 0.54030231, 0.99995])),
        compare("model.rope.Tables.sin", angles.sin, to_floats([0.84147098, 0.00999983, 0.84147098, 0.00999983])),
        compare_rows("model.rope.rotated", Qwen3_5Rope.rotated(head, angles, Qwen3_5Device.grid(16), shapes, cfg), 0, 6, to_floats([-1.98411065, 1.95990067, 2.46237790, 4.01979967, 5.0, 6.0])),
    ], rope_hook_cases(cfg))


def rope_hook_cases(cfg):
    """The fixture's `rotary_emb` cos table, one check per position of the batched run."""
    width = Qwen3_5Config.rotary_dim(cfg)
    return toList(map(range(len(input_tokens(1))), lambda t: compare(f"model.rope.cos.{t}", Qwen3_5Rope.tables(t, cfg).cos, hook_row("rotary_emb", t, width))))


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
            return listConcat([compare("model.Embedding.row", row, hidden_row(CACHED, 0)), compare("model.Walk.forward", walk.hidden, hook_row("layers.3", 0, Expected.MODEL_HIDDEN))], flatten(toList(map(range(Expected.MODEL_LAYERS), lambda index: layer_interface_cases(model, cache, layer_input(row, index), index)))))


def layer_input(row, index):
    """Layer `index`'s input: the embedding row for layer 0, the fixture's layer
    `index - 1` output after it."""
    return row if index == 0 else Qwen3_5Device.constant(hook_row(f"layers.{index - 1}", 0, Expected.MODEL_HIDDEN))


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
    return listConcat(
        [compare(f"model.Layer.input_norm.{index}", normed, hook_row(f"layers.{index}.input_layernorm", 0, Expected.MODEL_HIDDEN))],
        mixer_cases(model, layer, slot, row, normed, index),
    )


def mixer_cases(model, layer, slot, row, normed, index):
    match layer.mixer:
        case Qwen3_5Model.Linear(layer=mixer):
            match slot:
                case Qwen3_5Cache.Delta(held=held):
                    return delta_mixer_cases(model, layer, mixer, row, normed, held, index)
                case Qwen3_5Cache.Attention():
                    return [Check.failed("model.Mixer.linear", "expected the delta slot")]
        case Qwen3_5Model.Full(layer=mixer):
            match slot:
                case Qwen3_5Cache.Attention(held=held):
                    return attention_mixer_cases(model, layer, mixer, row, normed, held, index)
                case Qwen3_5Cache.Delta():
                    return [Check.failed("model.Mixer.attention", "expected the attention slot")]


def tail_cases(model, layer, row, index, mixed):
    """The post-attention norm of the residual `row + mixed` against its hook, then the
    MLP of the fixture's normed row."""
    cfg = model.config
    shapes = model.shapes
    post = Qwen3_5Norm.rms_row(Qwen3_5Device.added(row, mixed, shapes.hidden), layer.post_norm, cfg, shapes)
    normed = hook_row(f"layers.{index}.post_attention_layernorm", 0, Expected.MODEL_HIDDEN)
    return [
        compare(f"model.Layer.post_norm.{index}", post, normed),
        compare(f"model.Mlp.swiglu.{index}", Qwen3_5Mlp.swiglu(Qwen3_5Device.constant(normed), layer.mlp, cfg, shapes), hook_row(f"layers.{index}.mlp", 0, Expected.MODEL_HIDDEN)),
    ]


def delta_mixer_cases(model, layer, mixer, row, normed, held, index):
    mixed = Qwen3_5DeltaNet.mix(normed, mixer, held, model.tiled, model.config, model.shapes)
    slot = Qwen3_5Cache.Delta(held=Qwen3_5Cache.DeltaSlot(conv=mixed.slot.conv, state=mixed.slot.state))
    grown = Qwen3_5Cache.grown(Qwen3_5Cache.Cache(slots=[]), slot)
    return listConcat([
        compare(f"model.Mixer.linear.Mixed.{index}", mixed.hidden, hook_row(f"layers.{index}.linear_attn", 0, Expected.MODEL_HIDDEN)),
        Check.equal(f"model.DeltaSlot.grown.{index}", len(grown.slots), 1),
    ], tail_cases(model, layer, row, index, mixed.hidden))


def attention_mixer_cases(model, layer, mixer, row, normed, held, index):
    mixed = Qwen3_5Attention.mix(normed, mixer, held, model.config, model.shapes)
    return listConcat([
        compare(f"model.Mixer.attention.Mixed.{index}", mixed.hidden, hook_row(f"layers.{index}.self_attn", 0, Expected.MODEL_HIDDEN)),
        Check.equal(f"model.AttentionSlot.positions.{index}", mixed.slot.positions, 1),
    ], tail_cases(model, layer, row, index, mixed.hidden))


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
                return walk_tokens(model, tokens, t + 1, next.walk.cache, listConcat(acc, listConcat(stage_cases(f"model_cached{t}", next, CACHED, t), listConcat(hook_cases(next, t), [logits_case(next, t)]))))


def logits_case(next, t):
    """The step's logits against the oracle's batched-run logits row `t`."""
    return compare(f"model_cached{t}_logits", next.logits, logits_row(t))


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
