# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=ToolSupport
"""Readers over the tool-call oracle records (oracle/tool_calls.cpython) and the
fingerprints the parser cases compare: shared by every tool-call module."""
from qwen_tools_expected import ExpectedTools
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema

__all__ = ["call_fingerprint_of", "decoded", "encoded", "error_field", "finished", "first_call", "flag_of", "indices_of", "ints_of", "parsed_turn", "record_at", "records_of", "scanned", "text_of", "texts_of", "tool_list", "turn_fingerprint", "verdict_text", "violation", "want_fingerprint", "want_text"]


# --- tool calls: [LLM-TOOL-PARSE], [LLM-TOOL-CONSTRAIN], [LLM-TOOL-REPAIR] ---
def decoded(text):
    """A JSON text as a value, `JsonNull` when it does not parse."""
    return unwrap_or(jsonDecode(text), JsonNull)


def text_of(document, name):
    """The string member `name` of a JSON object, or ""."""
    return unwrap_or(jsonText(unwrap_or(jsonField(document, name), JsonNull)), "")


def flag_of(document, name):
    """The boolean member `name` of a JSON object, or `False`."""
    return unwrap_or(jsonBool(unwrap_or(jsonField(document, name), JsonNull)), False)


def array_of(value):
    match value:
        case JsonArray(items):
            return items
        case _:
            return []


def texts_of(value):
    """The strings of a JSON array."""
    return mapList(array_of(value), lambda item: unwrap_or(jsonText(item), ""))


def ints_of(value):
    """The integers of a JSON array."""
    return mapList(array_of(value), lambda item: unwrap_or(jsonInt(item), 0))


def tool_list():
    """The advertised tools of the oracle corpus."""
    return decoded(ExpectedTools.TOOLS)


def records_of(constant):
    """An oracle constant's JSON texts as values."""
    return mapList(constant, decoded)


def indices_of(values):
    """`0 .. len(values) - 1`."""
    return toList(range(len(values)))


def record_at(records, i):
    """Record `i`, or `JsonNull` past the end."""
    return unwrap_or(listGet(records, i), JsonNull)


def violation(message):
    """A schema violation message as `keyword/path`."""
    return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


def verdict_text(verdict):
    """A schema verdict as `keyword/path`, or "" when it held."""
    match verdict:
        case Success(_accepted):
            return ""
        case Error(message=message):
            return violation(message)


def want_text(record):
    """A fixture record's expected verdict: "" when valid, else `keyword/path`."""
    return "" if flag_of(record, "ok") else f"{text_of(record, 'keyword')}/{text_of(record, 'path')}"


def encoded(value):
    """A JSON value as text, `?` when it cannot be encoded."""
    return unwrap_or(jsonEncode(value), "?")


# Parsing the template's own turns, whole and in chunks.
def call_fingerprint(call):
    return f"{call.name}|{encoded(call.arguments)}|{call.error}"


def turn_fingerprint(step):
    """One string standing for a whole parsed turn: its deltas and its calls."""
    return f"{step.text}|{step.think}|" + join(mapList(step.calls, call_fingerprint), ";")


def want_fingerprint(record):
    """The oracle's text, thinking, call name and arguments of one turn, joined by `|`."""
    return f"{text_of(record, 'text')}|{text_of(record, 'think')}|{text_of(record, 'name')}|{text_of(record, 'args')}|"


def parsed_turn(tools, wire):
    """The parser's closed read of `wire`, or a step carrying the parse error as its text."""
    match ToolParser.parse(tools, wire):
        case Success(value=step):
            return step
        case Error(message=message):
            return ToolParser.Step(state=ToolParser.start(tools), text=message, think="", calls=[])


def first_call(step):
    """The step's first call, or an empty one."""
    return unwrap_or(listGet(step.calls, 0), ToolParser.Call(name="", arguments=JsonNull, error=""))


def error_field(call, name):
    """One field of a call's tool error, or "" when the call carries none."""
    match ToolParser.error_field(call.error, name):
        case Success(value=field):
            return field
        case Error(_rejected):
            return ""


def scanned(schema, text):
    """Feed `text` to the scanner and say whether it completed, stalled or was rejected."""
    return scanning(ToolJsonScan.begin(schema), text, 0)


def scanning(stack, text, i):
    """One byte of `text` through the scanner's `Frame` stack, from the start of the text."""
    if i >= len(text):
        return "complete" if finished(stack) else "incomplete"
    else:
        return scan_fed(text, i, ToolJsonScan.feed(stack, unwrap_or(byteAt(text, i), -1)))


def finished(stack):
    """Whether the scanner accepts the stack as a complete document."""
    match ToolJsonScan.finished(stack):
        case Success(_accepted):
            return True
        case Error(_rejected):
            return False


def scan_fed(text, i, outcome):
    """Apply the `ScanStep` one `feed` returned, which says whether the byte was taken: a
    stage that finished without taking it hands the same byte to the next stage."""
    match outcome:
        case Success(value=stepped):
            return handed(stepped, text, i)
        case Error(_rejected):
            return "rejected"


def handed(stepped, text, i):
    if len(stepped.stack) == 0:
        return "complete" if i + 1 >= len(text) else "trailing"
    elif stepped.consumed:
        return scanning(stepped.stack, text, i + 1)
    else:
        return scanning(stepped.stack, text, i)


def call_fingerprint_of(record):
    """The oracle's call name and arguments of one turn, joined by `|`."""
    return f"{text_of(record, 'name')}|{text_of(record, 'args')}|"
