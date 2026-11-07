# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=ToolJsonScan
"""An incremental JSON scanner that accepts only values a schema allows.

`ToolConstrain` needs one question answered per candidate token: does this text
leave the argument a prefix of a value the parameter's schema accepts? This
module answers it a byte at a time, so a token costs its own length and not the
length of the history. `begin` opens one value's frame; `feed` takes one byte and
returns the new stack with whether the byte was consumed, because a JSON number,
literal and enum member end only when the next byte is not theirs.

The scanner is schema-directed rather than validate-after-the-fact: an object
frame admits only its declared property names, an `enum` frame admits only the
`jsonEncode` texts of its members, and a container pushes its `items` or property
schema for the nested value. A masked sample therefore cannot produce
`{"mode": "delete"}` where the schema declares `enum ["check","apply"]`, which is
the point of [LLM-TOOL-CONSTRAIN].

An empty stack means the value is complete. A rejected byte is a `Result` error,
never a substituted value.
"""

from qwen import ToolJsonFrame
from qwen import ToolSchema

__all__ = ["Frame", "ScanStep", "begin", "feed", "finished"]

type Frame = ToolJsonFrame.Frame
"""One open JSON value: `ToolJsonFrame`'s record, re-exported under this module's name."""
type ScanStep = ToolJsonFrame.ScanStep
"""One fed byte's result: `ToolJsonFrame`'s record, re-exported under this module's name."""


# --- entry points ---------------------------------------------------------

def begin(schema: Json) -> list[Frame]:
    """The stack for one value that must satisfy `schema`."""
    return [value_frame(schema)]


def value_frame(schema):
    """A frame that has not read its value's first byte yet."""
    return ToolJsonFrame.Frame(schema=schema, stage="start", container="", scan="", progress=0, text="", candidates=[], seen=[], child=JsonNull)


def feed(stack, byte):
    """Advance one byte through the stack; an empty stack means the value is complete."""
    if len(stack) == 0:
        return Success(value=ToolJsonFrame.ScanStep(stack=[], consumed=False))
    elif skippable(last(stack).stage) and ToolJsonFrame.is_space(byte):
        return Success(value=ToolJsonFrame.ScanStep(stack=stack, consumed=True))
    else:
        return enter(stack, byte)


def skippable(stage):
    """Whether JSON allows whitespace before this stage's next byte; a string's content
    is not skippable, because a space there is part of the value."""
    return stage != "str" and stage != "key" and starts_with_scan(stage)


def starts_with_scan(stage):
    return contains(" start value arrFirst objFirst keyNext colon afterArray afterObject ", f" {stage} ")


def finished(stack):
    """Whether the text ended with `stack` holding one complete value.

    A number, literal or enum member completes at the byte after it, so at the end of
    the text the stack itself has to say whether the value is complete."""
    if len(stack) == 0:
        return Success(value=None)
    else:
        return ending(last(stack), len(stack) == 1)


def ending(frame, outermost):
    """Whether the text ended at a stage where the value is complete.

    A top-level number or matched literal can end at EOF. A nested value still
    needs every enclosing container's closing byte."""
    number = listContains(["numZero", "numInt", "numFrac", "numExpDigit"], frame.stage)
    literal = (frame.stage == "lit" or frame.stage == "enum") and listContains(frame.candidates, frame.text)
    if outermost and (number or literal):
        return Success(value=None)
    else:
        return Error(message=ToolSchema.violation("json", frame.stage, "the value is incomplete"))


def enter(stack, byte):
    """Dispatch on the top frame's stage, pushing a child value where the grammar needs one."""
    top = last(stack)
    match top.stage:
        case "value":
            return feed(pushed(stack, top.child), byte)
        case "arrFirst":
            return array_element(stack, byte)
        case "objFirst":
            return object_key(stack, byte, True)
        case "keyNext":
            return object_key(stack, byte, False)
        case _:
            return framed(stack, ToolJsonFrame.step(top, byte))


def array_element(stack, byte):
    """Inside `[`: either the closing `]` or the first element."""
    if byte == ToolJsonFrame.of_char("]"):
        return Success(value=pop(parents(stack), True))
    else:
        return feed(listAppend(stack, value_frame(ToolSchema.items_schema(last(stack).schema))), byte)


def object_key(stack, byte, allow_close):
    """Inside `{`: the closing `}` or a quoted key."""
    if byte == ToolJsonFrame.of_char("}") and allow_close and ToolJsonFrame.required_met(last(stack)):
        return Success(value=pop(parents(stack), True))
    elif byte == ToolJsonFrame.of_char("}") and allow_close:
        return Error(message=ToolSchema.violation("json", "object", f"missing {ToolJsonFrame.first_missing(last(stack))}"))
    elif byte == ToolJsonFrame.of_char('"'):
        return Success(value=ToolJsonFrame.ScanStep(stack=replaced(stack, update(last(stack), stage="key", text="", candidates=ToolJsonFrame.key_candidates(last(stack)))), consumed=True))
    else:
        return Error(message=ToolSchema.violation("json", "object", "expected a quoted key"))


def pushed(parents, schema):
    return listAppend(parents, value_frame(schema))


def framed(stack, turn):
    """Apply one frame's reaction to its stack."""
    match turn:
        case Success(value=value):
            return Success(value=settle(stack, value))
        case Error(message=message):
            return Error(message=message)


def settle(stack, turn):
    """Record a frame's reaction: replace it, or pop it when it finished."""
    if turn.done:
        return pop(parents(stack), turn.consumed)
    else:
        return ToolJsonFrame.ScanStep(stack=replaced(stack, turn.frame), consumed=turn.consumed)


def pop(parents, consumed):
    """One value finished: the whole value is done, or its parent moves on to `,` or a closer."""
    if len(parents) == 0:
        return ToolJsonFrame.ScanStep(stack=[], consumed=consumed)
    else:
        return ToolJsonFrame.ScanStep(stack=replaced(parents, update(last(parents), stage=after_stage(last(parents)))), consumed=consumed)


def after_stage(frame):
    return "afterObject" if frame.container == "object" else "afterArray"


def parents(stack):
    """Everything below the top frame."""
    return below(stack, len(stack) - 1)


def below(stack, count):
    """The first `count` frames of `stack`."""
    empty = []
    return fold(range(count), empty, lambda acc, i: listAppend(acc, unwrap_or(listGet(stack, i), last(stack))))


def last(stack):
    return unwrap_or(listGet(stack, len(stack) - 1), value_frame(JsonNull))


def replaced(stack, frame):
    """`stack` with its top frame replaced by `frame`."""
    return listAppend(parents(stack), frame)
