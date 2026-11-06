# osprey: flavor=python
"""The Qwen3.5 example model's suite: the tool-call parser, scanner and constraint
against the values the real chat template and jsonschema produced
(oracle/tool_calls.cpython), and the device model against every activation and cache
state transformers 5.20.2 + torch 2.14.1 computed for a tiny checkpoint
(oracle/qwen3_5.cpython)."""
from testing import Check
from qwen_expected import Expected
from qwen_tools_expected import ExpectedTools
from qwen import Qwen3_5Attention, Qwen3_5Cache, Qwen3_5Config, Qwen3_5DeltaNet, Qwen3_5Device, Qwen3_5Hf, Qwen3_5Mlp, Qwen3_5Model, Qwen3_5Norm, Qwen3_5Rope, Qwen3_5Shapes
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from transformers.Math import argmax


def close_list(name, got, want, rtol, atol):
    """Float32 lists compared by exact widening to float64."""
    return Check.close_all(name, mapList(got, toFloat), mapList(want, toFloat), rtol, atol)


def main():
    match Check.strictly(lambda: Check.report(listConcat(tool_cases(), model_fixture_cases()))):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")


# --- tool calls: [LLM-TOOL-PARSE], [LLM-TOOL-CONSTRAIN], [LLM-TOOL-REPAIR] ---

def decoded(text):
    return unwrap_or(jsonDecode(text), JsonNull)


def text_of(document, name):
    return unwrap_or(jsonText(unwrap_or(jsonField(document, name), JsonNull)), "")


def flag_of(document, name):
    return unwrap_or(jsonBool(unwrap_or(jsonField(document, name), JsonNull)), False)


def array_of(value):
    match value:
        case JsonArray(items):
            return items
        case _:
            return []


def texts_of(value):
    return mapList(array_of(value), lambda item: unwrap_or(jsonText(item), ""))


def ints_of(value):
    return mapList(array_of(value), lambda item: unwrap_or(jsonInt(item), 0))


def tool_list():
    return decoded(ExpectedTools.TOOLS)


def records_of(constant):
    return mapList(constant, decoded)


def indices_of(values):
    return toList(range(len(values)))


def record_at(records, i):
    return unwrap_or(listGet(records, i), JsonNull)


def verdict_text(verdict):
    """A schema verdict as `keyword/path`, or "" when it held."""
    match verdict:
        case Success(_accepted):
            return ""
        case Error(message=message):
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


def want_text(record):
    """A fixture record's expected verdict: "" when valid, else `keyword/path`."""
    return "" if flag_of(record, "ok") else f"{text_of(record, 'keyword')}/{text_of(record, 'path')}"


def encoded(value):
    return unwrap_or(jsonEncode(value), "?")


def flatten(lists):
    """One list of every outcome in a list of outcome lists."""
    return foldList(lists, [], lambda acc, outcomes: listConcat(acc, outcomes))


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
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


def coercion_want(record):
    return text_of(record, "want") if flag_of(record, "ok") else want_text(record)


# Parsing the template's own turns, whole and in chunks.

def call_fingerprint(call):
    return f"{call.name}|{encoded(call.arguments)}|{call.error}"


def turn_fingerprint(step):
    """One string standing for a whole parsed turn: its deltas and its calls."""
    return f"{step.text}|{step.think}|" + join(mapList(step.calls, call_fingerprint), ";")


def want_fingerprint(record):
    return f"{text_of(record, 'text')}|{text_of(record, 'think')}|{text_of(record, 'name')}|{text_of(record, 'args')}|"


def parsed_turn(tools, wire):
    match ToolParser.parse(tools, wire):
        case Success(value=step):
            return step
        case Error(message=message):
            return ToolParser.Step(state=ToolParser.start(tools), text=message, think="", calls=[])


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


def first_call(step):
    return unwrap_or(listGet(step.calls, 0), ToolParser.Call(name="", arguments=JsonNull, error=""))


def error_field(call, name):
    """One field of a call's tool error, or "" when the call carries none."""
    match ToolParser.error_field(call.error, name):
        case Success(value=field):
            return field
        case Error(_rejected):
            return ""


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
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


# Masked sampling: a schema-invalid call is unsamplable.

def constrain_cases():
    """Every Phase 0 case, sampled twice: towards its own tokens, and adversarially."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return listConcat(flatten(mapList(indices_of(records), lambda i: constrain_case(tools, record_at(records, i), i))), constrain_enum_cases())


def constrain_enum_cases():
    tools = decoded("[{\"type\":\"function\",\"function\":{\"name\":\"set_mode\",\"parameters\":{\"type\":\"object\",\"properties\":{\"mode\":{\"type\":\"string\",\"enum\":[\"run\",\"review\"]}},\"required\":[\"mode\"],\"additionalProperties\":false}}}]")
    prefix = "<tool_call>\n<function=set_mode>\n<parameter=mode>\n"
    suffix = "\n</parameter>\n</function>\n</tool_call>"
    return [
        Check.equal("constrain_enum_run", completed_enum_call(tools, prefix + "run" + suffix), True),
        Check.equal("constrain_enum_review", completed_enum_call(tools, prefix + "review" + suffix), True),
        Check.equal("constrain_enum_prefix", completed_enum_call(tools, prefix + "r"), False),
        refusal("constrain_enum_undeclared", ToolConstrain.advance(ToolConstrain.start(tools), tools, prefix + "reject"), "constrain@value: not a declared member"),
    ]


def completed_enum_call(tools, text):
    match ToolConstrain.advance(ToolConstrain.start(tools), tools, text):
        case Success(value=machine):
            return ToolConstrain.complete(machine)
        case Error():
            return False


def constrain_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    constraint = ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return [
        Check.equal(f"target[{i}]", walked(tools, constraint, record, started), want_fingerprint(record)),
        Check.equal(f"adversarial[{i}]", sampled(tools, constraint, record, started), ""),
    ]


def spelled(tokens, ids):
    return join(mapList(ids, lambda id: unwrap_or(listGet(tokens, id), "")), "")


def machine_after(tools, text):
    match ToolConstrain.advance(ToolConstrain.start(tools), tools, text):
        case Success(value=machine):
            return machine
        case Error(_rejected):
            return ToolConstrain.start(tools)


def walked(tools, constraint, record, text):
    """Greedy towards the fixture's own tokens: the mask has to allow every one."""
    return walk(tools, constraint, record, machine_after(tools, text), ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0, text)


def walk(tools, constraint, record, machine, want, step, text):
    if ToolConstrain.complete(machine):
        return turn_fingerprint(parsed_turn(tools, text))
    elif step >= len(want):
        return f"stalled at {machine.stage} after {toString(step)} tokens"
    else:
        return walked_step(tools, constraint, record, machine, want, step, text)


def walked_step(tools, constraint, record, machine, want, step, text):
    token = unwrap_or(listGet(constraint.vocabulary, unwrap_or(listGet(want, step), 0)), "")
    match ToolConstrain.advance(machine, tools, token):
        case Success(value=advanced):
            return walk(tools, constraint, record, advanced, want, step + 1, text + token)
        case Error(message=message):
            return f"rejected {unwrap_or(jsonQuote(token), token)} at {toString(step)}: {message}"


def sampled(tools, constraint, record, text):
    """Seeded adversarial sampling over the corpus token, a seeded draw and every closing
    token: a forbidden candidate is scored highest, so an unmasked row corrupts the
    format at once, and the mask alone keeps the call schema-valid. Returns "" when the
    turn is one schema-valid call."""
    return sample(tools, constraint, record, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)), text, 0)


def sample(tools, constraint, record, ids, text, step):
    machine = machine_after(tools, text)
    if ToolConstrain.complete(machine):
        return sampled_verdict(tools, parsed_turn(tools, text))
    elif step > 200:
        return f"never completed after {toString(step)} at {machine.stage} depth={toString(len(machine.stack))} inner={inner_stage(machine)}"
    else:
        return sampled_step(tools, constraint, record, ids, text, step, machine)


def sampled_step(tools, constraint, record, ids, text, step, machine):
    """One adversarial step: a seeded draw is sampled whenever the mask admits it, so a
    corrupting token is what the run takes by default; when the mask forbids it, the
    corpus token keeps the run on its schema-valid path."""
    drawn = seeded_id(constraint, step, 0)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), step), 0 - 1)
    chosen = choose(constraint, machine, drawn, corpus)
    if chosen < 0:
        return f"stalled at {toString(step)} in {machine.stage} inner={inner_stage(machine)} key={inner_text(machine)} seen={join(machine.seen, ',')}"
    else:
        return sample(tools, constraint, record, listAppend(ids, chosen), text + token_at(constraint, chosen), step + 1)


def choose(constraint, machine, drawn, corpus):
    """The seeded draw when the mask admits it and it moves the grammar on, else the corpus
    token when the mask admits that, else whatever moves the grammar on. A draw the mask
    rejects is where the constraint bites; an admitted draw the run keeps."""
    if progresses_with(constraint, machine, drawn):
        return drawn
    elif allowed_from(constraint, machine, corpus):
        return corpus
    else:
        return first_progressing(constraint, machine, 0, 0 - 1)


def progresses_with(constraint, machine, id):
    if id < 0:
        return False
    else:
        return moved_on(constraint, machine, token_at(constraint, id))


def moved_on(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(value=advanced):
            return progresses(machine, advanced)
        case Error(_rejected):
            return False


def seeded_id(constraint, step, k):
    return (step * 7919 + k * 104729) % len(constraint.vocabulary)


def first_progressing(constraint, machine, k, found):
    """The first allowed token that moves the grammar on: a new stage or a finished JSON
    value. It keeps a seeded source from idling inside a verbatim string argument."""
    if k >= len(constraint.vocabulary):
        return found if found >= 0 else first_allowed(constraint, machine, 0)
    else:
        return progressed(constraint, machine, k, found)


def progressed(constraint, machine, k, found):
    match ToolConstrain.advance(machine, constraint.tools, token_at(constraint, k)):
        case Success(value=advanced):
            if found < 0 and moves(machine, advanced):
                return k
            else:
                return first_progressing(constraint, machine, k + 1, found)
        case Error(_rejected):
            return first_progressing(constraint, machine, k + 1, found)


def progresses(machine, advanced):
    """Whether one token moved the grammar to a new stage, finished a JSON value or
    advanced a literal: what a draw has to do to be worth taking."""
    return advanced.stage != machine.stage or advanced.mode != machine.mode or len(advanced.stack) < len(machine.stack) or advanced.progress != machine.progress


def moves(machine, advanced):
    """Whether one token moved at all, including a step inside a JSON value, so a stuck run
    always has a deterministic token to take. Text growth alone is not a move: at a closed
    stage the first allowed token already continues the name or member."""
    return progresses(machine, advanced) or top_stage(advanced) != top_stage(machine)


def top_stage(machine):
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def inner_text(machine):
    """The key or literal text the JSON scan has accumulated: what a stall names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.text)


def inner_stage(machine):
    """The JSON scan's own stage at the top of its stack: what a failure message names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def first_allowed(constraint, machine, k):
    if k >= len(constraint.vocabulary):
        return 0 - 1
    elif allowed_from(constraint, machine, k):
        return k
    else:
        return first_allowed(constraint, machine, k + 1)


def allowed_from(constraint, machine, id):
    if id < 0:
        return False
    else:
        return accepts(constraint, machine, token_at(constraint, id))


def accepts(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(_advanced):
            return True
        case Error(_rejected):
            return False


def token_at(constraint, i):
    return unwrap_or(listGet(constraint.vocabulary, i), "")


def sampled_verdict(tools, step):
    """"" when the sampled turn is one schema-valid call, else what is wrong with it."""
    if len(step.calls) != 1:
        return f"{toString(len(step.calls))} calls ({step.text})"
    else:
        return call_verdict(tools, first_call(step))


def call_verdict(tools, call):
    if call.error != "":
        return f"tool error {call.error}"
    else:
        return verdict_text(ToolSchema.validate(ToolSchema.parameters(ToolSchema.tool_named(tools, call.name)), call.arguments))


def control_cases():
    """Clean controls: without the mask the same adversarial row corrupts the format."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return mapList([0, 1, 2, 7], lambda i: control_case(tools, record_at(records, i), i))


def control_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return Check.equal(f"control_unmasked[{i}]", unmasked(tools, constraint_of(tools, tokens), started, 0), "corrupted")


def constraint_of(tools, tokens):
    return ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))


def unmasked(tools, constraint, text, step):
    """Sample an adversarial row with no mask: the turn stops being a parseable call."""
    if step >= 12:
        return "corrupted"
    else:
        return unmasked_step(tools, constraint, text, step)


def unmasked_step(tools, constraint, text, step):
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda i: adversarial_draw(i, step))
    picked = argmax(scores)
    return unmasked(tools, constraint, text + token_at(constraint, picked), step + 1)


def adversarial_draw(i, step):
    return toFloat32((step * 7919 + i * 104729) % 1000) / toFloat32(1000.0)


# The schema readers the coercion and the mask are built on.

def reader_cases():
    """Every reader of the schema sub-language, over the request's own tool list."""
    tools = tool_list()
    policy = ToolSchema.parameters(ToolSchema.tool_named(tools, "run_tests"))
    mode = ToolSchema.property_schema(ToolSchema.property_schema(policy, "policy"), "mode")
    suites = ToolSchema.property_schema(policy, "suites")
    return [
        Check.equal("reader_kind", ToolSchema.kind(policy), "object"),
        Check.equal("reader_kind_matches", ToolSchema.kind_matches("integer", JsonInt(3)), True),
        Check.equal("reader_members", join(ToolSchema.property_names(policy), ","), "suites,policy"),
        Check.equal("reader_names", join(ToolSchema.property_names(ToolSchema.parameters(ToolSchema.tool_named(tools, "write_file"))), ","), "path,content"),
        Check.equal("reader_required", join(ToolSchema.required_names(policy), ","), "suites,policy"),
        Check.equal("reader_items", ToolSchema.kind(ToolSchema.items_schema(suites)), "string"),
        Check.equal("reader_extra", ToolSchema.allows_extra(policy), False),
        Check.equal("reader_enum", join(mapList(ToolSchema.enum_values(mode), lambda value: encoded(value)), ","), "\"check\",\"apply\",\"preview\""),
        Check.equal("reader_tool", ToolSchema.tool_name(ToolSchema.tool_named(tools, "find_symbol")), "find_symbol"),
        Check.equal("reader_tools", toString(len(ToolSchema.tool_list(tools))), "5"),
        Check.equal("reader_violation", ToolSchema.violation_keyword(ToolSchema.violation("enum", "mode", "not a member")), "enum"),
        Check.equal("reader_violation_path", ToolSchema.violation_path(ToolSchema.violation("enum", "mode", "not a member")), "mode"),
        Check.equal("reader_members_of", toString(len(ToolSchema.members(policy))), "2"),
        Check.equal("reader_nonarray_enum", len(ToolSchema.enum_values(JsonNull)), 0),
        Check.equal("reader_nonobject_members", len(ToolSchema.members(JsonInt(2))), 0),
        Check.equal("reader_nonstr_kind", ToolSchema.kind(decoded("{\"type\":7}")), ""),
        Check.equal("reader_unknown_kind", ToolSchema.kind_matches("unrecognised", JsonNull), False),
        Check.equal("reader_violation_without_keyword", ToolSchema.violation_keyword("plain error"), ""),
        Check.equal("reader_violation_without_path", ToolSchema.violation_path("enum@mode"), ""),
    ]


# The JSON scanner the mask is built on, driven byte at a time.

def scanner_cases():
    """One value per schema against the scanner: what a masked token can never write."""
    policy = decoded("{\"type\":\"object\",\"properties\":{\"mode\":{\"type\":\"string\",\"enum\":[\"check\",\"apply\"]},\"n\":{\"type\":\"integer\"}},\"required\":[\"mode\",\"n\"],\"additionalProperties\":false}")
    suites = decoded("{\"type\":\"array\",\"items\":{\"type\":\"string\"}}")
    nested = decoded("{\"type\":\"object\",\"properties\":{\"files\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\"}},\"required\":[\"path\"],\"additionalProperties\":false}}},\"required\":[\"files\"],\"additionalProperties\":false}")
    return listConcat([
        Check.equal("scan_valid", scanned(policy, "{\"mode\": \"apply\", \"n\": 1}"), "complete"),
        Check.equal("scan_enum", scanned(policy, "{\"mode\": \"delete\""), "rejected"),
        Check.equal("scan_key", scanned(policy, "{\"nosuch\": 1}"), "rejected"),
        Check.equal("scan_required", scanned(policy, "{\"mode\": \"check\"}"), "rejected"),
        Check.equal("scan_array", scanned(suites, "[\"gc\", \"arc\"]"), "complete"),
        Check.equal("scan_nested", scanned(nested, "{\"files\": [{\"path\": \"a\", \"path\": \"b\"}]}"), "rejected"),
        Check.equal("scan_bad_literal", scanned(suites, "[true]"), "rejected"),
        Check.equal("scan_prefix", scanned(policy, "{\"mode\": \"ap"), "incomplete"),
    ], scanner_boundary_cases())


def scanner_boundary_cases():
    """JSON scalar, escape and object boundaries under the same byte scanner."""
    closed_object = decoded("{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}")
    null_schema = decoded("{\"type\":\"null\"}")
    return [
        Check.equal("scan_null_schema", scanned(null_schema, "null"), "complete"),
        Check.equal("scan_true_at_end", scanned(JsonNull, "true"), "complete"),
        Check.equal("scan_literal_prefix_at_end", scanned(JsonNull, "nul"), "incomplete"),
        Check.equal("scan_nested_number_at_end", scanned(JsonNull, "[1"), "incomplete"),
        Check.equal("scan_nested_literal_at_end", scanned(JsonNull, "[null"), "incomplete"),
        Check.equal("scan_any_nested", scanned(JsonNull, "[\"text\",true,false,null,{},[],[-12.50E+23,1e-12,2e2]]"), "complete"),
        Check.equal("scan_escape_valid", scanned(JsonNull, "\"\\u0041\""), "complete"),
        Check.equal("scan_escape_invalid", scanned(JsonNull, "\"\\u00x1\""), "rejected"),
        Check.equal("scan_control_key", scanned(JsonNull, "{\"\n\":1}"), "rejected"),
        Check.equal("scan_repeated_key", scanned(JsonNull, "{\"a\":1,\"a\":2}"), "rejected"),
        Check.equal("scan_undeclared_key", scanned(closed_object, "{\"x\":1}"), "rejected"),
        Check.equal("scan_fraction_missing", scanned(JsonNull, "[1.e2]"), "rejected"),
        Check.equal("scan_exponent_missing", scanned(JsonNull, "[1e]"), "rejected"),
        Check.equal("scan_exponent_sign_missing", scanned(JsonNull, "[1e-]"), "rejected"),
        Check.equal("scan_exponent_open", scanned(JsonNull, "[1e+"), "incomplete"),
        Check.equal("scan_empty_stack_finished", finished([]), True),
        Check.equal("scan_empty_stack_no_consumption", empty_scan_unused(), True),
    ]


def empty_scan_unused():
    match ToolJsonScan.feed([], 65):
        case Success(value=step):
            return len(step.stack) == 0 and not step.consumed
        case Error():
            return False


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


# The logits-processor entry point itself, over a token history.

def logits_processor_cases():
    """`apply` and `mask` over a decoded history: the processor a server installs."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList([0, 1, 2, 7], lambda i: processor_case(tools, record_at(records, i), i)))


def processor_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    constraint = ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))
    ids = ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull))
    machine = machine_after(tools, ToolConstrain.decode(constraint, ids))
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda k: corpus_score(record, k))
    masked = ToolConstrain.apply(constraint, ids, scores)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0)
    return [
        Check.equal(f"processor_pick[{i}]", argmax(masked), corpus),
        Check.equal(f"processor_mask[{i}]", toString(len(ToolConstrain.mask(constraint, machine, scores))), toString(len(scores))),
        Check.equal(f"processor_accepts[{i}]", ToolConstrain.accepts(constraint, ToolConstrain.decode(constraint, ids) + token_at(constraint, corpus)), True),
    ]


def corpus_score(record, k):
    """The corpus token outranks everything, so the processor has to keep it."""
    return toFloat32(1.0) if k == unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0) else toFloat32(0.0)


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


def call_fingerprint_of(record):
    return f"{text_of(record, 'name')}|{text_of(record, 'args')}|"


def tool_cases():
    return listConcat(corner_cases(), listConcat(reader_cases(), listConcat(scanner_cases(), listConcat(validation_cases(), listConcat(coercion_cases(), listConcat(parse_cases(), listConcat(chunked_cases(), listConcat(repair_cases(), listConcat(constrain_cases(), listConcat(control_cases(), logits_processor_cases()))))))))))


# --- the tiny Qwen3_5TextConfig checkpoint: [LLM-MODEL-ORACLE] ---

def shape_size(shape):
    """The element count of one row-major shape."""
    return foldList(shape, 1, lambda total, extent: total * extent)


def shape_total(shapes):
    """The element count of every shape in a manifest."""
    return foldList(shapes, 0, lambda total, shape: total + shape_size(shape))


def manifest_cases(parameter_values):
    """The fixture's own counts: the declared parameter count is the sum of the recorded
    shapes, and every tensor has one manifest row per value list (so the fixture cannot
    describe a shape it does not carry)."""
    return [
        Check.equal("model_param_count", shape_total(Expected.MODEL_PARAM_SHAPES), Expected.MODEL_PARAM_COUNT),
        Check.equal("model_param_tensors", len(Expected.MODEL_PARAM_NAMES), len(Expected.MODEL_PARAM_SHAPES)),
        Check.equal("model_param_dtypes", len(Expected.MODEL_PARAM_DTYPES), len(parameter_values)),
        Check.equal("model_cache_tensors", len(Expected.MODEL_CACHE_NAMES), len(Expected.MODEL_CACHE_VALUES)),
        Check.equal("model_cache_dtypes", len(Expected.MODEL_CACHE_DTYPES), len(Expected.MODEL_CACHE_SHAPES)),
        Check.equal("model_hidden_stages", len(Expected.MODEL_HIDDEN_STAGES), Expected.MODEL_LAYERS + 1),
        Check.equal("model_hidden_states", len(Expected.MODEL_HIDDEN_VALUES), len(Expected.MODEL_INPUT_TOKENS) * len(Expected.MODEL_HIDDEN_STAGES)),
        Check.equal("model_inputs", len(Expected.MODEL_INPUTS), len(Expected.MODEL_INPUT_TOKENS)),
        Check.equal("model_layer_types", len(Expected.MODEL_LAYER_TYPES), Expected.MODEL_LAYERS),
        Check.equal("model_layer_pattern", join(Expected.MODEL_LAYER_TYPES, ","), "linear_attention,linear_attention,linear_attention,full_attention"),
        Check.equal("model_hook_names", len(Expected.MODEL_HOOK_NAMES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_hook_dtypes", len(Expected.MODEL_HOOK_DTYPES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_dtype_path", len(Expected.MODEL_DTYPE_PATH), 8),
    ]


def param_cases(parameter_values):
    """One assertion per recorded parameter tensor: its values fill its shape."""
    return toList(map(range(len(parameter_values)), lambda k: Check.equal(
        "model_param[" + name_at(Expected.MODEL_PARAM_NAMES, k) + "]",
        len(unwrap_or(listGet(parameter_values, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, k), [])),
    )))


def hidden_cases():
    """One assertion per recorded hidden state: one value per token per hidden channel."""
    return toList(map(range(len(Expected.MODEL_HIDDEN_VALUES)), lambda k: Check.equal(
        "model_hidden[" + name_at(Expected.MODEL_HIDDEN_STAGES, k % len(Expected.MODEL_HIDDEN_STAGES)) + "]",
        len(unwrap_or(listGet(Expected.MODEL_HIDDEN_VALUES, k), [])),
        unwrap_or(listGet(Expected.MODEL_HIDDEN_TOKENS, k), 0) * Expected.MODEL_HIDDEN,
    )))


def cache_cases():
    """One assertion per cache state tensor: its values fill its shape."""
    return toList(map(range(len(Expected.MODEL_CACHE_VALUES)), lambda k: Check.equal(
        "model_cache[" + name_at(Expected.MODEL_CACHE_NAMES, k) + "]",
        len(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_CACHE_SHAPES, k), [])),
    )))


def name_at(names, k):
    return unwrap_or(listGet(names, k), "")


def model_fixture_cases():
    """[LLM-MODEL-ORACLE] The tiny `Qwen3_5TextConfig` fixture transformers wrote is
    self-consistent (its counts and shapes), and the device model built from its
    parameters reproduces its activations and cache."""
    parameter_values = Expected.MODEL_PARAM_VALUES
    return listConcat(manifest_cases(parameter_values), listConcat(param_cases(parameter_values), listConcat(hidden_cases(), listConcat(cache_cases(), model_layer_cases(fixture_params(parameter_values))))))


# --- the qwen3_5 model on device buffers: [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE] ---

MODEL_RTOL = 1e-5
MODEL_ATOL = 1e-6
"""The port's tolerance against the fixture: the path runs binary32 arithmetic whose
dot products reduce in a different order from torch's CPU GEMM and whose `exp`/`ln`
come from the device library, so [PORT-TEST-ORACLE]'s exact-arithmetic tolerances do
not apply."""
SINGLE = 0
"""`MODEL_HIDDEN_VALUES` index of the one-token input's embedding; its stages follow."""
CACHED = 10
"""`MODEL_HIDDEN_VALUES` index of the cached run's embeddings; its stages follow."""


def index_in(names, name):
    """The position of `name` in a fixture name list, or -1 when it is absent."""
    return fold(range(len(names)), -1, lambda hit, i: i if hit < 0 and unwrap_or(listGet(names, i), "") == name else hit)


def param_shape(name):
    """One recorded parameter's shape, or `[]` when the manifest has no such name."""
    return unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, index_in(Expected.MODEL_PARAM_NAMES, name)), [])


def extent(shape, axis):
    return unwrap_or(listGet(shape, axis), 0)


def model_config():
    """The fixture's `Qwen3_5TextConfig`: the attention and MLP widths come from the
    recorded parameter shapes; the linear-attention head split (two key heads of eight,
    four value heads of eight) and the rope settings from the config the oracle's
    docstring records, since `[64, 64]` alone does not say how `key_dim` splits."""
    head_dim = extent(param_shape("layers.3.self_attn.q_norm.weight"), 0)
    conv = param_shape("layers.0.linear_attn.conv1d.weight")
    return Qwen3_5Config.Config(
        hidden_size=Expected.MODEL_HIDDEN,
        intermediate_size=extent(param_shape("layers.3.mlp.gate_proj.weight"), 0),
        vocab_size=Expected.MODEL_VOCAB,
        num_hidden_layers=Expected.MODEL_LAYERS,
        num_attention_heads=extent(param_shape("layers.3.self_attn.q_proj.weight"), 0) // (2 * head_dim),
        num_key_value_heads=extent(param_shape("layers.3.self_attn.k_proj.weight"), 0) // head_dim,
        head_dim=head_dim,
        partial_rotary_factor=0.25,
        rope_theta=10000.0,
        rms_norm_eps=0.000001,
        linear_num_key_heads=2,
        linear_num_value_heads=extent(param_shape("layers.0.linear_attn.A_log"), 0),
        linear_key_head_dim=8,
        linear_value_head_dim=extent(param_shape("layers.0.linear_attn.norm.weight"), 0),
        linear_conv_kernel_dim=extent(conv, 2),
        layer_types=Expected.MODEL_LAYER_TYPES,
    )


def to_floats(xs):
    """A numeric `MODEL_*` row as float32 (the fixture writes the shortest decimal that
    reads back as the same binary32)."""
    return mapList(xs, toFloat32)


def fixture_params(parameter_values):
    return Qwen3_5Hf.Params(names=Expected.MODEL_PARAM_NAMES, values=mapList(parameter_values, to_floats))


def hidden_row(k, t):
    """Row `t` of `MODEL_HIDDEN_VALUES[k]`."""
    return take_row(to_floats(unwrap_or(listGet(Expected.MODEL_HIDDEN_VALUES, k), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def hook_row(name, t):
    """Row `t` of one `MODEL_HOOK_*` activation, or `[]` when the fixture has no such hook."""
    return take_row(to_floats(unwrap_or(listGet(Expected.MODEL_HOOK_VALUES, index_in(Expected.MODEL_HOOK_NAMES, name)), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def cache_value(name):
    """One `MODEL_CACHE_*` state tensor, flattened, or `[]` when the cache has no such name."""
    return to_floats(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, index_in(Expected.MODEL_CACHE_NAMES, name)), []))


def take_row(xs, start, width):
    return toList(map(range(width), lambda i: unwrap_or(listGet(xs, start + i), toFloat32(0.0))))


def compare(name, buffer, want):
    """A device buffer read back and compared with a fixture row."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, got, want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def compare_rows(name, buffer, start, count, want):
    """`count` values of a buffer from `start` against a fixture tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, take_row(got, start, count), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def model_layer_cases(parameters):
    """The device model from the fixture's parameters: the one-token input and the
    eight tokens fed one at a time through the cache reproduce every recorded hidden
    stage, the first cached token every layer hook, the final cache every state tensor,
    and the loader refuses a missing or mis-sized parameter."""
    match Qwen3_5Hf.load(parameters, model_config()):
        case Error(message=message):
            return [Check.failed("model_load", message)]
        case Success(value=model):
            return flatten([[Check.passed("model_load")], model_geometry_cases(model), checkpoint_conversion_cases(), checked_device_cases(model.config), single_cases(model), model_interface_cases(model), cached_cases(model), tiled_cases(model, parameters), refusal_cases(), model_fault_cases(model)])


def model_geometry_cases(model):
    cfg = model.config
    return [
        Check.equal("model.config.linear", Qwen3_5Config.LINEAR, "linear_attention"),
        Check.equal("model.config.full", Qwen3_5Config.FULL, "full_attention"),
        Check.truth("model.config.layer_type", structuralEquals(toList(map(range(5), lambda i: Qwen3_5Config.layer_type(cfg, i))), listAppend(Expected.MODEL_LAYER_TYPES, "")), "layer kinds must match the fixture and end at index 4"),
        Check.truth("model.config.is_full", structuralEquals(toList(map(range(5), lambda i: Qwen3_5Config.is_full(cfg, i))), [False, False, False, True, False]), "only layer 3 is full attention"),
        Check.equal("model.config.value_dim", Qwen3_5Config.value_dim(cfg), 32),
        Check.equal("model.config.kv_groups", Qwen3_5Config.kv_groups(cfg), 1),
        Check.equal("model.config.rotary_dim", Qwen3_5Config.rotary_dim(cfg), 4),
        Check.equal("model.shapes.norm_chunk", model.shapes.norm_chunk, 64),
        Check.equal("model.shapes.vocab_chunk", model.shapes.vocab_chunk, 128),
        Check.ints_equal("model.shapes.chunk_of", [Qwen3_5Shapes.chunk_of(1024, Qwen3_5Shapes.NORM_CHUNK), Qwen3_5Shapes.chunk_of(1024, Qwen3_5Shapes.VOCAB_CHUNK), Qwen3_5Shapes.chunk_of(511, Qwen3_5Shapes.VOCAB_CHUNK)], [256, 512, 511]),
    ]


def checkpoint_conversion_cases():
    row = to_floats([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    return [
        close_list("model.checkpoint.transpose", Qwen3_5Hf.transposed(row, 2, 3), to_floats([1.0, 4.0, 2.0, 5.0, 3.0, 6.0]), 0.0, 0.0),
        close_list("model.checkpoint.centered", Qwen3_5Hf.plus_one(to_floats([-1.0, 0.0, 0.5])), to_floats([0.0, 1.0, 1.5]), 0.0, 0.0),
        close_list("model.checkpoint.decay", Qwen3_5Hf.neg_exp(to_floats([-1.0, 0.0, 1.0])), to_floats([-0.36787944, -1.0, -2.71828183]), MODEL_RTOL, MODEL_ATOL),
    ]


def checked_device_cases(cfg):
    match run_device_cases(cfg):
        case Success(value=checks):
            return checks
        case Error(message=message):
            return [Check.failed("model.device", message)]


def run_device_cases(cfg):
    with Qwen3_5Device.strictly:
        shapes = Qwen3_5Shapes.shapes_of(cfg)
        return Success(value=flatten([divider_cases(), buffer_cases(), shape_grid_cases(shapes), rope_cases(cfg, shapes), head_norm_cases()]))


def divider_cases():
    divisor = Qwen3_5Device.divider(3)
    expected = Qwen3_5Device.Div(mul=5726623062, shift=34)
    return [
        Check.ints_equal("model.device.divider", [divisor.mul, divisor.shift], [expected.mul, expected.shift]),
        Check.ints_equal("model.device.quot", mapList([0, 2, 3, 4, 17, 65535], lambda i: Qwen3_5Device.quot(i, divisor.mul, divisor.shift)), [0, 0, 1, 1, 5, 21845]),
        Check.ints_equal("model.device.rem", mapList([0, 2, 3, 4, 17, 65535], lambda i: Qwen3_5Device.rem(i, divisor.mul, divisor.shift, 3)), [0, 2, 0, 1, 2, 0]),
        Check.close("model.device.inverse", toFloat(Qwen3_5Device.inverse(8)), 0.125, 0.0, 0.0),
        Check.close("model.device.rsqrt", toFloat(Qwen3_5Device.rsqrt(toFloat32(4.0))), 0.5, MODEL_RTOL, MODEL_ATOL),
        Check.close("model.device.softplus", toFloat(Qwen3_5Device.softplus(toFloat32(0.0))), 0.69314718, MODEL_RTOL, MODEL_ATOL),
    ]


def buffer_cases():
    indices = Qwen3_5Device.must_ints(gpuIota(2))
    a = Qwen3_5Device.constant(to_floats([1.0, -2.0, 3.0]))
    b = Qwen3_5Device.constant(to_floats([4.0, 2.0]))
    return [
        compare("model.device.zeros", Qwen3_5Device.zeros(indices), to_floats([0.0, 0.0])),
        compare("model.device.added", Qwen3_5Device.added(a, b, indices), to_floats([5.0, 0.0])),
        compare("model.device.slice", Qwen3_5Device.slice_of(a, 1, indices), to_floats([-2.0, 3.0])),
        Check.close("model.device.squares", toFloat(Qwen3_5Device.squares(a, 1, 2)), 13.0, 0.0, 0.0),
        compare("model.device.sigmoid", Qwen3_5Device.sigmoid(Qwen3_5Device.constant(to_floats([-1.0, 0.0, 1.0]))), to_floats([0.26894142, 0.5, 0.73105858])),
        compare("model.device.Weight.dense", Qwen3_5Device.linear(b, Qwen3_5Device.Dense(values=b), 1, 2, 1), to_floats([20.0])),
    ]


def shape_grid_cases(shapes):
    return [
        Check.ints_equal("model.device.Shapes.grids", [gpuLength(shapes.hidden), gpuLength(shapes.conv), gpuLength(shapes.value), gpuLength(shapes.state)], [64, 64, 32, 288]),
    ]


def rope_cases(cfg, shapes):
    angles = Qwen3_5Rope.tables(1, cfg)
    head = Qwen3_5Device.constant(toList(map(range(1, 17), lambda i: toFloat32(i))))
    return [
        compare("model.rope.Tables.cos", angles.cos, to_floats([0.54030231, 0.99995, 0.54030231, 0.99995])),
        compare("model.rope.Tables.sin", angles.sin, to_floats([0.84147098, 0.00999983, 0.84147098, 0.00999983])),
        compare_rows("model.rope.rotated", Qwen3_5Rope.rotated(head, angles, Qwen3_5Device.grid(16), shapes, cfg), 0, 6, to_floats([-1.98411065, 1.95990067, 2.46237790, 4.01979967, 5.0, 6.0])),
    ]


def head_norm_cases():
    x = Qwen3_5Device.constant(to_floats([1.0, 1.0, 2.0, 2.0]))
    z = Qwen3_5Device.constant(to_floats([1.0, 1.0, 1.0, 1.0]))
    w = Qwen3_5Device.constant(to_floats([2.0, 3.0]))
    heads = Qwen3_5Device.grid(2)
    cells = Qwen3_5Device.grid(4)
    div = Qwen3_5Device.divider(2)
    return [
        compare("model.norm.rms_heads", Qwen3_5Norm.rms_heads(x, w, heads, cells, div, 2, toFloat32(0.0)), to_floats([2.0, 3.0, 2.0, 3.0])),
        compare("model.norm.gated", Qwen3_5Norm.gated(x, z, w, heads, cells, div, 2, toFloat32(0.0)), to_floats([1.46211716, 2.19317574, 1.46211716, 2.19317574])),
    ]


def token_in(tokens, t):
    return unwrap_or(listGet(tokens, t), 0)


def input_tokens(i):
    return unwrap_or(listGet(Expected.MODEL_INPUTS, i), [])


def started(model, label, run):
    """`run` over a fresh cache, or the failure to allocate one."""
    match Qwen3_5Cache.empty(model.config, model.shapes):
        case Success(value=cache):
            return run(cache)
        case Error(message=message):
            return [Check.failed(f"{label}_cache", message)]


def single_cases(model):
    """`MODEL_INPUTS[0]`, one token at position 0, from a fresh cache."""
    return started(model, "model_single", lambda cache: single_run(model, cache))


def single_run(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_single", message)]
        case Success(value=next):
            return listConcat(stage_cases("model_single", next, SINGLE, 0), [greedy_case("model_single", next), public_greedy_case(model, next)])


def public_greedy_case(model, next):
    match public_greedy(model, next.logits):
        case Success(value=best):
            return Check.equal("model.Next.greedy", best, next.best)
        case Error(message=message):
            return Check.failed("model.Next.greedy", message)


def public_greedy(model, logits):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            return Success(value=Qwen3_5Model.greedy(model, logits))


def model_interface_cases(model):
    match run_model_interfaces(model):
        case Success(value=checks):
            return checks
        case Error(message=message):
            return [Check.failed("model.interfaces", message)]


def run_model_interfaces(model):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            return Success(value=started(model, "model_interfaces", lambda cache: model_interface_run(model, cache)))


def model_interface_run(model, cache):
    row = Qwen3_5Model.embed(model, token_in(input_tokens(2), 0))
    match Qwen3_5Model.forward(model, cache, row):
        case Error(message=message):
            return [Check.failed("model.Walk.forward", message)]
        case Success(value=walk):
            return flatten([[compare("model.Embedding.row", row, hidden_row(CACHED, 0)), compare("model.Walk.forward", walk.hidden, hook_row("layers.3", 0))], layer_interface_cases(model, cache, row, 0), layer_interface_cases(model, cache, Qwen3_5Device.constant(hook_row("layers.2", 0)), 3)])


def layer_interface_cases(model, cache, row, index):
    match listGet(model.layers, index):
        case Error(message=message):
            return [Check.failed(f"model.Layer.{index}", toString(message))]
        case Success(value=layer):
            match Qwen3_5Cache.slot_at(cache, index):
                case Error(message=message):
                    return [Check.failed(f"model.Slot.{index}", toString(message))]
                case Success(value=slot):
                    return layer_hook_cases(model, row, index, layer, slot)


def layer_hook_cases(model, row, index, layer, slot):
    cfg = model.config
    shapes = model.shapes
    normed = Qwen3_5Norm.rms_row(row, layer.input_norm, cfg, shapes)
    return flatten([
        [compare(f"model.Layer.input_norm.{index}", normed, hook_row(f"layers.{index}.input_layernorm", 0))],
        mixer_cases(model, layer.mixer, slot, normed),
        [compare(f"model.Mlp.swiglu.{index}", Qwen3_5Mlp.swiglu(Qwen3_5Device.constant(hook_row(f"layers.{index}.post_attention_layernorm", 0)), layer.mlp, cfg, shapes), hook_row(f"layers.{index}.mlp", 0))],
    ])


def mixer_cases(model, mixer, slot, row):
    match mixer:
        case Qwen3_5Model.Linear(layer=layer):
            match slot:
                case Qwen3_5Cache.Delta(held=held):
                    return delta_mixer_cases(model, layer, row, held)
                case Qwen3_5Cache.Attention():
                    return [Check.failed("model.Mixer.linear", "expected the delta slot")]
        case Qwen3_5Model.Full(layer=layer):
            match slot:
                case Qwen3_5Cache.Attention(held=held):
                    return attention_mixer_cases(model, layer, row, held)
                case Qwen3_5Cache.Delta():
                    return [Check.failed("model.Mixer.attention", "expected the attention slot")]


def delta_mixer_cases(model, layer, row, held):
    mixed = Qwen3_5DeltaNet.mix(row, layer, held, model.tiled, model.config, model.shapes)
    slot = Qwen3_5Cache.Delta(held=Qwen3_5Cache.DeltaSlot(conv=mixed.slot.conv, state=mixed.slot.state))
    grown = Qwen3_5Cache.grown(Qwen3_5Cache.Cache(slots=[]), slot)
    return [
        compare("model.Mixer.linear.Mixed", mixed.hidden, hook_row("layers.0.linear_attn", 0)),
        Check.equal("model.DeltaSlot.grown", len(grown.slots), 1),
    ]


def attention_mixer_cases(model, layer, row, held):
    mixed = Qwen3_5Attention.mix(row, layer, held, model.config, model.shapes)
    return [
        compare("model.Mixer.attention.Mixed", mixed.hidden, hook_row("layers.3.self_attn", 0)),
        Check.equal("model.AttentionSlot.positions", mixed.slot.positions, 1),
    ]


def stage_cases(label, next, base, t):
    """The embedding, the three recorded layer outputs and the final norm of row `t`."""
    return listConcat(
        [compare(f"{label}_embed", next.row, hidden_row(base, t))],
        listConcat(
            toList(map(range(len(Expected.MODEL_HIDDEN_STAGES) - 2), lambda i: stage_case(label, next, base, t, i))),
            [compare(f"{label}_final_norm", next.normed, hidden_row(base + len(Expected.MODEL_HIDDEN_STAGES) - 1, t))],
        ),
    )


def stage_case(label, next, base, t, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"{label}_layer{i}", stage, hidden_row(base + 1 + i, t))
        case Error(message=message):
            return Check.failed(f"{label}_layer{i}", toString(message))


def greedy_case(label, next):
    """The device argmax agrees with the host's over the same logits."""
    match fromGpu(next.logits):
        case Success(value=values):
            return Check.equal(f"{label}_greedy", next.best, argmax(values))
        case Error(message=message):
            return Check.failed(f"{label}_greedy", toString(message))


def cached_cases(model):
    """`MODEL_INPUTS[2]` fed one token at a time, each through the cache the earlier tokens left."""
    return started(model, "model_cached", lambda cache: walk_tokens(model, input_tokens(2), 0, cache, []))


def walk_tokens(model, tokens, t, cache, acc):
    if t == len(tokens):
        return listConcat(acc, cache_tensor_cases(model, cache))
    else:
        match Qwen3_5Model.decode(model, cache, token_in(tokens, t)):
            case Error(message=message):
                return listAppend(acc, Check.failed(f"model_cached{t}", message))
            case Success(value=next):
                return walk_tokens(model, tokens, t + 1, next.walk.cache, listConcat(acc, listConcat(stage_cases(f"model_cached{t}", next, CACHED, t), hook_cases(next, t))))


def hook_cases(next, t):
    """At position 0 the batched run and the cached run agree, so the first token also
    checks every layer's output hook, including layer 3's, which no hidden stage records."""
    if t == 0:
        return toList(map(range(Expected.MODEL_LAYERS), lambda i: hook_case(next, i)))
    else:
        return []


def hook_case(next, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"model_hook_layer{i}", stage, hook_row(f"layers.{i}", 0))
        case Error(message=message):
            return Check.failed(f"model_hook_layer{i}", toString(message))


def cache_tensor_cases(model, cache):
    """Every slot of the final cache against the fixture's `layers.N.*` state tensors."""
    return flatten(toList(map(range(model.config.num_hidden_layers), lambda i: slot_cases(model, cache, i))))


def slot_cases(model, cache, i):
    match Qwen3_5Cache.slot_at(cache, i):
        case Error(message=message):
            return [Check.failed(f"model_cache_slot{i}", toString(message))]
        case Success(value=slot):
            return held_cases(model, slot, i)


def held_cases(model, slot, i):
    match slot:
        case Qwen3_5Cache.Delta(held=held):
            return [
                compare(f"model_cache{i}_conv_states", held.conv, cache_value(f"layers.{i}.conv_states.0")),
                compare_rows(f"model_cache{i}_recurrent_states", held.state, 0, Qwen3_5Config.state_cells(model.config), cache_value(f"layers.{i}.recurrent_states.0")),
            ]
        case Qwen3_5Cache.Attention(held=held):
            return [
                compare_heads(f"model_cache{i}_keys", held.keys, held.positions, model.config, cache_value(f"layers.{i}.keys.0")),
                compare_heads(f"model_cache{i}_values", held.values, held.positions, model.config, cache_value(f"layers.{i}.values.0")),
            ]


def compare_heads(name, buffer, positions, cfg, want):
    """A position-major KV buffer against the fixture's `[heads, positions, head_dim]` tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, head_major(got, positions, cfg.num_key_value_heads, cfg.head_dim), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def head_major(flat, positions, heads, width):
    return toList(map(range(heads * positions * width), lambda j: unwrap_or(listGet(flat, ((j // width) % positions) * heads * width + (j // (positions * width)) * width + j % width), toFloat32(0.0))))


def tiled_cases(model, parameters):
    """The GGUF value-head order: the same checkpoint with every per-value-head
    parameter permuted into llama.cpp's tiled order, loaded as `tiled`, yields the same
    one-token activations as Transformers' grouped order."""
    match Qwen3_5Hf.load(tiled_params(model.config, parameters), model.config):
        case Error(message=message):
            return [Check.failed("model_tiled_load", message)]
        case Success(value=grouped):
            return tiled_run(Qwen3_5Model.Model(config=grouped.config, shapes=grouped.shapes, embedding=grouped.embedding, layers=grouped.layers, final_norm=grouped.final_norm, head=grouped.head, tiled=1))


def tiled_run(model):
    return started(model, "model_tiled", lambda cache: tiled_decode(model, cache))


def tiled_decode(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_tiled", message)]
        case Success(value=next):
            return stage_cases("model_tiled", next, SINGLE, 0)


def tiled_params(cfg, parameters):
    """`fixture_params` with the linear-attention value heads in tiled order, as
    `convert_hf_to_gguf.py`'s `_reorder_v_heads` writes them."""
    values = toList(map(range(len(parameters.names)), lambda k: tiled_param(cfg, name_at(parameters.names, k), unwrap_or(listGet(parameters.values, k), []))))
    return Qwen3_5Hf.Params(names=parameters.names, values=values)


def tiled_param(cfg, name, values):
    hk = cfg.linear_num_key_heads
    r = cfg.linear_num_value_heads // hk
    dv = cfg.linear_value_head_dim
    hidden = cfg.hidden_size
    if contains(name, "in_proj_qkv") or contains(name, "conv1d"):
        return reordered_rows(values, 2 * Qwen3_5Config.key_dim(cfg), hk, r, dv, len(values) // Qwen3_5Config.conv_dim(cfg))
    elif contains(name, "in_proj_z"):
        return reordered_rows(values, 0, hk, r, dv, hidden)
    elif contains(name, "in_proj_b") or contains(name, "in_proj_a"):
        return reordered_rows(values, 0, hk, r, 1, hidden)
    elif contains(name, "A_log") or contains(name, "dt_bias"):
        return reordered_rows(values, 0, hk, r, 1, 1)
    elif contains(name, "out_proj"):
        return reordered_columns(values, hk, r, dv, hidden)
    else:
        return values


def grouped_of(tiled, hk, r):
    """The grouped value head that sits at tiled position `tiled`: `(t % hk) * r + t // hk`."""
    return (tiled % hk) * r + tiled // hk


def reordered_rows(values, skip, hk, r, dim, width):
    """Rows of `width` from `skip` on, grouped by value head of `dim` rows, in tiled order."""
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, j if j < skip * width else source_row(j - skip * width, hk, r, dim, width) + skip * width), toFloat32(0.0))))


def source_row(j, hk, r, dim, width):
    row = j // width
    return (grouped_of(row // dim, hk, r) * dim + row % dim) * width + j % width


def reordered_columns(values, hk, r, dim, rows):
    """`[rows, hk * r * dim]` with its columns in tiled value-head order."""
    cols = hk * r * dim
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, (j // cols) * cols + grouped_of((j % cols) // dim, hk, r) * dim + j % dim), toFloat32(0.0))))


def refusal_cases():
    """A checkpoint missing a parameter, or holding one of the wrong size, is refused by name."""
    cfg = model_config()
    return [
        refusal("model_load_missing", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["norm.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight is not in the checkpoint"),
        refusal("model_load_sized", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["embed_tokens.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight holds 1 values, not 8192"),
    ]


def refusal(name, result, want):
    match result:
        case Success(value=_):
            return Check.failed(name, "loaded a checkpoint that should have been refused")
        case Error(message=message):
            return Check.equal(name, message, want)


def model_fault_cases(model):
    return listConcat([
        refusal("model_decode_missing_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[]), 0), "layer 0: no cache slot: index out of bounds"),
        refusal("model_step_missing_slot", step_missing_slot(model), "layer 0: no cache slot: index out of bounds"),
        refusal("model_fault_negative_grid", negative_grid(), "gpu: buffer allocation: a negative element count"),
        refusal("model_fault_missing_values", missing_values(), "index out of bounds"),
        refusal("model.Fault.halt_ints", explicit_device_fault(), "test device fault"),
    ], started(model, "model_wrong_slots", lambda cache: wrong_slot_cases(model, cache)))


def step_missing_slot(model):
    with Qwen3_5Device.strictly:
        return Qwen3_5Model.step(model, Qwen3_5Cache.Cache(slots=[]), Qwen3_5Model.embed(model, 0))


def wrong_slot_cases(model, cache):
    match Qwen3_5Cache.slot_at(cache, 0):
        case Error(message=message):
            return [Check.failed("model_wrong_slots", toString(message))]
        case Success(value=delta):
            match Qwen3_5Cache.slot_at(cache, 3):
                case Error(message=message):
                    return [Check.failed("model_wrong_slots", toString(message))]
                case Success(value=attention):
                    return mismatch_cases(model, cache, delta, attention)


def mismatch_cases(model, cache, delta, attention):
    return [
        refusal("model_linear_wrong_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[attention]), 0), "layer 0: a linear_attention layer was given an attention cache slot"),
        refusal("model_full_wrong_slot", Qwen3_5Model.decode(model, Qwen3_5Cache.Cache(slots=[delta, delta, delta, delta]), 0), "layer 3: a full_attention layer was given a linear cache slot"),
        refusal("model_decode_missing_layer", Qwen3_5Model.decode(model_with(model, [], model.embedding, model.head), cache, 0), "layer 0: index out of bounds"),
        mapped_missing_file(model, cache),
        quantized_head_fault(model, cache),
    ]


def model_with(model, layers, embedding, head):
    return Qwen3_5Model.Model(config=model.config, shapes=model.shapes, embedding=embedding, layers=layers, final_norm=model.final_norm, head=head, tiled=model.tiled)


def mapped_missing_file(model, cache):
    path = "/nonexistent_osprey_dir/missing-embedding.gguf"
    match mapped_decode(model, cache, path):
        case Error(message=message):
            return Check.truth("model_mapped_missing_file", startsWith(message, "gpuMapFile: " + path + ":"), message)
        case Success():
            return Check.failed("model_mapped_missing_file", "decoded a missing embedding file")


def mapped_decode(model, cache, path):
    with Qwen3_5Device.strictly:
        with Qwen3_5Device.wrapping:
            mapped = Qwen3_5Model.Mapped(path=path, offset=0, row_bytes=144, type="Q4_K", basis=Qwen3_5Model.identity(model.config.hidden_size))
            return Qwen3_5Model.decode(model_with(model, model.layers, mapped, model.head), cache, 0)


def quantized_head_fault(model, cache):
    match toGpu(toList(map(range(144), lambda _i: toUint8(0)))):
        case Error(message=message):
            return Check.failed("model_quantized_head_fault", toString(message))
        case Success(value=bytes):
            head = Qwen3_5Device.Quant(blocks=bytes, type="Q2_K")
            return refusal("model_quantized_head_fault", Qwen3_5Model.decode(model_with(model, model.layers, model.embedding, head), cache, 0), "gpuMatMulQuant: weights: unsupported tensor type; expected Q4_K, Q8_0, Q6_K, Q5_K, Q3_K, IQ4_NL, IQ4_XS, IQ3_S")


def negative_grid():
    with Qwen3_5Device.strictly:
        return Success(value=Qwen3_5Device.grid(-1))


def missing_values():
    with Qwen3_5Device.strictly:
        return Success(value=Qwen3_5Device.must_values(listGet([[toFloat32(1.0)]], 4)))


def explicit_device_fault():
    with Qwen3_5Device.strictly:
        return Success(value=perform(Qwen3_5Device.Fault.halt_ints("test device fault")))
