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
    "encode", "reply", "ok", "failed", "text_reply",
    "path_of", "decode", "field", "text", "number", "flag", "items",
]

JSON = "application/json"


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
    match encode(document):
        case Success(value=body):
            return reply(200, JSON, body)
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


def quoted(message):
    """`message` as a JSON string literal, or a spelled-out escape when it cannot be one."""
    match jsonQuote(message):
        case Success(value=literal):
            return literal
        case Error():
            return "\"error message is not encodable\""
