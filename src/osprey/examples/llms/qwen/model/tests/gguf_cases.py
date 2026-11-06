# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=GgufCases
"""The GGUF loader against the fixture written as tiny GGUF files
(oracle/qwen3_5_gguf.cpython): the `F32` file reproduces every activation the
Transformers-layout loader does, the `Q8_0` file reproduces Transformers' activations
over gguf-py's decode of the same blocks, through a `Mapped` embedding and `Quant`
weights, and each `refuse-*` file is refused by name ([LLM-MODEL-ORACLE],
[LLM-WEIGHTS-MAPPED], [LLM-WEIGHTS-QUANT])."""
from testing import Check
from qwen_expected import Expected
from qwen_gguf_expected import ExpectedGguf
from gguf import Reader
from qwen import Qwen3_5Device, Qwen3_5Gguf, Qwen3_5Hf, Qwen3_5Model
from qwen_tests.ModelCases import CACHED, SINGLE, compare, fixture_params, fixture_row, flatten, greedy_case, hidden_row, hook_cases, input_tokens, model_config, refusal, started, stages_against, token_in

__all__ = ["FIXTURES", "gguf_cases"]

FIXTURES = "src/osprey/examples/llms/qwen/model/tests/fixtures/"
"""Where the oracle wrote the tiny files, relative to the repository root the suite runs from."""
Q8_CACHED = 5
"""`MODEL_Q8_HIDDEN_VALUES` index of the cached run's embeddings; its stages follow."""


def path(name):
    return FIXTURES + name


def gguf_cases():
    """Every GGUF loader case."""
    return flatten([size_cases(), header_cases(), f32_cases(), q8_cases(), refusal_cases()])


# --- the files ------------------------------------------------------------

def size_cases():
    """Each committed fixture is the file the oracle wrote, by size."""
    return toList(map(range(len(ExpectedGguf.MODEL_GGUF_FILES)), size_case))


def size_case(i):
    name = unwrap_or(listGet(ExpectedGguf.MODEL_GGUF_FILES, i), "")
    match fileSize(path(name)):
        case Success(value=size):
            return Check.equal(f"gguf_fixture_size[{name}]", size, unwrap_or(listGet(ExpectedGguf.MODEL_GGUF_SIZES, i), -1))
        case Error(message=message):
            return Check.failed(f"gguf_fixture_size[{name}]", toString(message))


def header_cases():
    """`open_file` reads the tensor table and `config_of` the fixture's config from the keys."""
    match Qwen3_5Gguf.open_file(path("tiny-f32.gguf")):
        case Error(message=message):
            return [Check.failed("gguf_open", message)]
        case Success(value=source):
            return listConcat([Check.passed("gguf_open"), names_case("gguf_tensor_names", listSort(Reader.tensor_names(source.file)), ExpectedGguf.MODEL_GGUF_TENSORS)], config_cases(source))


def names_case(name, got, want):
    """Two string lists, equal element for element."""
    return Check.truth(name, structuralEquals(got, want), f"got {join(got, ' ')}, want {join(want, ' ')}")


def read_config(source):
    with Qwen3_5Gguf.refusing:
        return Success(value=Qwen3_5Gguf.config_of(source.file))


def config_cases(source):
    match read_config(source):
        case Error(message=message):
            return [Check.failed("gguf_config", message)]
        case Success(value=got):
            return config_checks(got, model_config())


def config_checks(got, want):
    """Every field of the file's config against the fixture's."""
    return [
        Check.equal("gguf_config.hidden_size", got.hidden_size, want.hidden_size),
        Check.equal("gguf_config.intermediate_size", got.intermediate_size, want.intermediate_size),
        Check.equal("gguf_config.vocab_size", got.vocab_size, want.vocab_size),
        Check.equal("gguf_config.num_hidden_layers", got.num_hidden_layers, want.num_hidden_layers),
        Check.equal("gguf_config.num_attention_heads", got.num_attention_heads, want.num_attention_heads),
        Check.equal("gguf_config.num_key_value_heads", got.num_key_value_heads, want.num_key_value_heads),
        Check.equal("gguf_config.head_dim", got.head_dim, want.head_dim),
        Check.close("gguf_config.partial_rotary_factor", got.partial_rotary_factor, want.partial_rotary_factor, 0.0, 0.0),
        Check.close("gguf_config.rope_theta", got.rope_theta, want.rope_theta, 0.0, 0.0),
        Check.close("gguf_config.rms_norm_eps", got.rms_norm_eps, want.rms_norm_eps, 1e-7, 0.0),
        Check.equal("gguf_config.linear_num_key_heads", got.linear_num_key_heads, want.linear_num_key_heads),
        Check.equal("gguf_config.linear_num_value_heads", got.linear_num_value_heads, want.linear_num_value_heads),
        Check.equal("gguf_config.linear_key_head_dim", got.linear_key_head_dim, want.linear_key_head_dim),
        Check.equal("gguf_config.linear_value_head_dim", got.linear_value_head_dim, want.linear_value_head_dim),
        Check.equal("gguf_config.linear_conv_kernel_dim", got.linear_conv_kernel_dim, want.linear_conv_kernel_dim),
        names_case("gguf_config.layer_types", got.layer_types, want.layer_types),
    ]


# --- the dense file -------------------------------------------------------

def f32_cases():
    """The `F32` file: the same activations as the Transformers-layout load, a tied `Dense` head."""
    match Qwen3_5Gguf.load_path(path("tiny-f32.gguf")):
        case Error(message=message):
            return [Check.failed("gguf_f32_load", message)]
        case Success(value=model):
            return flatten([[Check.passed("gguf_f32_load"), Check.equal("gguf_f32_tiled", model.tiled, 1), dense_head_case(model.head)], head_cases(model), single("gguf_f32", model, lambda k: hidden_row(SINGLE + k, 0)), cached("gguf_f32", model, lambda k, t: hidden_row(CACHED + k, t), hook_cases)])


def dense_head_case(head):
    match head:
        case Qwen3_5Device.Dense(values=_):
            return Check.passed("gguf_f32_head_dense")
        case Qwen3_5Device.Quant(blocks=_, type=type):
            return Check.failed("gguf_f32_head_dense", f"a {type} head")


def head_cases(model):
    """The tied head's logits match the Transformers-layout model's for the same token."""
    match Qwen3_5Hf.load(fixture_params(Expected.MODEL_PARAM_VALUES), model_config()):
        case Error(message=message):
            return [Check.failed("gguf_f32_head", message)]
        case Success(value=reference):
            return started(model, "gguf_f32_head", lambda cache: head_run(model, reference, cache))


def head_run(model, reference, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("gguf_f32_head", message)]
        case Success(value=next):
            return against_reference(next, Qwen3_5Model.decode(reference, cache, token_in(input_tokens(0), 0)))


def against_reference(next, decoded):
    match decoded:
        case Error(message=message):
            return [Check.failed("gguf_f32_head_reference", message)]
        case Success(value=want):
            return listAppend(logits_case("gguf_f32_head_logits", next, want.logits), Check.equal("gguf_f32_head_greedy", next.best, want.best))


def logits_case(name, next, want):
    match fromGpu(want):
        case Success(value=values):
            return [compare(name, next.logits, values)]
        case Error(message=message):
            return [Check.failed(name, toString(message))]


# --- one token and eight cached tokens ---------------------------------------

def single(label, model, row_of):
    """`MODEL_INPUTS[0]` from a fresh cache against `row_of(stage)`."""
    return started(model, label, lambda cache: single_run(label, model, cache, row_of))


def single_run(label, model, cache, row_of):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed(label, message)]
        case Success(value=next):
            return listAppend(stages_against(label, next, row_of), greedy_case(label, next))


def cached(label, model, row_of, extra):
    """`MODEL_INPUTS[2]` one token at a time against `row_of(stage, t)`, plus `extra(next, t)`."""
    return started(model, f"{label}_cached", lambda cache: walk(f"{label}_cached", model, input_tokens(2), 0, cache, [], row_of, extra))


def walk(label, model, tokens, t, cache, acc, row_of, extra):
    if t == len(tokens):
        return acc
    else:
        match Qwen3_5Model.decode(model, cache, token_in(tokens, t)):
            case Error(message=message):
                return listAppend(acc, Check.failed(f"{label}{t}", message))
            case Success(value=next):
                return walk(label, model, tokens, t + 1, next.walk.cache, listConcat(acc, listConcat(stages_against(f"{label}{t}", next, lambda k: row_of(k, t)), extra(next, t))), row_of, extra)


def no_extra(_next, _t):
    return []


# --- the quantized file ---------------------------------------------------

def q8_row(k, t):
    """Row `t` of `MODEL_Q8_HIDDEN_VALUES[k]`."""
    return fixture_row(ExpectedGguf.MODEL_Q8_HIDDEN_VALUES, k, t)


def q8_cases():
    """The `Q8_0` file: a `Mapped` embedding, `Quant` weights and a separate `Quant` head
    reproduce Transformers' activations over the dequantized weights."""
    match Qwen3_5Gguf.load_path(path("tiny-q8_0.gguf")):
        case Error(message=message):
            return [Check.failed("gguf_q8_load", message)]
        case Success(value=model):
            return flatten([[Check.passed("gguf_q8_load"), embedding_case(model.embedding), quant_head_case(model.head)], single("gguf_q8", model, lambda k: q8_row(k, 0)), cached("gguf_q8", model, lambda k, t: q8_row(Q8_CACHED + k, t), no_extra)])


def embedding_case(embedding):
    match embedding:
        case Qwen3_5Model.Mapped(path=_, offset=_, row_bytes=row_bytes, type=type, basis=_):
            return Check.equal("gguf_q8_embedding", f"{type} rows of {row_bytes} bytes", "Q8_0 rows of 68 bytes")
        case Qwen3_5Model.Table(values=_):
            return Check.failed("gguf_q8_embedding", "a dense table")


def quant_head_case(head):
    match head:
        case Qwen3_5Device.Quant(blocks=_, type=type):
            return Check.equal("gguf_q8_head", type, "Q8_0")
        case Qwen3_5Device.Dense(values=_):
            return Check.failed("gguf_q8_head", "a dense head")


# --- refusals -------------------------------------------------------------

def load(name):
    return Qwen3_5Gguf.load_path(path(name))


def refusal_cases():
    """Each file that lacks something is refused by name before any token runs."""
    return [
        refusal("gguf_refuse_arch", load("refuse-arch.gguf"), "qwen gguf: architecture `qwen3` is not `qwen35`"),
        refusal("gguf_refuse_missing", load("refuse-missing.gguf"), "qwen gguf: missing tensor `blk.0.attn_norm.weight`"),
        refusal("gguf_refuse_shape", load("refuse-shape.gguf"), "qwen gguf: tensor `token_embd.weight` is [32, 128], not [64, 128]"),
        refusal("gguf_refuse_type", load("refuse-type.gguf"), "qwen gguf: tensor `token_embd.weight` is Q4_0, which the device does not decode; expected one of F32, Q8_0, Q3_K, Q4_K, Q5_K, Q6_K, IQ4_NL, IQ4_XS, IQ3_S"),
        refusal("gguf_refuse_key", load("refuse-key.gguf"), "qwen gguf: gguf: the file has no metadata key `qwen35.ssm.inner_size`"),
        refusal("gguf_refuse_heads", load("refuse-heads.gguf"), "qwen gguf: ssm.state_size 16 is not the value head size 8; the delta rule keeps key and value heads the same size"),
        prefixed("gguf_refuse_truncated", load("refuse-truncated.gguf"), "qwen gguf: " + path("refuse-truncated.gguf") + ": gguf: "),
        prefixed("gguf_refuse_absent", load("absent.gguf"), "qwen gguf: " + path("absent.gguf") + ": "),
    ]


def prefixed(name, result, want):
    match result:
        case Success(value=_):
            return Check.failed(name, "loaded a file that should have been refused")
        case Error(message=message):
            return Check.truth(name, startsWith(message, want), message)
