# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Requests
"""The served model's declared shape and the records the wire layer builds.

The metadata here is the model plan 0049 pins
(docs/specs/applications/LocalCodingModel.md *Model and weights* and
*Memory budget*): `Qwen/Qwen3.8-27B` at revision `1d4bf0f2…` with weights from
`unsloth/Qwen3.8-27B-GGUF` revision `4ca7207…`, quantized `UD-Q4_K_XL` at 64K
context. The inference handler behind these endpoints is the fixture model in
`TableModel.py` until plan 0049 Phase 3 lands the real weights; the declared
shape is what the weights *will* be, and no field below claims a byte the
server has read. `digest` is therefore absent rather than invented.

No clock builtin exists in Osprey ([BUILTIN-TERM] has none), so the timestamps
the protocols require are one fixed instant rather than a measured one.
"""

from qwen_server import Prompts
from qwen_server import Wire

__all__ = [
    "MODEL", "FAMILY", "QUANTIZATION", "CONTEXT", "BLOCKS", "EMBEDDING",
    "PARAMETERS", "WEIGHTS_BYTES", "WEIGHT_FILE", "REVISION", "INSTANT", "EPOCH",
    "VERSION", "TEMPERATURE",
    "Sampling", "Chat", "Generate",
    "show_request", "chat_request", "generate_request",
]

MODEL = "qwen3.8:27b"
"""The name both protocols serve, and require of requests as their `model`."""
FAMILY = "qwen3_5"
"""The family and `general.architecture` `/api/show` reports, prefixing its `model_info` keys."""
QUANTIZATION = "UD-Q4_K_XL"
"""The quantization of the served weights, as Ollama and the GGUF name it."""
CONTEXT = 65536
"""The served context length in tokens, and the default `num_ctx`."""
BLOCKS = 64
"""The transformer block count `/api/show` reports."""
EMBEDDING = 5120
"""The embedding width `/api/show` reports."""
PARAMETERS = 27781427952
"""The declared parameter count `/api/show` reports."""
WEIGHTS_BYTES = 18897914266
"""The size in bytes `/api/ps` reports for the weights and their VRAM residency."""
WEIGHT_FILE = "Qwen3.8-27B-UD-Q4_K_XL.gguf"
"""The GGUF file the served weights come from."""
REVISION = "4ca720788d1e01f1bff70c033e0d0028fd02e502"
"""The Hugging Face revision the weights were taken at."""
INSTANT = "2026-11-05T00:00:00Z"
"""The one timestamp every protocol field carries, Osprey having no clock."""
EPOCH = 1762300800
"""`INSTANT` as a Unix epoch second, OpenAI's `created`."""
VERSION = "0.1.0"
"""The server version `/api/version` reports."""
TEMPERATURE = 0.0
"""The served default sampling temperature."""
NUM_PREDICT = 256


class Sampling:
    """Ollama's generation options, reduced to the ones the server honours."""
    num_ctx: int
    num_predict: int
    temperature: float
    seed: int


class Chat:
    """A chat turn: the messages rendered, the tools advertised, and how to run it."""
    model: str
    prompt: str
    tools: Json
    stream: bool
    sampling: Sampling


class Generate:
    """A completion request: its prompt as served, and how to run it."""
    model: str
    prompt: str
    stream: bool
    sampling: Sampling


def show_request(body):
    """The model name from an `/api/show` body ([LLM-API-OLLAMA])."""
    match Wire.decode(body):
        case Success(value=document):
            return required(Wire.text(document, "model"))
        case Error(message=message):
            return Error(message=message)


def required(name):
    """A required model name, or the rejection both protocols use."""
    return Success(value=name) if name != "" else Error(message="model is required")


def default_sampling():
    """The served defaults: the model's context, a bounded reply, greedy sampling."""
    return Sampling(num_ctx=CONTEXT, num_predict=NUM_PREDICT, temperature=TEMPERATURE, seed=0)


def sampling_of(options):
    """The request's `options` object over the served defaults."""
    return Sampling(
        num_ctx=positive(Wire.number(options, "num_ctx"), CONTEXT),
        num_predict=positive(Wire.number(options, "num_predict"), NUM_PREDICT),
        temperature=decimal(options, "temperature", TEMPERATURE),
        seed=Wire.number(options, "seed"),
    )


def positive(value, fallback):
    """`value` when it is positive, and the served default otherwise."""
    return value if value > 0 else fallback


def decimal(options, name, fallback):
    """A numeric member written as a JSON float or as an integer."""
    match jsonFloat(Wire.field(options, name)):
        case Success(value=value):
            return value
        case Error():
            return fallback


def advertised(document):
    """The request's `tools`, or an empty list when it names none: a chat turn always runs
    through the tool parser, so the model's `<think>` reasoning is returned apart from its
    text; only `/api/generate` keeps the raw completion."""
    match Wire.field(document, "tools"):
        case JsonNull:
            return JsonArray([])
        case value:
            return value


def chat_request(body):
    """An `/api/chat` or `/v1/chat/completions` body as a `Chat` ([LLM-API-OLLAMA])."""
    match Wire.decode(body):
        case Success(value=document):
            return named(Wire.text(document, "model"), document)
        case Error(message=message):
            return Error(message=message)


def named(model, document):
    """Build a `Chat` once its body decoded and its model name is present."""
    match required(model):
        case Success(value=name):
            return Success(value=Chat(
                model=name,
                prompt=Prompts.render(Wire.items(Wire.field(document, "messages"))),
                tools=advertised(document),
                stream=Wire.flag(document, "stream"),
                sampling=sampling_of(Wire.field(document, "options")),
            ))
        case Error(message=message):
            return Error(message=message)


def generate_request(body):
    """An `/api/generate` body as a `Generate` ([LLM-API-OLLAMA])."""
    match Wire.decode(body):
        case Success(value=document):
            return named_generate(Wire.text(document, "model"), document)
        case Error(message=message):
            return Error(message=message)


def named_generate(model, document):
    """Build a `Generate` once its body decoded and its model name is present."""
    match required(model):
        case Success(value=name):
            prompt = Wire.text(document, "prompt")
            system = Wire.text(document, "system")
            return Success(value=Generate(
                model=name,
                prompt=prompt if Wire.flag(document, "raw") else Prompts.render_single(prompt, system),
                stream=Wire.flag(document, "stream"),
                sampling=sampling_of(Wire.field(document, "options")),
            ))
        case Error(message=message):
            return Error(message=message)
