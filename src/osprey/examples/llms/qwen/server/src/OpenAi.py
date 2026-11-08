# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=OpenAi
"""The OpenAI protocol endpoints ([LLM-API-OPENAI]).

`/v1/models` lists the served model and `/v1/chat/completions` answers a turn,
whole or as server-sent events. The two protocols disagree on one thing that
matters to a harness: Ollama's `tool_calls` carry their arguments as a JSON
object, and OpenAI's carry the same object as a JSON *string*. Both are built
from the same parsed call, in `call_of` here and in `Ollama.call_of` there.
"""

from qwen_server import Completions
from qwen_server import Requests as Q
from qwen_server import Wire

__all__ = ["models", "completions", "failed"]

DONE = "[DONE]"


def models():
    """`/v1/models`: the one model this port serves."""
    return Wire.ok(JsonObject([Wire.text_member("object", "list"), Wire.member("data", JsonArray([entry()]))]))


def entry():
    """One `/v1/models` entry."""
    return JsonObject([
        Wire.text_member("id", Q.MODEL),
        Wire.text_member("object", "model"),
        Wire.number_member("created", Q.EPOCH),
        Wire.text_member("owned_by", "osprey"),
    ])


def completions(model, request):
    """`/v1/chat/completions`: one assistant turn, streamed as SSE or answered whole."""
    return turn(model, request)


def turn(model, request):
    """The reply for a decoded request, once the served model is known."""
    match Completions.complete(model, request.prompt, request.sampling, request.tools):
        case Success(value=finished):
            reply_id = f"{random()}-{random()}"
            return Wire.reply(200, Wire.SSE, stream(events(finished, reply_id))) if request.stream else Wire.ok(completion(finished, reply_id))
        case Error(message=message):
            return failed(400, message)


def completion(turn, reply_id):
    """The whole chat completion: one choice carrying the turn's text and calls."""
    return JsonObject(listConcat(envelope(reply_id), [
        Wire.member("choices", JsonArray([choice(turn, reply_id)])),
        Wire.member("usage", usage(turn)),
    ]))


def envelope(reply_id):
    """The members every completion reply of this server carries."""
    return [Wire.text_member("id", f"chatcmpl-{reply_id}"), Wire.text_member("object", "chat.completion"), Wire.number_member("created", Q.EPOCH), Wire.text_member("model", Q.MODEL)]


def choice(turn, reply_id):
    """One completion choice over the finished turn."""
    return JsonObject([
        Wire.number_member("index", 0),
        Wire.member("message", message(turn, reply_id)),
        Wire.text_member("finish_reason", turn.reason),
    ])


def message(turn, reply_id):
    """The assistant message: its text, and its calls with arguments as JSON text."""
    return JsonObject(listConcat(listConcat([Wire.text_member("role", "assistant"), Wire.text_member("content", text_of(turn))], reasoning_of(turn)), calls_of(turn.calls, reply_id)))


def usage(turn):
    """The token counts OpenAI reports, and their sum."""
    return JsonObject([
        Wire.number_member("prompt_tokens", turn.prompt_tokens),
        Wire.number_member("completion_tokens", turn.eval_tokens),
        Wire.number_member("total_tokens", satAdd(turn.prompt_tokens, turn.eval_tokens)),
    ])


def events(turn, reply_id):
    """The SSE documents of one streamed turn, ending with the choice's finish reason."""
    pieces = mapList(turn.pieces, lambda piece: chunk(reply_id, [Wire.member("delta", delta_of(piece))], ""))
    calls = [] if len(turn.calls) == 0 else [chunk(reply_id, [Wire.member("delta", JsonObject([Wire.member("tool_calls", JsonArray(indexed(turn.calls, reply_id, True, 0, []))) ]))], "")]
    return listAppend(listConcat(pieces, calls), chunk(reply_id, [Wire.member("delta", JsonObject([]))], turn.reason))


def chunk(reply_id, members, finish):
    """One `chat.completion.chunk` document."""
    head = [Wire.text_member("id", f"chatcmpl-{reply_id}"), Wire.text_member("object", "chat.completion.chunk"), Wire.number_member("created", Q.EPOCH), Wire.text_member("model", Q.MODEL)]
    choice_members = listConcat(members, [Wire.number_member("index", 0), Wire.member("finish_reason", JsonNull if finish == "" else JsonString(finish))])
    return JsonObject(listConcat(head, [Wire.member("choices", JsonArray([JsonObject(choice_members)]))]))


def delta_of(piece):
    """A piece as a delta: its text, or its reasoning."""
    return JsonObject([Wire.text_member("content", piece.text)]) if piece.kind == Completions.TEXT else JsonObject([Wire.text_member("reasoning_content", piece.text)])


def text_of(turn):
    """The turn's free text."""
    return Completions.pieces_text(turn, Completions.TEXT)


def reasoning_of(turn):
    """The `reasoning_content` member when the turn reasoned, and nothing when it did not."""
    reasoned = Completions.pieces_text(turn, Completions.THINKING)
    return [] if reasoned == "" else [Wire.text_member("reasoning_content", reasoned)]


def calls_of(calls, reply_id):
    """The tool calls in a whole completion."""
    return [] if len(calls) == 0 else [Wire.member("tool_calls", JsonArray(indexed(calls, reply_id, False, 0, [])))]


def indexed(calls, reply_id, streamed, at, kept):
    """Encode calls in order, including the position for SSE clients."""
    match listGet(calls, at):
        case Success(value=call):
            return indexed(calls, reply_id, streamed, satAdd(at, 1), listAppend(kept, call_of(call, reply_id, streamed, at)))
        case Error():
            return kept


def call_of(call, reply_id, streamed, at):
    """Give each call a distinct ID and retain its JSON-text arguments."""
    index = [Wire.number_member("index", at)] if streamed else []
    return JsonObject(listConcat(index, [
        Wire.text_member("id", f"call_{reply_id}_{at}"),
        Wire.text_member("type", "function"),
        Wire.member("function", JsonObject([Wire.text_member("name", call.name), Wire.text_member("arguments", arguments_of(call))])),
    ]))


def arguments_of(call):
    """A call's arguments as the JSON text OpenAI clients parse."""
    return unwrap_or(Wire.encode(call.arguments), "{}")


def stream(events):
    """One SSE body: the turn's events, then the terminator OpenAI's clients stop at."""
    return Wire.sse(events) + f"data: {DONE}\n\n"


def failed(status, message):
    """An error in OpenAI's shape, which its client parses into `error.message`."""
    error = JsonObject([Wire.text_member("message", message), Wire.text_member("type", "invalid_request_error"), Wire.member("param", JsonNull), Wire.member("code", JsonNull)])
    return Wire.json_reply(status, JsonObject([Wire.member("error", error)]))
