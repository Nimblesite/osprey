# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Wire
"""The wire boundary: JSON text in, records out, HTTP replies and stream framing back.

Every JSON read and write in the server happens here, so the protocol handlers
work on typed records and never on `Json` values ([FLAVOR-PYTHON-BOUNDARY],
docs/specs/ecosystem/0043-PythonFlavorPorts.md). Readers are total: a member
that is absent or of another kind reads as the empty value, and a request whose
required members are absent is rejected by `decode` before a record exists.
"""

__all__ = [
    "member", "text_member", "number_member", "flag_member",
    "encode", "reply", "ok", "json_reply", "failed", "text_reply",
    "path_of", "decode", "field", "text", "number", "flag", "items", "openai_history",
    "ndjson", "sse", "NDJSON", "SSE",
]

JSON = "application/json"
NDJSON = "application/x-ndjson"
"""The media type of Ollama's streamed replies."""
SSE = "text/event-stream"
"""The media type of OpenAI's streamed replies."""


def member(name, value):
    """One object member, keeping document order."""
    return JsonMember(name, value)


def text_member(name, value):
    """A string member."""
    return JsonMember(name, JsonString(value))


def number_member(name, value):
    """An integer member."""
    return JsonMember(name, JsonInt(value))


def flag_member(name, value):
    """A boolean member."""
    return JsonMember(name, JsonBool(value))


def encode(document):
    """`document` as compact RFC 8259 text, or the encoder's error."""
    return jsonEncode(document)


def reply(status, media_type, body):
    """A complete HTTP reply; the runtime computes Content-Length from `body`."""
    return HttpResponse(status=status, headers=f"Content-Type: {media_type}\r\n", contentType=media_type, streamFd=-1, isComplete=True, partialBody=body)


def ok(document):
    """A 200 reply carrying `document`, or a 500 naming the encoder's failure."""
    return json_reply(200, document)


def json_reply(status, document):
    """A reply carrying `document` at `status`, or a 500 naming the encoder's failure."""
    match encode(document):
        case Success(value=body):
            return reply(status, JSON, body)
        case Error(message=message):
            return failed(500, message)


def failed(status, message):
    """An error reply in the shape both protocols use: `{"error": message}`."""
    match jsonQuote(message):
        case Success(value=quoted):
            return reply(status, JSON, f"{{\"error\":{quoted}}}")
        case Error(message=problem):
            return text_reply(500, f"{message}\n{problem}")


def text_reply(status, body):
    """A plain-text reply, for a failure that cannot be spelled as JSON."""
    return reply(status, "text/plain; charset=utf-8", body)


def path_of(url):
    """The path of a request target, without its query string."""
    match indexOf(url, "?"):
        case Success(value=at):
            return take(url, at)
        case Error():
            return url


def decode(body):
    """A request body as one JSON document, or an error naming the JSON fault."""
    match jsonDecode(body):
        case Success(value=document):
            return Success(value=document)
        case Error(message=message):
            return Error(message=f"request body: {message}")


def field(document, name):
    """The member `name` of a JSON object, or `JsonNull` when it is absent."""
    match jsonField(document, name):
        case Success(value=value):
            return value
        case Error():
            return JsonNull


def text(document, name):
    """The string member `name`, or an empty string."""
    match jsonText(field(document, name)):
        case Success(value=value):
            return value
        case Error():
            return ""


def number(document, name):
    """The integer member `name`, or 0."""
    match jsonInt(field(document, name)):
        case Success(value=value):
            return value
        case Error():
            return 0


def flag(document, name):
    """The boolean member `name`, or false."""
    match jsonBool(field(document, name)):
        case Success(value=value):
            return value
        case Error():
            return False


def items(value):
    """The elements of a JSON array, or none."""
    match value:
        case JsonArray(values):
            return values
        case _:
            return []


def openai_history(messages):
    """Convert OpenAI's serialized function arguments before rendering the chat template."""
    return mapList(messages, openai_message)


def openai_message(message):
    return map_field(message, "tool_calls", openai_calls) if text(message, "role") == "assistant" else message


def openai_calls(calls):
    match calls:
        case JsonArray(values):
            return JsonArray(mapList(values, lambda call: map_field(call, "function", openai_function)))
        case _:
            return calls


def openai_function(function):
    return map_field(function, "arguments", openai_arguments)


def openai_arguments(arguments):
    match arguments:
        case JsonString(raw):
            match jsonDecode(raw):
                case Success(value=decoded):
                    return object_or(decoded, arguments)
                case Error():
                    return arguments
        case _:
            return arguments


def object_or(decoded, fallback):
    match decoded:
        case JsonObject(members):
            return JsonObject(members)
        case _:
            return fallback


def map_field(document, name, transform):
    match document:
        case JsonObject(members):
            return JsonObject(mapList(members, lambda item: mapped_member(item, name, transform)))
        case _:
            return document


def mapped_member(item, name, transform):
    match item:
        case JsonMember(key, value):
            return member(key, transform(value)) if key == name else item


def quoted(message):
    """`message` as a JSON string literal, or a spelled-out escape when it cannot be one."""
    match jsonQuote(message):
        case Success(value=literal):
            return literal
        case Error():
            return "\"error message is not encodable\""


def ndjson(documents):
    """NDJSON: one encoded document per line, each newline-terminated ([LLM-API-OLLAMA])."""
    return framed(documents, "", "\n")


def sse(documents):
    """Server-sent events: each document as one `data:` line and a blank line ([LLM-API-OPENAI])."""
    return framed(documents, "data: ", "\n\n")


def framed(documents, before, after):
    """Every document on its own `before`…`after` line, concatenated."""
    return join(foldList(documents, [], lambda lines, document: listAppend(lines, f"{before}{encoded(document)}{after}")), "")


def encoded(document):
    """One encoded document, or an error document naming the encoder's failure."""
    match encode(document):
        case Success(value=body):
            return body
        case Error(message=message):
            return f"{{\"error\":{quoted(message)}}}"
