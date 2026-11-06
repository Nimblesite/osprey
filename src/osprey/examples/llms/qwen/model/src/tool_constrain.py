# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=ToolConstrain
"""A logits processor that makes a schema-invalid tool call unsamplable.

Once the model opens `<tool_call>`, every token the sampler may pick has to leave
the turn a prefix of a call the request's tool schemas accept: a declared
function name, a declared parameter name, and a value its schema allows. Before
`<tool_call>` and after `</tool_call>` everything stays free, so ordinary replies
are unaffected. Warpers still follow the mask, exactly as in
`transformers.LogitsProcess`.

`Constrain(tools, vocabulary, filter_value)` builds the processor over a token
vocabulary (`vocabulary[i]` is token `i`'s text) and `apply` has the
`LogitsProcessorList` signature: the token history plus one logits row. `start`
and `advance` are the incremental form a server's sampler uses, and `accepts`
answers whether a whole text stays inside the language.

The language is the template's own: `<tool_call>`, then `<function=NAME>`, then
one `<parameter=NAME>` per argument, then `</function></tool_call>`. A string
argument is written verbatim, because the template writes a string argument
unquoted; every other argument is written as a JSON literal, because that is what
the template's `tojson` produced. `ToolJsonScan` scans a JSON argument against
its own schema, so a nested object can only hold its declared properties, its
enums, and values of their declared types.

Implements [LLM-TOOL-CONSTRAIN] of docs/specs/applications/LocalCodingModel.md.
"""

from qwen import ToolJsonScan
from qwen import ToolSchema

__all__ = ["start", "advance", "complete", "Constrain", "apply", "mask", "accepts", "decode"]


CALL_OPEN = "<tool_call>"
CALL_CLOSE = "</tool_call>"
FUNCTION_OPEN = "<function="
FUNCTION_CLOSE = "</function>"
PARAMETER_OPEN = "<parameter="
PARAMETER_CLOSE = "</parameter>"
RAW_CLOSE = "</parameter>"
STAGE_ATTEMPTS = 4


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


class Constraint:
    """A masking processor: the tool schemas, the vocabulary and the filter value."""
    tools: Json
    vocabulary: list[str]
    filter_value: float32


class Outcome:
    """One byte's effect: the new machine and whether the byte was consumed."""
    machine: Machine
    consumed: bool


# --- construction ---------------------------------------------------------

def start(tools) -> Machine:
    """A machine at the start of a turn: free text, no call open."""
    return Machine(names=tool_names(tools), anchor=JsonNull, params=[], required=[], mode="free", stage="fn", progress=0, name="", param="", value="", candidates=[], seen=[], stack=[], finished=False)


def Constrain(tools, vocabulary, filter_value) -> Constraint:
    """The processor over one request's tools and one token vocabulary."""
    return Constraint(tools=tools, vocabulary=vocabulary, filter_value=filter_value)


def complete(machine):
    """Whether a call has closed, so the turn now holds a schema-valid call."""
    return machine.finished


# --- the logits processor -------------------------------------------------

def apply(constraint: Constraint, input_ids, scores: list[float32]):
    """`LogitsProcessorList`'s signature: mask every token that would leave the
    constrained language, leaving `filter_value` (usually `-inf`) in its place."""
    return mask(constraint, record(constraint, decode(constraint, input_ids)), scores)


def mask(constraint, machine, scores):
    """One logits row with the impossible continuations filtered.

    `apply` is this over the machine a token history reaches; a sampler that already
    holds the machine (the suite's driver, a server between steps) calls `mask`
    directly and pays no replay."""
    return toList(map(range(len(scores)), lambda i: kept(constraint, machine, i, scores)))


def kept(constraint, machine, i, scores):
    """Token `i`'s score when the language allows it after `machine`, else the filter."""
    if accepts_token(machine, constraint.tools, token_at(constraint, i)):
        return unwrap_or(listGet(scores, i), toFloat32(0.0))
    else:
        return constraint.filter_value


def accepts_token(machine, tools, text):
    match advance(machine, tools, text):
        case Success(_machine):
            return True
        case _:
            return False


def accepts(constraint, text):
    """Whether a whole decoded text stays inside the language."""
    return accepts_token(start(constraint.tools), constraint.tools, text)


def decode(constraint, input_ids):
    """A token history as text, through the vocabulary."""
    return join(mapList(input_ids, lambda id: token_at(constraint, id)), "")


def token_at(constraint, i):
    return unwrap_or(listGet(constraint.vocabulary, i), "")


def record(constraint, text):
    """The machine a decoded history reaches; a history outside the language keeps
    the free machine, so masking never claims a constraint it cannot prove."""
    match advance(start(constraint.tools), constraint.tools, text):
        case Success(value=machine):
            return machine
        case _:
            return start(constraint.tools)


# --- the incremental acceptor --------------------------------------------

def advance(machine, tools, text):
    """Feed `text` to `machine`, rejecting the first byte the language does not allow.

    One byte costs one `fold` step and a bounded stage loop, never a stack frame per
    byte: a 64K-token turn would otherwise exhaust the stack before the sampler did.
    The request's tool list is a parameter here and not part of the machine, because a
    record is copied at every byte."""
    return fold(range(len(text)), Success(value=machine), lambda scanned, i: scan_byte(scanned, tools, text, i))


def scan_byte(scanned, tools, text, i):
    """One byte of `text`, or the rejection that already ended the scan."""
    match scanned:
        case Success(value=machine):
            return step_byte(machine, tools, unwrap_or(byteAt(text, i), -1))
        case Error(_rejected):
            return scanned


def step_byte(machine, tools, byte):
    """One byte through the machine's current mode."""
    if machine.mode == "free":
        return Success(value=free_byte(machine, byte))
    else:
        return try_stage(machine, tools, byte, 0)


def try_stage(machine, tools, byte, attempt):
    """Run one stage; a stage that finishes without taking the byte hands it to the next."""
    if attempt >= STAGE_ATTEMPTS:
        return Error(message=ToolSchema.violation("constrain", machine.stage, "no stage accepts this byte"))
    else:
        return staged(tools, byte, attempt, step_call(machine, tools, byte))


def staged(tools, byte, attempt, outcome):
    match outcome:
        case Success(value=consumed):
            return Success(value=consumed.machine) if consumed.consumed else try_stage(consumed.machine, tools, byte, attempt + 1)
        case Error(message=message):
            return Error(message=message)


def free_byte(machine, byte):
    """Outside a call every byte is free; a completed `<tool_call>` opens the grammar."""
    if byte == of_char(CALL_OPEN, machine.progress):
        return opened(machine)
    else:
        return update(machine, progress=restart(CALL_OPEN, byte))


def reset(machine, mode):
    """The machine with no call open: `mode` ("call" or "free") and a fresh
    grammar, every per-call field cleared."""
    return update(machine, mode=mode, stage="fn", progress=0, name="", param="", value="", candidates=[], seen=[], stack=[], anchor=JsonNull, params=[], required=[])


def opened(machine):
    """`<tool_call>` completed: arm the call grammar, else record the progress."""
    if machine.progress + 1 == len(CALL_OPEN):
        return reset(machine, "call")
    else:
        return update(machine, progress=machine.progress + 1)


def restart(literal, byte):
    """Progress after a byte that did not match: restart at a fresh first byte."""
    if byte == of_char(literal, 0):
        return 1
    else:
        return 0


# --- the call grammar -----------------------------------------------------

def step_call(machine, tools, byte):
    """One byte of the call grammar, as a consumed-or-not outcome."""
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
        case "pnopen":
            return strict(machine, byte, PARAMETER_OPEN, "pname")
        case "pname":
            return parameter_name_byte(machine, byte)
        case "sep":
            return separator_byte(machine, byte)
        case "valuestart":
            return value_start_byte(machine, byte)
        case "raw":
            return raw_byte(machine, byte)
        case "penum":
            return prefix_byte(machine, byte)
        case "json":
            return json_byte(machine, byte)
        case "ptail":
            return closed_parameter(machine, byte)
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
        return Success(value=Outcome(machine=machine, consumed=True))
    elif byte == of_char(literal, machine.progress):
        return literal_matched(machine, literal, next_stage)
    else:
        return Error(message=ToolSchema.violation("constrain", machine.stage, f"expected {literal}"))


def literal_matched(machine, literal, next_stage):
    """One literal byte matched: finish the literal or record the progress."""
    if machine.progress + 1 == len(literal):
        return Success(value=Outcome(machine=update(machine, stage=next_stage, progress=0), consumed=True))
    else:
        return Success(value=Outcome(machine=update(machine, progress=machine.progress + 1), consumed=True))


def name_byte(machine, tools, byte):
    """A function name: a prefix of a declared name, then its `>`, which binds the
    tool and keeps its parameter schema in the machine."""
    if byte == of_char(">", 0) and declares_tool(machine, machine.name):
        anchor = anchor_of(tools, machine.name)
        bound = update(machine, stage="params", progress=0, anchor=anchor, params=ToolSchema.property_names(anchor), required=ToolSchema.required_names(anchor))
        return Success(value=Outcome(machine=bound, consumed=True))
    else:
        return extended_name(machine, byte)


def extended_name(machine, byte):
    """Extend the name with a byte that keeps it a prefix of some declared name."""
    longer = machine.name + char_of(byte)
    if len(filterList(machine.names, lambda declared: startsWith(declared, longer))) > 0:
        return Success(value=Outcome(machine=update(machine, name=longer), consumed=True))
    else:
        return Error(message=ToolSchema.violation("constrain", "name", "undeclared function name"))


def tool_names(tools):
    return mapList(ToolSchema.tool_list(tools), lambda tool: ToolSchema.tool_name(tool))


def anchor_of(tools, name):
    """The chosen tool's `parameters` schema, kept in the machine after the name."""
    return ToolSchema.parameters(ToolSchema.tool_named(tools, name))


def declares_tool(machine, name):
    return len(filterList(machine.names, lambda declared: declared == name)) > 0


def params_byte(machine, byte):
    """After a name's `>`: whitespace, then `<parameter=` or `</function>`."""
    if is_space(byte):
        return Success(value=Outcome(machine=machine, consumed=True))
    elif byte == of_char("<", 0):
        return Success(value=Outcome(machine=update(machine, stage="tag", progress=1), consumed=True))
    else:
        return Error(message=ToolSchema.violation("constrain", "params", "expected <"))


def tag_byte(machine, byte):
    """After `<`: `parameter=` opens a parameter, `/function>` closes the call. A
    parameter may only open while the tool still has a property left to write, so the
    grammar cannot reach a name it has nothing to spell."""
    if byte == of_char("p", 0) and len(unwritten_parameters(machine)) > 0:
        return Success(value=Outcome(machine=update(machine, stage="pnopen", progress=2), consumed=True))
    elif byte == of_char("p", 0):
        return Error(message=ToolSchema.violation("constrain", "params", "every parameter is already written"))
    elif byte == of_char("/", 0) and every_required_written(machine):
        return Success(value=Outcome(machine=update(machine, stage="fnclose", progress=2), consumed=True))
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
        return Success(value=Outcome(machine=update(written, params=without(machine.params, machine.param)), consumed=True))
    else:
        return extended_parameter(machine, byte)


def extended_parameter(machine, byte):
    """Extend the parameter name with a byte that keeps it a prefix of one still unwritten."""
    longer = machine.param + char_of(byte)
    if len(filterList(unwritten_parameters(machine), lambda declared: startsWith(declared, longer))) > 0:
        return Success(value=Outcome(machine=update(machine, param=longer), consumed=True))
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
        return Success(value=Outcome(machine=update(machine, stage="valuestart", progress=0), consumed=True))
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
        return Success(value=Outcome(machine=update(machine, progress=restart(RAW_CLOSE, byte)), consumed=True))


def matched_close(machine):
    """One byte of the closing sequence matched, or the parameter closing."""
    if machine.progress + 1 == len(RAW_CLOSE):
        return Success(value=Outcome(machine=update(machine, stage="params", progress=0, param="", value=""), consumed=True))
    else:
        return Success(value=Outcome(machine=update(machine, progress=machine.progress + 1), consumed=True))


def prefix_byte(machine, byte):
    """An enum member or literal: extend it, or finish when it is complete."""
    longer = machine.value + char_of(byte)
    if len(filterList(machine.candidates, lambda candidate: startsWith(candidate, longer))) > 0:
        return Success(value=Outcome(machine=update(machine, value=longer), consumed=True))
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
