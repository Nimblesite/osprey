# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=ToolCallGrammar
"""The call grammar inside <tool_call>: the function name, its parameters, values and closes.

Implements [LLM-TOOL-CONSTRAIN] of docs/specs/applications/LocalCodingModel.md.
"""

from qwen import ToolJsonScan
from qwen import ToolSchema

__all__ = ["Machine", "step_call", "tool_names", "reset", "restart", "of_char"]


CALL_CLOSE = "</tool_call>"
FUNCTION_OPEN = "<function="
FUNCTION_CLOSE = "</function>"
PARAMETER_OPEN = "<parameter="
PARAMETER_CLOSE = "</parameter>"
RAW_CLOSE = "</parameter>"


class Machine:
    """How far a turn has got into the constrained language.

    `mode` is "free" before a call opens and after it closes; `stage` names the
    step inside the call grammar; `stack` is the JSON value being scanned for the
    parameter currently written; `finished` records that a call has closed. The
    request's tool list is never held here: `names` and the chosen tool's `anchor`
    schema are the whole of it, because a record is copied at every byte and a tool
    list would be copied with it."""
    names: list[str]
    anchor: Json
    params: list[str]
    required: list[str]
    mode: str
    stage: str
    progress: int
    name: str
    param: str
    value: str
    candidates: list[str]
    seen: list[str]
    stack: list[ToolJsonScan.Frame]
    finished: bool


class Outcome:
    """One byte's effect: the new machine and whether the byte was consumed."""
    machine: Machine
    consumed: bool


# --- the call grammar -----------------------------------------------------

def step_call(machine, tools, byte):
    """One byte of the call grammar's frame: the function's name and its close."""
    match machine.stage:
        case "fn":
            return strict(machine, byte, FUNCTION_OPEN, "name")
        case "name":
            return name_byte(machine, tools, byte)
        case "params":
            return params_byte(machine, byte)
        case "tag":
            return tag_byte(machine, byte)
        case "fnclose":
            return strict(machine, byte, FUNCTION_CLOSE, "cend")
        case _:
            return step_argument(machine, byte)


def step_argument(machine, byte):
    """One byte of one argument: its opening tag, its name, its separator, its close."""
    match machine.stage:
        case "pnopen":
            return strict(machine, byte, PARAMETER_OPEN, "pname")
        case "pname":
            return parameter_name_byte(machine, byte)
        case "sep":
            return separator_byte(machine, byte)
        case "valuestart":
            return value_start_byte(machine, byte)
        case "ptail":
            return closed_parameter(machine, byte)
        case _:
            return argument_value_byte(machine, byte)


def argument_value_byte(machine, byte):
    """One byte of an argument's value: verbatim text, an enum member, or JSON."""
    match machine.stage:
        case "raw":
            return raw_byte(machine, byte)
        case "penum":
            return prefix_byte(machine, byte)
        case "json":
            return json_byte(machine, byte)
        case _:
            return closed_call(machine, byte)


def closed_parameter(machine, byte):
    """The closing `</parameter>`, after which no parameter is open."""
    match strict(machine, byte, PARAMETER_CLOSE, "params"):
        case Success(value=outcome):
            return Success(value=Outcome(machine=update(outcome.machine, param="", value=""), consumed=outcome.consumed))
        case Error(message=message):
            return Error(message=message)


def closed_call(machine, byte):
    """The closing `</tool_call>`, after which the turn is free text again."""
    match strict(machine, byte, CALL_CLOSE, "fn"):
        case Success(value=outcome):
            return Success(value=Outcome(machine=closed(outcome.machine), consumed=outcome.consumed))
        case Error(message=message):
            return Error(message=message)


def closed(machine):
    """The machine a completed call leaves: free text, one call recorded."""
    cleared = reset(machine, "free")
    return update(cleared, finished=True) if machine.stage == "fn" else machine


def strict(machine, byte, literal, next_stage):
    """A required literal, after optional whitespace at its first byte."""
    if machine.progress == 0 and is_space(byte):
        return moved(machine)
    elif byte == of_char(literal, machine.progress):
        return literal_matched(machine, literal, next_stage)
    else:
        return Error(message=ToolSchema.violation("constrain", machine.stage, f"expected {literal}"))


def literal_matched(machine, literal, next_stage):
    """One literal byte matched: finish the literal or record the progress."""
    if machine.progress + 1 == len(literal):
        return moved(update(machine, stage=next_stage, progress=0))
    else:
        return moved(update(machine, progress=machine.progress + 1))


def name_byte(machine, tools, byte):
    """A function name: a prefix of a declared name, then its `>`, which binds the
    tool and keeps its parameter schema in the machine."""
    if byte == of_char(">", 0) and declares_tool(machine, machine.name):
        anchor = anchor_of(tools, machine.name)
        bound = update(machine, stage="params", progress=0, anchor=anchor, params=ToolSchema.property_names(anchor), required=ToolSchema.required_names(anchor))
        return moved(bound)
    else:
        return extended_name(machine, byte)


def moved(machine):
    """The outcome after consuming this byte into `machine`."""
    return Success(value=Outcome(machine=machine, consumed=True))


def declared_prefix(declared, text):
    """Whether `text` is still a prefix of one of the declared spellings."""
    return len(filterList(declared, lambda candidate: startsWith(candidate, text))) > 0


def extended_name(machine, byte):
    """Extend the name with a byte that keeps it a prefix of some declared name."""
    longer = machine.name + char_of(byte)
    if declared_prefix(machine.names, longer):
        return moved(update(machine, name=longer))
    else:
        return Error(message=ToolSchema.violation("constrain", "name", "undeclared function name"))


def tool_names(tools):
    """The declared function names of the request's tool list."""
    return mapList(ToolSchema.tool_list(tools), lambda tool: ToolSchema.tool_name(tool))


def anchor_of(tools, name):
    """The chosen tool's `parameters` schema, kept in the machine after the name."""
    return ToolSchema.parameters(ToolSchema.tool_named(tools, name))


def declares_tool(machine, name):
    return len(filterList(machine.names, lambda declared: declared == name)) > 0


def params_byte(machine, byte):
    """After a name's `>`: whitespace, then `<parameter=` or `</function>`."""
    if is_space(byte):
        return moved(machine)
    elif byte == of_char("<", 0):
        return moved(update(machine, stage="tag", progress=1))
    else:
        return Error(message=ToolSchema.violation("constrain", "params", "expected <"))


def tag_byte(machine, byte):
    """After `<`: `parameter=` opens a parameter, `/function>` closes the call. A
    parameter may only open while the tool still has a property left to write, so the
    grammar cannot reach a name it has nothing to spell."""
    if byte == of_char("p", 0) and len(unwritten_parameters(machine)) > 0:
        return moved(update(machine, stage="pnopen", progress=2))
    elif byte == of_char("p", 0):
        return Error(message=ToolSchema.violation("constrain", "params", "every parameter is already written"))
    elif byte == of_char("/", 0) and every_required_written(machine):
        return moved(update(machine, stage="fnclose", progress=2))
    elif byte == of_char("/", 0):
        return Error(message=ToolSchema.violation("constrain", "params", "a required parameter is missing"))
    else:
        return Error(message=ToolSchema.violation("constrain", "params", "expected parameter or /function"))


def every_required_written(machine):
    """Whether every required property of the named tool already appears."""
    return len(filterList(machine.required, lambda name: len(filterList(machine.seen, lambda written: written == name)) == 0)) == 0


def parameter_name_byte(machine, byte):
    """A parameter name: a prefix of a declared property not yet written, then its `>`."""
    if byte == of_char(">", 0) and declared_parameter(machine, machine.param):
        written = update(machine, stage="sep", param=machine.param, value="", seen=listAppend(machine.seen, machine.param))
        return moved(update(written, params=without(machine.params, machine.param)))
    else:
        return extended_parameter(machine, byte)


def extended_parameter(machine, byte):
    """Extend the parameter name with a byte that keeps it a prefix of one still unwritten."""
    longer = machine.param + char_of(byte)
    if declared_prefix(unwritten_parameters(machine), longer):
        return moved(update(machine, param=longer))
    else:
        return Error(message=ToolSchema.violation("constrain", "pname", "undeclared parameter name"))


def unwritten_parameters(machine):
    """The declared property names of the chosen tool that have not been written yet."""
    return machine.params


def without(names, name):
    """`names` without `name`."""
    return filterList(names, lambda written: written != name)


def declared_parameter(machine, name):
    return len(filterList(unwritten_parameters(machine), lambda declared: declared == name)) > 0


def separator_byte(machine, byte):
    """The template's single newline between `<parameter=X>` and the value."""
    if byte == of_char("\n", 0):
        return moved(update(machine, stage="valuestart", progress=0))
    else:
        return Error(message=ToolSchema.violation("constrain", "sep", "expected a newline after <parameter=NAME>"))


def value_start_byte(machine, byte):
    """Choose the value scanner: verbatim for a string, JSON for everything else."""
    schema = ToolSchema.property_schema(machine.anchor, machine.param)
    if ToolSchema.kind(schema) == "string" and len(ToolSchema.enum_values(schema)) == 0:
        return raw_byte(update(machine, stage="raw", progress=0), byte)
    elif ToolSchema.kind(schema) == "string":
        members = mapList(ToolSchema.enum_values(schema), quoted_text)
        return prefix_byte(update(machine, stage="penum", value="", progress=0, candidates=members), byte)
    else:
        return json_byte(update(machine, stage="json", stack=ToolJsonScan.begin(schema)), byte)


def quoted_text(member):
    return unwrap_or(jsonText(member), unwrap_or(jsonEncode(member), ""))


def json_byte(machine, byte):
    """One byte of a JSON argument, scanned against its schema."""
    match ToolJsonScan.feed(machine.stack, byte):
        case Success(value=stepped):
            return after_json(update(machine, stack=stepped.stack), stepped.consumed)
        case Error(message=message):
            return Error(message=message)


def after_json(machine, consumed):
    """A JSON byte consumed, or the value complete at this byte."""
    if len(machine.stack) == 0:
        return Success(value=Outcome(machine=update(machine, stage="ptail", progress=0), consumed=consumed))
    else:
        return Success(value=Outcome(machine=machine, consumed=consumed))


def raw_byte(machine, byte):
    """A verbatim string argument: any byte, ended by `</parameter>`. A mismatch is
    still value content, so the scan restarts; the template's newline before the tag is
    value text, and `ToolParser` strips it exactly as this scan counted it."""
    if byte == of_char(RAW_CLOSE, machine.progress):
        return matched_close(machine)
    else:
        return moved(update(machine, progress=restart(RAW_CLOSE, byte)))


def matched_close(machine):
    """One byte of the closing sequence matched, or the parameter closing."""
    if machine.progress + 1 == len(RAW_CLOSE):
        return moved(update(machine, stage="params", progress=0, param="", value=""))
    else:
        return moved(update(machine, progress=machine.progress + 1))


def prefix_byte(machine, byte):
    """An enum member or literal: extend it, or finish when it is complete."""
    longer = machine.value + char_of(byte)
    if declared_prefix(machine.candidates, longer):
        return moved(update(machine, value=longer))
    elif len(filterList(machine.candidates, lambda candidate: candidate == machine.value)) > 0:
        return Success(value=Outcome(machine=update(machine, stage="ptail", progress=0), consumed=False))
    else:
        return Error(message=ToolSchema.violation("constrain", "value", "not a declared member"))


def char_of(byte):
    """A byte as text, for comparing against ASCII literals and names."""
    return unwrap_or(fromCodePoint(byte), "")


def of_char(text, i):
    """The UTF-8 byte at `i` of an ASCII literal, or -1."""
    if i >= len(text):
        return -1
    else:
        return unwrap_or(byteAt(text, i), -1)


def is_space(byte):
    return byte == of_char(" ", 0) or byte == of_char("\n", 0) or byte == of_char("\r", 0) or byte == of_char("\t", 0)


def reset(machine, mode):
    """The machine with no call open: `mode` ("call" or "free") and a fresh
    grammar, every per-call field cleared."""
    return update(machine, mode=mode, stage="fn", progress=0, name="", param="", value="", candidates=[], seen=[], stack=[], anchor=JsonNull, params=[], required=[])


def restart(literal, byte):
    """Progress after a byte that did not match: restart at a fresh first byte."""
    if byte == of_char(literal, 0):
        return 1
    else:
        return 0
