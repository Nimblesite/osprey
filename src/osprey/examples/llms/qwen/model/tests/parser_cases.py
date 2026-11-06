# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=ParserCases
"""The tool-call parser and schema layer against the oracle: validation, coercion,
whole and chunked parses, repair, truncation and the corner cases ([LLM-TOOL-PARSE],
[LLM-TOOL-REPAIR])."""
from testing import Check
from qwen_tools_expected import ExpectedTools
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from qwen_tests.ModelCases import flatten, refusal
from qwen_tests.ToolSupport import call_fingerprint_of, decoded, encoded, error_field, first_call, flag_of, indices_of, parsed_turn, record_at, records_of, scanned, text_of, tool_list, turn_fingerprint, verdict_text, violation, want_fingerprint, want_text

__all__ = ["chunked_cases", "coercion_cases", "corner_cases", "parse_cases", "repair_cases", "validation_cases"]


# Schema validation against jsonschema's own verdicts.
def validation_cases():
    """Every mutation of a real argument, judged by the real `jsonschema`."""
    tools = tool_list()
    records = records_of(ExpectedTools.VALIDATE)
    return listConcat([Check.equal("validate_count", len(records), 280)], mapList(indices_of(records), lambda i: validation_case(tools, record_at(records, i), i)))


def validation_case(tools, record, i):
    schema = ToolSchema.parameters(ToolSchema.tool_named(tools, text_of(record, "tool")))
    return Check.equal(f"validate[{i}]", verdict_text(ToolSchema.validate(schema, decoded(text_of(record, "value")))), want_text(record))


# Parameter coercion against the template's own parameter texts.
def coercion_cases():
    """Every parameter text the template wrote, plus the boundary texts a model emits."""
    records = records_of(ExpectedTools.COERCE)
    return listConcat([Check.equal("coerce_count", len(records), 130)], mapList(indices_of(records), lambda i: coercion_case(record_at(records, i), i)))


def coercion_case(record, i):
    return Check.equal(f"coerce[{i}]", coercion_text(record), coercion_want(record))


def coercion_text(record):
    match ToolSchema.coerce(decoded(text_of(record, "schema")), text_of(record, "text")):
        case Success(value=value):
            return encoded(value)
        case Error(message=message):
            return violation(message)


def coercion_want(record):
    return text_of(record, "want") if flag_of(record, "ok") else want_text(record)


def parse_cases():
    """Every Phase 0 turn, parsed whole."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    oracle = mapList(indices_of(records), lambda i: parse_case(tools, record_at(records, i), i))
    return listConcat([Check.equal("case_count", len(records), 200)], listConcat(oracle, parser_boundary_cases(tools)))


def parser_boundary_cases(tools):
    name = join(toList(map(range(201), lambda _i: "x")), "")
    call = "<tool_call><function=write_file>"
    return [
        refusal("parse_wrong_function_tag", ToolParser.parse(tools, "<tool_call><fn=write_file>"), "format@call: expected <function="),
        refusal("parse_long_function_name", ToolParser.parse(tools, "<tool_call><function=" + name), "format@call: the function name is unterminated"),
        refusal("parse_wrong_parameter_tag", ToolParser.parse(tools, call + "<param=path>"), "format@call: expected <parameter= or </function>"),
        refusal("parse_long_parameter_name", ToolParser.parse(tools, call + "<parameter=" + name), "format@call: a parameter name is unterminated"),
        refusal("parse_wrong_close", ToolParser.parse(tools, call + "</function></wrong>"), "format@call: expected </tool_call>"),
        refusal("parse_bad_error_document", ToolParser.error_field("not json", "keyword"), "tool_error@: not a tool error"),
        Check.equal("parse_missing_error_field", unwrap_or(ToolParser.error_field("{}", "keyword"), "unexpected error"), ""),
        duplicate_parameter_case(tools, call),
    ]


def duplicate_parameter_case(tools, prefix):
    step = parsed_turn(tools, prefix + "<parameter=path>a</parameter><parameter=path>b</parameter></function></tool_call>")
    call = first_call(step)
    return Check.equal("parse_duplicate_parameter", encoded(call.arguments) + "|" + error_field(call, "keyword") + "|" + error_field(call, "parameter"), "null|duplicate|path")


def parse_case(tools, record, i):
    return Check.equal(f"parse[{i}]", turn_fingerprint(parsed_turn(tools, text_of(record, "wire"))), want_fingerprint(record))


def chunked_cases():
    """The same turns with chunks split at 1, 2, 5 and 11 bytes: any split, same turn."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList([1, 2, 5, 11], lambda size: mapList(indices_of(records), lambda i: chunked_case(tools, record_at(records, i), i, size))))


def chunked_case(tools, record, i, size):
    fed = chunked_parse(tools, ToolParser.start(tools), text_of(record, "wire"), size, "", "", [])
    return Check.equal(f"chunked[{size}][{i}]", turn_fingerprint(fed), want_fingerprint(record))


def chunked_parse(tools, state, rest, size, text, think, calls):
    """Feed `rest` in `size`-byte chunks, accumulating the turn's deltas and calls."""
    if len(rest) == 0:
        return flushed(ToolParser.Step(state=state, text=text, think=think, calls=calls), ToolParser.finish(state))
    else:
        return carried(tools, rest, size, fed_once(state, take(rest, size), text, think, calls))


def fed_once(state, chunk, text, think, calls):
    match ToolParser.feed(state, chunk):
        case Success(value=step):
            return ToolParser.Step(state=step.state, text=text + step.text, think=think + step.think, calls=listConcat(calls, step.calls))
        case Error(message=message):
            return ToolParser.Step(state=state, text=f"error {message}", think=think, calls=calls)


def carried(tools, rest, size, so_far):
    return chunked_parse(tools, so_far.state, drop(rest, size), size, so_far.text, so_far.think, so_far.calls)


def flushed(so_far, finishing):
    match finishing:
        case Success(value=last):
            return ToolParser.Step(state=last.state, text=so_far.text + last.text, think=so_far.think + last.think, calls=listConcat(so_far.calls, last.calls))
        case Error(message=message):
            return ToolParser.Step(state=so_far.state, text=f"error {message}", think=so_far.think, calls=so_far.calls)


# A schema-invalid call comes back as a tool error the harness can recover from.
def repair_cases():
    """Calls the template would never write, each rejected by `jsonschema`."""
    tools = tool_list()
    records = records_of(ExpectedTools.REPAIR)
    return listConcat(flatten(mapList(indices_of(records), lambda i: repair_case(tools, record_at(records, i), i))), truncated_cases(tools, records))


def repair_case(tools, record, i):
    step = parsed_turn(tools, text_of(record, "wire"))
    call = first_call(step)
    want_keyword = text_of(record, "keyword")
    kind = "unknown_function" if want_keyword == "unknown_function" else "schema"
    return [
        Check.equal(f"repair_calls[{i}]", toString(len(step.calls)), "1"),
        Check.equal(f"repair_kind[{i}]", error_field(call, "kind"), kind),
        Check.equal(f"repair_keyword[{i}]", error_field(call, "keyword"), "" if kind == "unknown_function" else want_keyword),
        Check.equal(f"repair_arguments[{i}]", encoded(call.arguments), "null"),
        Check.equal(f"repair_recoverable[{i}]", error_field(call, "recoverable"), "true"),
    ]


def truncated_cases(tools, records):
    """A turn that stops inside `<tool_call>` is an error, never a partial call."""
    wires = mapList(records, lambda record: text_of(record, "wire"))
    return mapList(indices_of(wires), lambda i: truncated_case(tools, unwrap_or(listGet(wires, i), ""), i))


def truncated_case(tools, wire, i):
    return Check.equal(f"truncated[{i}]", truncated_text(tools, take(wire, len(wire) - 5)), "truncated/call")


def truncated_text(tools, wire):
    match ToolParser.parse(tools, wire):
        case Success(value=step):
            return f"parsed {toString(len(step.calls))}"
        case Error(message=message):
            return violation(message)


# The corners the corpus leaves out: numbers with exponents, string escapes, an
# unconstrained object, and a turn that carries two calls.
def corner_cases():
    """Scanner and parser corners: what the 200 turns happen not to exercise."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    number = decoded("{\"type\":\"number\"}")
    text_schema = decoded("{\"type\":\"string\"}")
    free = decoded("{\"type\":\"object\",\"properties\":{\"x\":{\"type\":\"integer\"}}}")
    return [
        Check.equal("corner_number", scanned(number, "-1.5e3"), "complete"),
        Check.equal("corner_number_bad", scanned(number, "1.5e"), "incomplete"),
        Check.equal("corner_escapes", scanned(text_schema, "\"a\\u00e9\\n\""), "complete"),
        Check.equal("corner_extra", scanned(free, "{\"x\": 1, \"y\": 2}"), "complete"),
        Check.equal("corner_two_calls", two_call_fingerprint(tools, records), two_call_want(records)),
    ]


def two_call_fingerprint(tools, records):
    """One turn with the call blocks of the first two cases: two calls, in order."""
    first = text_of(record_at(records, 0), "wire")
    second = text_of(record_at(records, 1), "wire")
    wire = keep_from(first, "<tool_call>") + keep_from(second, "<tool_call>")
    return turn_fingerprint(parsed_turn(tools, "<think>\n</think>\n\n" + wire))


def keep_from(wire, marker):
    """`wire` from `marker` on: the call block without the think-and-text prefix."""
    return drop(wire, unwrap_or(indexOf(wire, marker), 0))


def two_call_want(records):
    return "\n\n|\n|" + call_fingerprint_of(record_at(records, 0)) + ";" + call_fingerprint_of(record_at(records, 1))
