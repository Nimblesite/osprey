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
from qwen import ToolCallGrammar

__all__ = ["start", "advance", "complete", "Constrain", "apply", "mask", "accepts", "decode"]


CALL_OPEN = "<tool_call>"
STAGE_ATTEMPTS = 4

type Machine = ToolCallGrammar.Machine


class Constraint:
    """A masking processor: the tool schemas, the vocabulary and the filter value."""
    tools: Json
    vocabulary: list[str]
    filter_value: float32


# --- construction ---------------------------------------------------------

def start(tools) -> Machine:
    """A machine at the start of a turn: free text, no call open."""
    return ToolCallGrammar.Machine(names=ToolCallGrammar.tool_names(tools), anchor=JsonNull, params=[], required=[], mode="free", stage="fn", progress=0, name="", param="", value="", candidates=[], seen=[], stack=[], finished=False)


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
        return staged(tools, byte, attempt, ToolCallGrammar.step_call(machine, tools, byte))


def staged(tools, byte, attempt, outcome):
    match outcome:
        case Success(value=consumed):
            return Success(value=consumed.machine) if consumed.consumed else try_stage(consumed.machine, tools, byte, attempt + 1)
        case Error(message=message):
            return Error(message=message)


def free_byte(machine, byte):
    """Outside a call every byte is free; a completed `<tool_call>` opens the grammar."""
    if byte == ToolCallGrammar.of_char(CALL_OPEN, machine.progress):
        return opened(machine)
    else:
        return update(machine, progress=ToolCallGrammar.restart(CALL_OPEN, byte))


def opened(machine):
    """`<tool_call>` completed: arm the call grammar, else record the progress."""
    if machine.progress + 1 == len(CALL_OPEN):
        return ToolCallGrammar.reset(machine, "call")
    else:
        return update(machine, progress=machine.progress + 1)
