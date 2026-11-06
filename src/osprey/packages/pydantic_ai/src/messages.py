# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Messages
"""pydantic_ai.messages: the parts of a request and of a response, the two
message kinds, the streaming events and the JSON wire form.

Port of pydantic_ai/messages.py (2.46.0). The dataclasses become three sum
types: `ModelRequestPart` (SystemPromptPart, UserPromptPart, ToolReturnPart,
RetryPromptPart, InstructionPart), `ModelResponsePart` (TextPart, ThinkingPart,
ToolCallPart) and `ModelMessage`, which wraps the `ModelRequest` and
`ModelResponse` records as `Request(request=)` / `Response(response=)` where
CPython's history list holds either directly. Optional
CPython fields are spelled as empty strings (`instructions`, `tool_name`,
`finish_reason`, `provider_response_id`); timestamps, file content, retries
of structured output and the native-tool parts are not ported. The methods
become functions: `ModelResponse.text` is `text`, `ToolCallPart.args_as_dict`
is `args_as_dict`, `RetryPromptPart.model_response` is `retry_model_response`,
and `ModelMessagesTypeAdapter.dump_python(mode="json")` / `validate_python`
are `dump_messages` / `validate_messages`, reading and writing the same
`kind`/`part_kind` discriminated objects.
"""
from pydantic_ai import JsonUtil as J
from pydantic_ai import Usage

__all__ = [
    "ModelRequestPart", "ModelResponsePart", "ModelRequest", "ModelResponse", "ModelMessage", "ModelResponsePartDelta", "ModelResponseStreamEvent",
    "of_request", "of_response", "responses", "requests", "dump_request", "dump_response",
    "model_response", "model_request", "text", "tool_calls", "has_tool_calls", "args_as_dict", "args_as_json_str",
    "retry_model_response", "tool_call_id_of", "tool_name_of", "dump_messages", "validate_messages",
    "dump_message", "validate_message", "is_response", "is_request", "request_parts", "response_parts",
    "generate_tool_call_id", "part_text", "is_text", "is_tool_call", "collect",
]


@union
class ModelRequestPart:
    """A part of a `ModelRequest`: what the program sends the model."""
    class SystemPromptPart:
        """A system prompt."""
        content: str
    class UserPromptPart:
        """The user's text."""
        content: str
    class ToolReturnPart:
        """What a tool returned for the call `tool_call_id`."""
        tool_name: str
        content: str
        tool_call_id: str
    class RetryPromptPart:
        """Feedback asking the model to try again; `tool_name` is empty for a plain validation retry."""
        content: str
        tool_name: str
        tool_call_id: str
    class InstructionPart:
        """An instruction the agent adds to every request."""
        content: str


@union
class ModelResponsePart:
    """A part of a `ModelResponse`: what the model answered."""
    class TextPart:
        """Plain text."""
        content: str
    class ThinkingPart:
        """The model's reasoning text."""
        content: str
    class ToolCallPart:
        """A request to call `tool_name` with the JSON-object text `args`."""
        tool_name: str
        args: str
        tool_call_id: str


class ModelRequest:
    """A request to the model; `instructions` is empty when none are given."""
    parts: list[ModelRequestPart]
    instructions: str


class ModelResponse:
    """A response from the model."""
    parts: list[ModelResponsePart]
    usage: Usage.RequestUsage
    model_name: str
    provider_name: str
    provider_response_id: str
    finish_reason: str


@union
class ModelMessage:
    """One message of a conversation: CPython's `ModelRequest | ModelResponse`."""
    class Request:
        """A `ModelRequest` in the history."""
        request: ModelRequest
    class Response:
        """A `ModelResponse` in the history."""
        response: ModelResponse


@union
class ModelResponsePartDelta:
    """A streamed change to a response part."""
    class TextPartDelta:
        """More text for a `TextPart`."""
        content_delta: str
    class ThinkingPartDelta:
        """More text for a `ThinkingPart`."""
        content_delta: str
    class ToolCallPartDelta:
        """More of a `ToolCallPart`."""
        tool_name_delta: str
        args_delta: str
        tool_call_id: str


@union
class ModelResponseStreamEvent:
    """An event of a streamed response."""
    class PartStartEvent:
        """A new part at `index`, with whatever content it opened with."""
        index: int
        part: ModelResponsePart
    class PartDeltaEvent:
        """A change to the part at `index`."""
        index: int
        delta: ModelResponsePartDelta
    class PartEndEvent:
        """The part at `index` is complete."""
        index: int
        part: ModelResponsePart
    class FinalResultEvent:
        """The response holds the run's final result."""
        tool_name: str
        tool_call_id: str


def model_request(parts):
    """`ModelRequest(parts=parts)`: no instructions."""
    return ModelRequest(parts=parts, instructions="")


def model_response(parts, model_name):
    """`ModelResponse(parts=parts, model_name=model_name)`: zero usage, no provider details."""
    return ModelResponse(parts=parts, usage=Usage.request_usage(), model_name=model_name, provider_name="", provider_response_id="", finish_reason="")


def generate_tool_call_id(n) -> str:
    """A tool call id for the n-th generated call (CPython draws a random one)."""
    return f"pyd_ai_{n}"


def of_request(request) -> ModelMessage:
    """A `ModelRequest` as a history message."""
    return Request(request=request)


def of_response(response) -> ModelMessage:
    """A `ModelResponse` as a history message."""
    return Response(response=response)


def is_response(message) -> bool:
    """Whether a history message is a `ModelResponse`."""
    match message:
        case Request():
            return False
        case Response():
            return True


def is_request(message) -> bool:
    """Whether a history message is a `ModelRequest`."""
    return not is_response(message)


def request_parts(message) -> list[ModelRequestPart]:
    """The parts of a request, `[]` for a response."""
    match message:
        case Request(request=request):
            return request.parts
        case Response():
            return []


def response_parts(message) -> list[ModelResponsePart]:
    """The parts of a response, `[]` for a request."""
    match message:
        case Request():
            return []
        case Response(response=response):
            return response.parts


def responses(messages) -> list[ModelResponse]:
    """Every `ModelResponse` in the history, in order."""
    return foldList(messages, [], lambda acc, m: response_step(acc, m))


def response_step(acc, message):
    match message:
        case Request():
            return acc
        case Response(response=response):
            return listAppend(acc, response)


def requests(messages) -> list[ModelRequest]:
    """Every `ModelRequest` in the history, in order."""
    return foldList(messages, [], lambda acc, m: request_step(acc, m))


def request_step(acc, message):
    match message:
        case Request(request=request):
            return listAppend(acc, request)
        case Response():
            return acc


def part_text(part) -> str:
    """A text part's content; empty for any other part."""
    match part:
        case TextPart(content=content):
            return content
        case _:
            return ""


def is_text(part) -> bool:
    """Whether a response part is a `TextPart`."""
    match part:
        case TextPart():
            return True
        case _:
            return False


def is_tool_call(part) -> bool:
    """Whether a response part is a `ToolCallPart`."""
    match part:
        case ToolCallPart():
            return True
        case _:
            return False


class TextFold:
    """Accumulator of `text`: the closed text groups, the open group and whether one is open."""
    texts: list[str]
    current: str
    open: bool


def text_step(acc, part):
    if is_text(part):
        return TextFold(texts=acc.texts, current=acc.current + part_text(part), open=True)
    elif acc.open:
        return TextFold(texts=listAppend(acc.texts, acc.current), current="", open=False)
    else:
        return acc


def text(response) -> str:
    """`ModelResponse.text`: adjacent text parts joined, separated groups split by a blank line; empty when there is no text."""
    folded = foldList(response.parts, TextFold(texts=[], current="", open=False), text_step)
    groups = listAppend(folded.texts, folded.current) if folded.open else folded.texts
    return join(groups, "\n\n")


def tool_calls(response) -> list[ModelResponsePart]:
    """`ModelResponse.tool_calls`: the tool call parts in order."""
    return filterList(response.parts, is_tool_call)


def has_tool_calls(response) -> bool:
    """`ModelResponse.tool_calls` is non-empty."""
    return len(tool_calls(response)) > 0


def tool_name_of(part) -> str:
    """`ToolCallPart.tool_name`; empty for any other part."""
    match part:
        case ToolCallPart(tool_name=tool_name):
            return tool_name
        case _:
            return ""


def tool_call_id_of(part) -> str:
    """`ToolCallPart.tool_call_id`; empty for any other part."""
    match part:
        case ToolCallPart(tool_call_id=tool_call_id):
            return tool_call_id
        case _:
            return ""


def args_text(part):
    match part:
        case ToolCallPart(args=args):
            return args
        case _:
            return ""


def args_as_dict(part) -> dict[str, Json]:
    """`ToolCallPart.args_as_dict()`: the arguments; `{}` when empty and `{'INVALID_JSON': args}` when not a JSON object."""
    raw = args_text(part)
    if raw == "":
        return J.to_map(JsonNull())
    else:
        match J.decode(raw):
            case Success(value=value):
                return J.to_map(value) if len(J.members_of(value)) > 0 or J.encode(value) == "{}" else {"INVALID_JSON": JsonString(raw)}
            case Error():
                return {"INVALID_JSON": JsonString(raw)}


def args_as_json_str(part) -> str:
    """`ToolCallPart.args_as_json_str()`: the arguments as JSON object text, `{}` when empty."""
    raw = args_text(part)
    return J.encode(J.of_map(args_as_dict(part))) if raw == "" or not is_object_text(raw) else raw


def is_object_text(raw):
    match J.decode(raw):
        case Success(value=value):
            return len(J.members_of(value)) > 0 or J.encode(value) == "{}"
        case Error():
            return False


def retry_model_response(part) -> str:
    """`RetryPromptPart.model_response()`: the text the model is shown."""
    match part:
        case RetryPromptPart(content=content, tool_name=tool_name):
            description = f"Validation feedback:\n{content}" if tool_name == "" else content
            return f"{description}\n\nFix the errors and try again."
        case _:
            return ""


# Wire form: ModelMessagesTypeAdapter.dump_python(messages, mode="json") / validate_python


def content_part(content, kind) -> Json:
    """A part's JSON form when its content and its part_kind are all it has."""
    return J.obj([J.member("content", JsonString(content)), J.member("part_kind", JsonString(kind))])


def dump_request_part(part):
    match part:
        case SystemPromptPart(content=content):
            return content_part(content, "system-prompt")
        case UserPromptPart(content=content):
            return content_part(content, "user-prompt")
        case ToolReturnPart(tool_name=tool_name, content=content, tool_call_id=tool_call_id):
            return J.obj([J.member("tool_name", JsonString(tool_name)), J.member("content", JsonString(content)), J.member("tool_call_id", JsonString(tool_call_id)), J.member("part_kind", JsonString("tool-return"))])
        case RetryPromptPart(content=content, tool_name=tool_name, tool_call_id=tool_call_id):
            return J.obj([J.member("content", JsonString(content)), J.member("tool_name", JsonNull() if tool_name == "" else JsonString(tool_name)), J.member("tool_call_id", JsonString(tool_call_id)), J.member("part_kind", JsonString("retry-prompt"))])
        case InstructionPart(content=content):
            return content_part(content, "instruction")


def dump_response_part(part):
    match part:
        case TextPart(content=content):
            return content_part(content, "text")
        case ThinkingPart(content=content):
            return content_part(content, "thinking")
        case ToolCallPart(tool_name=tool_name, args=args, tool_call_id=tool_call_id):
            return J.obj([J.member("tool_name", JsonString(tool_name)), J.member("args", JsonString(args)), J.member("tool_call_id", JsonString(tool_call_id)), J.member("part_kind", JsonString("tool-call"))])


def dump_usage(usage):
    return J.obj([
        J.member("input_tokens", JsonInt(usage.input_tokens)),
        J.member("cache_write_tokens", JsonInt(usage.cache_write_tokens)),
        J.member("cache_read_tokens", JsonInt(usage.cache_read_tokens)),
        J.member("output_tokens", JsonInt(usage.output_tokens)),
        J.member("input_audio_tokens", JsonInt(usage.input_audio_tokens)),
        J.member("cache_audio_read_tokens", JsonInt(usage.cache_audio_read_tokens)),
        J.member("output_audio_tokens", JsonInt(usage.output_audio_tokens)),
        J.member("details", JsonObject(mapList(mapKeys(usage.details), lambda key: JsonMember(key, JsonInt(unwrap_or(mapGet(usage.details, key), 0)))))),
    ])


def null_if_empty(value):
    return JsonNull() if value == "" else JsonString(value)


def dump_request(request) -> Json:
    """A `ModelRequest` in the `ModelMessagesTypeAdapter` JSON form."""
    return J.obj([J.member("parts", JsonArray(mapList(request.parts, dump_request_part))), J.member("instructions", null_if_empty(request.instructions)), J.member("kind", JsonString("request"))])


def dump_response(response) -> Json:
    """A `ModelResponse` in the `ModelMessagesTypeAdapter` JSON form."""
    return J.obj([
        J.member("parts", JsonArray(mapList(response.parts, dump_response_part))),
        J.member("usage", dump_usage(response.usage)),
        J.member("model_name", null_if_empty(response.model_name)),
        J.member("kind", JsonString("response")),
        J.member("provider_name", null_if_empty(response.provider_name)),
        J.member("provider_response_id", null_if_empty(response.provider_response_id)),
        J.member("finish_reason", null_if_empty(response.finish_reason)),
    ])


def dump_message(message) -> Json:
    """One message in the `ModelMessagesTypeAdapter` JSON form."""
    match message:
        case Request(request=request):
            return dump_request(request)
        case Response(response=response):
            return dump_response(response)


def dump_messages(messages) -> Json:
    """`ModelMessagesTypeAdapter.dump_python(messages, mode="json")` as a JSON array."""
    return JsonArray(mapList(messages, dump_message))


def validate_request_part(value) -> Result[ModelRequestPart, str]:
    kind = J.text_at(value, "part_kind", "")
    content = J.text_at(value, "content", "")
    if kind == "system-prompt":
        return Success(value=SystemPromptPart(content=content))
    elif kind == "user-prompt":
        return Success(value=UserPromptPart(content=content))
    elif kind == "tool-return":
        return Success(value=ToolReturnPart(tool_name=J.text_at(value, "tool_name", ""), content=content_text(J.object_at(value, "content")), tool_call_id=J.text_at(value, "tool_call_id", "")))
    elif kind == "retry-prompt":
        return Success(value=RetryPromptPart(content=content, tool_name=J.text_at(value, "tool_name", ""), tool_call_id=J.text_at(value, "tool_call_id", "")))
    elif kind == "instruction":
        return Success(value=InstructionPart(content=content))
    else:
        return Error(message=f"unknown request part_kind: {kind}")


def content_text(value):
    """A tool return's content: text verbatim, any other JSON as its text."""
    match value:
        case JsonString(s):
            return s
        case JsonNull():
            return ""
        case _:
            return J.encode(value)


def validate_response_part(value) -> Result[ModelResponsePart, str]:
    kind = J.text_at(value, "part_kind", "")
    content = J.text_at(value, "content", "")
    if kind == "text":
        return Success(value=TextPart(content=content))
    elif kind == "thinking":
        return Success(value=ThinkingPart(content=content))
    elif kind == "tool-call":
        return Success(value=ToolCallPart(tool_name=J.text_at(value, "tool_name", ""), args=content_text(J.object_at(value, "args")), tool_call_id=J.text_at(value, "tool_call_id", "")))
    else:
        return Error(message=f"unknown response part_kind: {kind}")


def validate_usage(value) -> Usage.RequestUsage:
    details = J.to_map(J.object_at(value, "details"))
    return Usage.RequestUsage(
        input_tokens=J.int_at(value, "input_tokens", 0),
        cache_write_tokens=J.int_at(value, "cache_write_tokens", 0),
        cache_read_tokens=J.int_at(value, "cache_read_tokens", 0),
        output_tokens=J.int_at(value, "output_tokens", 0),
        input_audio_tokens=J.int_at(value, "input_audio_tokens", 0),
        cache_audio_read_tokens=J.int_at(value, "cache_audio_read_tokens", 0),
        output_audio_tokens=J.int_at(value, "output_audio_tokens", 0),
        details=fold(range(len(mapKeys(details))), {}, lambda acc, i: mapSet(acc, unwrap_or(listGet(mapKeys(details), i), ""), unwrap_or(jsonInt(unwrap_or(mapGet(details, unwrap_or(listGet(mapKeys(details), i), "")), JsonNull())), 0))),
    )


def collect(items, validate):
    """Every validated item in order, or the first error."""
    return foldList(items, Success(value=[]), lambda acc, item: collect_step(acc, validate(item)))


def collect_step(acc, r):
    match acc:
        case Success(value=items):
            match r:
                case Success(value=item):
                    return Success(value=listAppend(items, item))
                case Error(message=message):
                    return Error(message=message)
        case Error(message=message):
            return Error(message=message)


def validate_message(data) -> Result[ModelMessage, str]:
    """One message from its `ModelMessagesTypeAdapter` JSON form."""
    kind = J.text_at(data, "kind", "")
    if kind == "request":
        match collect(J.list_at(data, "parts"), validate_request_part):
            case Success(value=parts):
                return Success(value=Request(request=ModelRequest(parts=parts, instructions=J.text_at(data, "instructions", ""))))
            case Error(message=message):
                return Error(message=message)
    elif kind == "response":
        match collect(J.list_at(data, "parts"), validate_response_part):
            case Success(value=parts):
                return Success(value=Response(response=ModelResponse(parts=parts, usage=validate_usage(J.object_at(data, "usage")), model_name=J.text_at(data, "model_name", ""), provider_name=J.text_at(data, "provider_name", ""), provider_response_id=J.text_at(data, "provider_response_id", ""), finish_reason=J.text_at(data, "finish_reason", ""))))
            case Error(message=message):
                return Error(message=message)
    else:
        return Error(message=f"unknown message kind: {kind}")


def validate_messages(value) -> Result[list[ModelMessage], str]:
    """`ModelMessagesTypeAdapter.validate_python(payload)` from a JSON array."""
    match value:
        case JsonArray(items):
            return collect(items, validate_message)
        case _:
            return Error(message="messages must be a JSON array")
