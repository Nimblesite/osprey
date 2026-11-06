# osprey: flavor=python
"""The Qwen3.5 example model's suite: the tool-call parser, scanner and constraint
against the values the real chat template and jsonschema produced
(oracle/tool_calls.cpython); `ModelCases` runs the device model against every activation
and cache state transformers 5.20.2 + torch 2.14.1 computed for a tiny checkpoint
(oracle/qwen3_5.cpython), and `GgufCases` the GGUF loader against the same checkpoint
written as tiny GGUF files (oracle/qwen3_5_gguf.cpython)."""
from testing import Check
from qwen_tools_expected import ExpectedTools
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from transformers.Math import argmax
from qwen_tests import GgufCases
from qwen_tests import ModelCases
from qwen_tests.ModelCases import flatten, refusal


def main():
    return Check.run(lambda: flatten([tool_cases(), ModelCases.model_fixture_cases(), GgufCases.gguf_cases()]))


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
    return unwrap_or(jsonEncode(value), "?")


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
            return violation(message)


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


def case_tokens(record):
    """The fixture case's token texts."""
    return texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))


def case_constraint(tools, record):
    """The constraint a server installs: the case's tokens under `tools`."""
    return ToolConstrain.Constrain(tools, case_tokens(record), toFloat32(0.0) - toFloat32(exp(1000.0)))


def constrain_case(tools, record, i):
    tokens = case_tokens(record)
    constraint = case_constraint(tools, record)
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
    constraint = case_constraint(tools, record)
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
