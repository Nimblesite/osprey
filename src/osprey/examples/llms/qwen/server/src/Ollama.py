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

from qwen_server import Completions
from qwen_server import Requests as Q
from qwen_server import Wire

__all__ = ["version", "tags", "show", "ps", "chat", "generate"]


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


def chat(model, request):
    """`/api/chat`: one assistant turn, streamed as NDJSON or answered whole."""
    return completed_turn(model, request, chat_turn_of, request.tools)


def generate(model, request):
    """`/api/generate`: the same turn as one completion, streamed or answered whole."""
    return completed_turn(model, request, generate_turn_of, JsonNull)


def completed_turn(model, request, turn_of, tools):
    """The complete or streamed reply for a turn `turn_of` renders."""
    match Completions.complete(model, request.prompt, request.sampling, tools):
        case Success(value=turn):
            return turn_of(request, turn)
        case Error(message=message):
            return Wire.failed(400, message)


def chat_turn_of(request, turn):
    """The chat reply, streamed as NDJSON or answered whole."""
    return Wire.reply(200, Wire.NDJSON, Wire.ndjson(chat_lines(request, turn))) if request.stream else Wire.ok(chat_turn(request, turn))


def generate_turn_of(request, turn):
    """The completion reply, streamed as NDJSON or answered whole."""
    return Wire.reply(200, Wire.NDJSON, Wire.ndjson(generate_lines(request, turn))) if request.stream else Wire.ok(generate_turn(request, turn))


def chat_turn(request, turn):
    """The whole chat reply: one message carrying every piece and call."""
    message = [Wire.member("message", message_of(text_of(turn), thinking_of(turn), turn.calls))]
    return JsonObject(listConcat(envelope(request), listConcat(message, completed(turn))))


def generate_turn(request, turn):
    """The whole completion reply: the turn's text as `response`."""
    return JsonObject(listConcat(envelope(request), listConcat([Wire.text_member("response", text_of(turn))], completed(turn))))


def chat_lines(request, turn):
    """The streamed chat turn: one line per piece, one for the calls, one to close."""
    pieces = mapList(turn.pieces, lambda piece: chunk(request, [Wire.member("message", message_of(piece_text(piece), piece_think(piece), []))]))
    closing = [Wire.member("message", message_of("", "", []))]
    return listAppend(listConcat(pieces, called(request, turn)), final(request, closing, turn))


def generate_lines(request, turn):
    """The streamed completion: one line per piece, then the line that closes it."""
    pieces = mapList(turn.pieces, lambda piece: chunk(request, [Wire.text_member("response", piece_text(piece))]))
    return listAppend(pieces, final(request, [Wire.text_member("response", "")], turn))


def called(request, turn):
    """The chunk carrying the turn's tool calls, when it made any."""
    return [] if len(turn.calls) == 0 else [chunk(request, [Wire.member("message", message_of("", "", turn.calls))])]


def chunk(request, members):
    """A stream line for a turn that is not over."""
    return JsonObject(listConcat(envelope(request), listConcat(members, [Wire.flag_member("done", False)])))


def final(request, members, turn):
    """The stream line that closes a turn, with why it ended and what it cost."""
    return JsonObject(listConcat(envelope(request), listConcat(members, completed(turn))))


def envelope(request):
    """The members every reply of this turn carries."""
    return [Wire.text_member("model", request.model), Wire.text_member("created_at", Q.INSTANT)]


def completed(turn):
    """`done`, why the turn ended, and the token counts Ollama reports."""
    return [Wire.flag_member("done", True), Wire.text_member("done_reason", turn.reason), Wire.number_member("prompt_eval_count", turn.prompt_tokens), Wire.number_member("eval_count", turn.eval_tokens)]


def message_of(text, thinking, calls):
    """An assistant message carrying `text`, its reasoning and its calls."""
    reasoned = [] if thinking == "" else [Wire.text_member("thinking", thinking)]
    return JsonObject(listConcat(listConcat([Wire.text_member("role", "assistant"), Wire.text_member("content", text)], reasoned), calls_of(calls)))


def text_of(turn):
    """The turn's free text, in order."""
    return Completions.pieces_text(turn, Completions.TEXT)


def thinking_of(turn):
    """The turn's reasoning, in order."""
    return Completions.pieces_text(turn, Completions.THINKING)


def piece_text(piece):
    """A piece's text when it is free text, and nothing when it is reasoning."""
    return piece.text if piece.kind == Completions.TEXT else ""


def piece_think(piece):
    """A piece's text when it is reasoning, and nothing when it is free text."""
    return piece.text if piece.kind == Completions.THINKING else ""


def calls_of(calls):
    """The `tool_calls` member when the turn called tools, and nothing when it did not."""
    return [] if len(calls) == 0 else [Wire.member("tool_calls", JsonArray(mapList(calls, call_of)))]


def call_of(call):
    """One tool call, its arguments as a JSON object ([LLM-API-OLLAMA])."""
    function = JsonObject([Wire.text_member("name", call.name), Wire.member("arguments", call.arguments)])
    return JsonObject([Wire.member("function", function)])


def not_found(name):
    """A 404 for a model the server does not serve, naming it."""
    return Wire.failed(404, f"model '{name}' not found")


def method_not_allowed(allowed):
    """A 405 for a route that exists under another method, naming the method."""
    return Wire.failed(405, f"method not allowed; {allowed}")
