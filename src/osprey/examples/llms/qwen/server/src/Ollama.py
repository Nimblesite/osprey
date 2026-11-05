# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Ollama
"""The Ollama protocol endpoints ([LLM-API-OLLAMA], [LLM-API-QUEUE], [LLM-API-LIMIT]).

The shapes are the ones `ollama-python`'s own models parse: `/api/tags` and
`/api/ps` carry one entry per served model, `/api/show` carries the model's
`model_info` (where Ollama reports the context length) and its `capabilities`,
and a failure is `{"error": message}` with a status the client raises
`ResponseError` for.
"""

from qwen_server import Requests as Q
from qwen_server import Wire

__all__ = ["version", "tags", "show", "ps", "not_found", "method_not_allowed", "summary", "details"]


def version():
    """`/api/version`: the protocol version this server speaks."""
    return Wire.ok(JsonObject([Wire.text_member("version", Q.VERSION)]))


def tags():
    """`/api/tags`: exactly one model, the one this port serves."""
    return Wire.ok(JsonObject([Wire.member("models", JsonArray([summary()]))]))


def ps():
    """`/api/ps`: the same model, loaded, with the context length it runs at."""
    return Wire.ok(JsonObject([Wire.member("models", JsonArray([running()]))]))


def show(name):
    """`/api/show` for `name`, or a 404 naming the model the server does not serve."""
    return details_response() if name == Q.MODEL else not_found(name)


def details_response():
    """The served model's capabilities, context length and tensor shape."""
    return Wire.ok(JsonObject([
        Wire.text_member("modified_at", Q.INSTANT),
        Wire.member("details", details()),
        Wire.member("model_info", model_info()),
        Wire.text_member("parameters", f"num_ctx {Q.CONTEXT}\ntemperature {Q.TEMPERATURE}"),
        Wire.member("capabilities", JsonArray([JsonString("completion"), JsonString("tools")])),
    ]))


def details():
    """Ollama's model details for the pinned weights file."""
    return JsonObject([
        Wire.text_member("format", "gguf"),
        Wire.text_member("family", Q.FAMILY),
        Wire.member("families", JsonArray([JsonString(Q.FAMILY)])),
        Wire.text_member("parameter_size", "27.8B"),
        Wire.text_member("quantization_level", Q.QUANTIZATION),
    ])


def model_info():
    """The GGUF metadata keys Ollama reports, including the context length."""
    return JsonObject([
        Wire.text_member("general.architecture", Q.FAMILY),
        Wire.number_member("general.parameter_count", Q.PARAMETERS),
        Wire.text_member("general.quantization", Q.QUANTIZATION),
        Wire.text_member("general.weights_file", Q.WEIGHT_FILE),
        Wire.text_member("general.huggingface_revision", Q.REVISION),
        Wire.number_member(f"{Q.FAMILY}.context_length", Q.CONTEXT),
        Wire.number_member(f"{Q.FAMILY}.block_count", Q.BLOCKS),
        Wire.number_member(f"{Q.FAMILY}.embedding_length", Q.EMBEDDING),
    ])


def summary():
    """One `/api/tags` entry. `digest` is absent until the weights are read."""
    return JsonObject([
        Wire.text_member("name", Q.MODEL),
        Wire.text_member("model", Q.MODEL),
        Wire.text_member("modified_at", Q.INSTANT),
        Wire.number_member("size", Q.WEIGHTS_BYTES),
        Wire.member("details", details()),
    ])


def running():
    """One `/api/ps` entry for the loaded model."""
    return JsonObject([
        Wire.text_member("name", Q.MODEL),
        Wire.text_member("model", Q.MODEL),
        Wire.number_member("size", Q.WEIGHTS_BYTES),
        Wire.number_member("size_vram", Q.WEIGHTS_BYTES),
        Wire.text_member("expires_at", Q.INSTANT),
        Wire.number_member("context_length", Q.CONTEXT),
        Wire.member("details", details()),
    ])


def not_found(name):
    """A 404 for a model the server does not serve, naming it."""
    return Wire.failed(404, f"model '{name}' not found")


def method_not_allowed(allowed):
    """A 405 for a route that exists under another method, naming the method."""
    return Wire.failed(405, f"method not allowed; {allowed}")
