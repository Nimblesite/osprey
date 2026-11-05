# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=ToolParser
"""A streaming parser for the Qwen3.5 tool-call format, coercing by JSON Schema.

The Qwen chat template (Qwen/Qwen3.8-27B, revision
1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0; the pinned weights file's
`tokenizer.chat_template` writes the same bytes) generates a turn as

    <think>
    reasoning
    </think>

    free text before the call
    <tool_call>
    <function=NAME>
    <parameter=PARAM>
    value
    </parameter>
    </function>
    </tool_call>

and repeats the `<tool_call>` block for each call. A string argument is written
verbatim, so it may span lines; every other argument is the JSON literal the
template's `tojson` produced. vLLM's `Qwen3CoderToolParser` reads the same
format (`<function=`, `<parameter=`, one newline stripped at each end of a
value) and is the reference for the framing rules here.

`feed` is incremental: a chunk may split anywhere, including inside a tag, and
any split produces the same steps as one whole-text call. `finish` reports a
turn that ended inside a `<think>` or `<tool_call>` as an error instead of
truncating it silently.

Implements [LLM-TOOL-PARSE] and [LLM-TOOL-REPAIR] of
docs/specs/applications/LocalCodingModel.md: a call whose arguments fail the
tool's schema closes as a `Call` carrying a tool error and `JsonNull`
arguments, so the harness hands the model a `<tool_response>` it can recover
from and never sees invented arguments.
"""

from transformers import ToolSchema

__all__ = ["Call", "Step", "start", "feed", "finish", "parse", "error_field"]


THINK_OPEN = "<think>"
THINK_CLOSE = "</think>"
CALL_OPEN = "<tool_call>"
CALL_CLOSE = "</tool_call>"
FUNCTION_OPEN = "<function="
FUNCTION_CLOSE = "</function>"
PARAMETER_OPEN = "<parameter="
PARAMETER_CLOSE = "</parameter>"
MAX_NAME = 200


class State:
    """The parser's whole state: where it is in the turn and the call in progress."""
    tools: Json
    mode: str
    stage: str
    buffer: str
    name: str
    param: str
    raws: list[Raw]


class Raw:
    """One `<parameter=NAME>` value as the template wrote it, before coercion."""
    name: str
    text: str


class Call:
    """One parsed call: its name, its coerced arguments, and its tool error.

    A valid call carries `arguments` as a JSON object and an empty `error`. A call
    whose arguments do not satisfy the tool's schema carries `JsonNull` arguments
    and a JSON tool error in `error` ([LLM-TOOL-REPAIR])."""
    name: str
    arguments: Json
    error: str


class Step:
    """What one `feed` produced: the new state and the chunk's text, think and calls."""
    state: State
    text: str
    think: str
    calls: list[Call]


# --- entry points ---------------------------------------------------------

def start(tools) -> State:
    """A parser for one assistant turn against the request's tool list."""
    return State(tools=tools, mode="text", stage="open", buffer="", name="", param="", raws=[])


def parse(tools, text):
    """Parse a whole turn at once; a turn that ends inside a tag is an error."""
    match feed(start(tools), text):
        case Success(value=step):
            return merged(step, finish(step.state))
        case Error(message=message):
            return Error(message=message)


def merged(step, finishing):
    """The turn's whole result: what `feed` emitted plus what `finish` flushed."""
    match finishing:
        case Success(value=last):
            return Success(value=Step(state=last.state, text=step.text + last.text, think=step.think + last.think, calls=listConcat(step.calls, last.calls)))
        case Error(message=message):
            return Error(message=message)


def feed(state, chunk):
    """Add `chunk` to the turn and consume as much of it as is complete."""
    return consume(update(state, buffer=state.buffer + chunk), "", "", [])


def finish(state):
    """End of the turn: flush pending text, and reject a turn that stopped inside a tag."""
    if state.mode == "text":
        return Success(value=Step(state=update(state, buffer=""), text=state.buffer, think="", calls=[]))
    else:
        return Error(message=ToolSchema.violation("truncated", state.mode, f"the turn ended inside {state.mode}"))


# --- the turn scanner -----------------------------------------------------

def consume(state, text, think, calls):
    """Consume the buffered text in the current mode until it needs more input."""
    match state.mode:
        case "think":
            return think_step(state, text, think, calls)
        case "call":
            return consume_call(state, text, think, calls)
        case _:
            return text_step(state, text, think, calls)


def text_step(state, text, think, calls):
    """Free text: emit it up to the next opening tag, holding back a partial tag."""
    at = earliest(state.buffer, [THINK_OPEN, CALL_OPEN])
    if at < 0:
        keep = held(state.buffer, [THINK_OPEN, CALL_OPEN])
        body = take(state.buffer, len(state.buffer) - len(keep))
        return Success(value=Step(state=update(state, buffer=keep), text=text + body, think=think, calls=calls))
    else:
        tag = THINK_OPEN if startsWith(drop(state.buffer, at), THINK_OPEN) else CALL_OPEN
        entered = update(state, mode="think" if tag == THINK_OPEN else "call", stage="open", buffer=drop(state.buffer, at + len(tag)))
        return consume(entered, text + take(state.buffer, at), think, calls)


def think_step(state, text, think, calls):
    """Inside `<think>`: emit reasoning up to `</think>`."""
    at = unwrap_or(indexOf(state.buffer, THINK_CLOSE), -1)
    if at < 0:
        keep = held(state.buffer, [THINK_CLOSE])
        body = take(state.buffer, len(state.buffer) - len(keep))
        return Success(value=Step(state=update(state, buffer=keep), text=text, think=think + body, calls=calls))
    else:
        return consume(update(state, mode="text", stage="open", buffer=drop(state.buffer, at + len(THINK_CLOSE))), text, think + take(state.buffer, at), calls)


def earliest(buffer, tags):
    """The index of the earliest of `tags` in `buffer`, or -1."""
    found = filterList(mapList(tags, lambda tag: unwrap_or(indexOf(buffer, tag), -1)), lambda at: at >= 0)
    return foldList(found, -1, lambda best, at: at if best < 0 or at < best else best)


def emitted(buffer, tags):
    """Everything in `buffer` except a trailing partial tag."""
    return take(buffer, len(buffer) - len(held(buffer, tags)))


def held(buffer, tags):
    """The longest suffix of `buffer` that is still a proper prefix of one of `tags`."""
    return fold(
        range(shortest(len(buffer), len(widest(tags)) - 1)),
        "",
        lambda best, k: longer(best, tag_prefix(buffer, tags, k + 1)),
    )


def widest(tags):
    return foldList(tags, "", lambda best, tag: tag if len(tag) > len(best) else best)


def tag_prefix(buffer, tags, size):
    """The last `size` bytes of `buffer` when they are still a proper tag prefix."""
    tail = drop(buffer, len(buffer) - size)
    return tail if foldList(tags, False, lambda found, tag: found or is_prefix(tail, tag)) else ""


def shortest(left, right):
    """The smaller of two counts."""
    return left if left < right else right


def longer(best, candidate):
    return candidate if len(candidate) > len(best) else best


def is_prefix(text, literal):
    """True when `text` is shorter than `literal` and a prefix of it."""
    return len(text) < len(literal) and startsWith(literal, text)


# --- the call scanner -----------------------------------------------------

def consume_call(state, text, think, calls):
    """Inside `<tool_call>`: run the current stage, waiting when the buffer is short."""
    match state.stage:
        case "open":
            return open_function(state, text, think, calls)
        case "name":
            return read_name(state, text, think, calls)
        case "params":
            return read_params(state, text, think, calls)
        case "pname":
            return read_parameter_name(state, text, think, calls)
        case "valuestart":
            return start_value(state, text, think, calls)
        case "value":
            return read_value(state, text, think, calls)
        case _:
            return close_call(state, text, think, calls)


def waiting(state, text, think, calls):
    """Nothing more can be read from the buffer yet."""
    return Success(value=Step(state=state, text=text, think=think, calls=calls))


def open_function(state, text, think, calls):
    """Skip the whitespace after `<tool_call>` and match `<function=`."""
    body = skip_space(state.buffer)
    if startsWith(body, FUNCTION_OPEN):
        return consume(update(state, stage="name", buffer=drop(body, len(FUNCTION_OPEN))), text, think, calls)
    elif is_prefix(body, FUNCTION_OPEN):
        return waiting(update(state, buffer=body), text, think, calls)
    else:
        return Error(message=ToolSchema.violation("format", "call", f"expected {FUNCTION_OPEN}"))


def read_name(state, text, think, calls):
    """Read the function name up to its `>`."""
    body = skip_space(state.buffer)
    at = unwrap_or(indexOf(body, ">"), -1)
    if at >= 0:
        named = update(state, name=take(body, at), stage="params", buffer=drop(body, at + 1))
        return consume(named, text, think, calls)
    elif len(body) > MAX_NAME:
        return Error(message=ToolSchema.violation("format", "call", "the function name is unterminated"))
    else:
        return waiting(update(state, buffer=body), text, think, calls)


def read_params(state, text, think, calls):
    """After a `>`: the next `<parameter=` or the closing `</function>`."""
    body = skip_space(state.buffer)
    if startsWith(body, PARAMETER_OPEN):
        return consume(update(state, stage="pname", buffer=drop(body, len(PARAMETER_OPEN))), text, think, calls)
    elif startsWith(body, FUNCTION_CLOSE):
        return consume(update(state, stage="end", buffer=drop(body, len(FUNCTION_CLOSE))), text, think, calls)
    elif is_prefix(body, PARAMETER_OPEN) or is_prefix(body, FUNCTION_CLOSE):
        return waiting(update(state, buffer=body), text, think, calls)
    else:
        return Error(message=ToolSchema.violation("format", "call", f"expected {PARAMETER_OPEN} or {FUNCTION_CLOSE}"))


def read_parameter_name(state, text, think, calls):
    """Read a parameter name up to its `>`, then drop the template's newline."""
    body = skip_space(state.buffer)
    at = unwrap_or(indexOf(body, ">"), -1)
    if at >= 0:
        armed = update(state, param=take(body, at), stage="valuestart", buffer=drop(body, at + 1))
        return consume(armed, text, think, calls)
    elif len(body) > MAX_NAME:
        return Error(message=ToolSchema.violation("format", "call", "a parameter name is unterminated"))
    else:
        return waiting(update(state, buffer=body), text, think, calls)


def start_value(state, text, think, calls):
    """Drop the single newline the template writes after `<parameter=X>`, then read the
    value. It arrives in its own chunk when a chunk boundary falls on it, so the step
    has to be part of the state and not of one chunk's bytes."""
    if startsWith(state.buffer, "\n"):
        return consume(update(state, stage="value", buffer=drop(state.buffer, 1)), text, think, calls)
    elif len(state.buffer) == 0:
        return waiting(state, text, think, calls)
    else:
        return consume(update(state, stage="value"), text, think, calls)


def read_value(state, text, think, calls):
    """Read a parameter's value up to `</parameter>`, dropping the template's newline."""
    at = unwrap_or(indexOf(state.buffer, PARAMETER_CLOSE), -1)
    if at < 0:
        return waiting(state, text, think, calls)
    else:
        raw = Raw(name=state.param, text=without_trailing_newline(take(state.buffer, at)))
        recorded = update(state, stage="params", param="", buffer=drop(state.buffer, at + len(PARAMETER_CLOSE)), raws=listAppend(state.raws, raw))
        return consume(recorded, text, think, calls)


def close_call(state, text, think, calls):
    """Match `</tool_call>` and emit the call bound to its tool."""
    body = skip_space(state.buffer)
    if startsWith(body, CALL_CLOSE):
        call = bind(state, state.name, state.raws)
        return consume(update(state, mode="text", stage="open", name="", param="", raws=[], buffer=drop(body, len(CALL_CLOSE))), text, think, listAppend(calls, call))
    elif is_prefix(body, CALL_CLOSE):
        return waiting(update(state, buffer=body), text, think, calls)
    else:
        return Error(message=ToolSchema.violation("format", "call", f"expected {CALL_CLOSE}"))


def skip_space(text):
    """`text` without its leading spaces, newlines and tabs."""
    return drop(text, spaces(text))


def spaces(text):
    """How many leading bytes of `text` are whitespace."""
    return fold(range(shortest(len(text), MAX_NAME)), 0, lambda count, i: count + 1 if count == i and is_space_byte(text, i) else count)


def is_space_byte(text, i):
    """Whether byte `i` is a space, newline, carriage return or tab."""
    byte = unwrap_or(byteAt(text, i), -1)
    return byte == of_char(" ") or byte == of_char("\n") or byte == of_char("\r") or byte == of_char("\t")


def of_char(text):
    """The UTF-8 byte of an ASCII one-character literal."""
    return unwrap_or(byteAt(text, 0), -1)


def without_trailing_newline(text):
    """`text` without the single newline the template writes before `</parameter>`."""
    return take(text, len(text) - 1) if endsWith(text, "\n") else text


# --- binding a call to its tool -------------------------------------------

def absent(value):
    """Whether a tool or property lookup found nothing (`JsonNull`)."""
    return ToolSchema.kind_matches("null", value)


def bind(state, name, raws):
    """A call's raw parameters coerced and checked against its tool's schema."""
    tool = ToolSchema.tool_named(state.tools, name)
    if absent(tool):
        return Call(name=name, arguments=JsonNull, error=error_text("unknown_function", "", "", f"no tool named {name}"))
    else:
        coerced = coerce_all(ToolSchema.parameters(tool), raws)
        if coerced.error != "":
            return Call(name=name, arguments=JsonNull, error=coerced.error)
        else:
            return checked_call(name, ToolSchema.parameters(tool), coerced.members)


class Bound:
    """The coerced members of a call so far, or the first problem found."""
    members: list[JsonMember]
    error: str


def coerce_all(schema, raws):
    """Coerce every raw parameter in document order, keeping the first problem."""
    return foldList(raws, Bound(members=[], error=""), lambda bound, raw: bound if bound.error != "" else coerce_one(schema, bound, raw))


def coerce_one(schema, bound, raw):
    """One raw parameter: an undeclared or repeated name, or its coerced value."""
    declared = ToolSchema.property_schema(schema, raw.name)
    if absent(declared):
        return Bound(members=bound.members, error=reported("additionalProperties", raw.name, ""))
    elif repeated(bound.members, raw.name):
        return Bound(members=bound.members, error=reported("duplicate", raw.name, ""))
    else:
        return coerced_member(declared, bound, raw)


def repeated(members, name):
    return len(filterList(members, lambda member: member_name(member) == name)) > 0


def member_name(member):
    match member:
        case JsonMember(name, _value):
            return name
        case _:
            return ""


def coerced_member(declared, bound, raw):
    """`raw`'s value coerced by its parameter schema, or the violation it caused."""
    match ToolSchema.coerce(declared, raw.text):
        case Success(value=value):
            return Bound(members=listAppend(bound.members, JsonMember(raw.name, value)), error="")
        case Error(message=message):
            return Bound(members=bound.members, error=reported(ToolSchema.violation_keyword(message), raw.name, message))


def checked_call(name, schema, members):
    """The whole argument object validated against the tool's schema."""
    arguments = JsonObject(members)
    missing = filterList(ToolSchema.required_names(schema), lambda required: len(filterList(members, lambda member: member_name(member) == required)) == 0)
    if len(missing) > 0:
        absent = unwrap_or(listGet(missing, 0), "")
        return Call(name=name, arguments=JsonNull, error=reported("required", absent, f"missing {absent}"))
    else:
        return verdict_call(name, arguments, ToolSchema.validate(schema, arguments))


def verdict_call(name, arguments, verdict):
    """The call a schema verdict produces: the arguments, or the tool error."""
    match verdict:
        case Success(_accepted):
            return Call(name=name, arguments=arguments, error="")
        case Error(message=message):
            return Call(name=name, arguments=JsonNull, error=verdict_error(message))


def verdict_error(message):
    """A validation violation as a tool error, keeping its keyword and instance path."""
    return error_text("schema", ToolSchema.violation_keyword(message), ToolSchema.violation_path(message), message)


def reported(keyword, parameter, detail):
    """The tool error for one parameter that a call got wrong."""
    return error_text("schema", keyword, parameter, detail)


def error_text(kind, keyword, parameter, detail):
    """A tool error as the JSON object the harness reads and the model receives."""
    return unwrap_or(
        jsonEncode(
            JsonObject(
                [
                    JsonMember("kind", JsonString(kind)),
                    JsonMember("keyword", JsonString(keyword)),
                    JsonMember("parameter", JsonString(parameter)),
                    JsonMember("detail", JsonString(detail)),
                    JsonMember("recoverable", JsonBool(True)),
                ]
            )
        ),
        "",
    )


def error_field(message, name):
    """One field of a tool error produced by this module, as text."""
    match jsonDecode(message):
        case Success(value=document):
            return field_text(any_field(document, name))
        case Error(_rejected):
            return Error(message=ToolSchema.violation("tool_error", "", "not a tool error"))


def any_field(document, name):
    match jsonField(document, name):
        case Success(value=field):
            return field
        case Error(_rejected):
            return JsonString("")


def field_text(field):
    """A tool-error field as text: a string verbatim, anything else as its JSON."""
    match field:
        case JsonString(text):
            return Success(value=text)
        case _:
            return jsonEncode(field)
