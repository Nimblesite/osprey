# osprey: flavor=python
# osprey: namespace=transformers
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

from transformers import ToolSchema

__all__ = ["Frame", "ScanStep", "begin", "feed", "finished"]


class Frame:
    """One open JSON value: its schema, how far its scan has got, what it accepts,
    and the key or literal text accumulated so far."""
    schema: Json
    stage: str
    container: str
    scan: str
    progress: int
    text: str
    candidates: list[str]
    seen: list[str]
    child: Json


class ScanStep:
    """The result of one byte: the new stack, and whether the byte was consumed."""
    stack: list[Frame]
    consumed: bool


class Turn:
    """One frame's reaction to a byte: the updated frame, and whether it finished."""
    frame: Frame
    consumed: bool
    done: bool


# --- entry points ---------------------------------------------------------

def begin(schema: Json) -> list[Frame]:
    """The stack for one value that must satisfy `schema`."""
    return [value_frame(schema)]


def value_frame(schema):
    """A frame that has not read its value's first byte yet."""
    return Frame(schema=schema, stage="start", container="", scan="", progress=0, text="", candidates=[], seen=[], child=JsonNull)


def feed(stack, byte):
    """Advance one byte through the stack; an empty stack means the value is complete."""
    if len(stack) == 0:
        return Success(value=ScanStep(stack=[], consumed=False))
    elif skippable(last(stack).stage) and is_space(byte):
        return Success(value=ScanStep(stack=stack, consumed=True))
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
        return ending(last(stack))


def ending(frame):
    """Whether the text ended at a stage where the value is complete.

    Only a number can end without a byte of its own: every other value ends with a byte
    that closes it, so an open object or array is incomplete, never complete."""
    if len(filterList(["numZero", "numInt", "numFrac", "numExpDigit"], lambda stage: stage == frame.stage)) > 0:
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
            return framed(stack, step(top, byte))


def array_element(stack, byte):
    """Inside `[`: either the closing `]` or the first element."""
    if byte == of_char("]"):
        return Success(value=pop(parents(stack), True))
    else:
        return feed(listAppend(stack, value_frame(ToolSchema.items_schema(last(stack).schema))), byte)


def object_key(stack, byte, allow_close):
    """Inside `{`: the closing `}` or a quoted key."""
    if byte == of_char("}") and allow_close and required_met(last(stack)):
        return Success(value=pop(parents(stack), True))
    elif byte == of_char("}") and allow_close:
        return Error(message=ToolSchema.violation("json", "object", f"missing {first_missing(last(stack))}"))
    elif byte == of_char('"'):
        return Success(value=ScanStep(stack=replaced(stack, update(last(stack), stage="key", text="", candidates=key_candidates(last(stack)))), consumed=True))
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
        return ScanStep(stack=replaced(stack, turn.frame), consumed=turn.consumed)


def pop(parents, consumed):
    """One value finished: the whole value is done, or its parent moves on to `,` or a closer."""
    if len(parents) == 0:
        return ScanStep(stack=[], consumed=consumed)
    else:
        return ScanStep(stack=replaced(parents, update(last(parents), stage=after_stage(last(parents)))), consumed=consumed)


def required_met(frame):
    """Whether every property the schema requires has already been written."""
    missing = filterList(ToolSchema.required_names(frame.schema), lambda name: len(filterList(frame.seen, lambda written: written == name)) == 0)
    return len(missing) == 0


def first_missing(frame):
    missing = filterList(ToolSchema.required_names(frame.schema), lambda name: len(filterList(frame.seen, lambda written: written == name)) == 0)
    return unwrap_or(listGet(missing, 0), "")


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


def of_char(literal):
    """The UTF-8 byte of a one-character literal."""
    return unwrap_or(byteAt(literal, 0), -1)


# --- one frame, one byte --------------------------------------------------

def step(frame, byte):
    """One frame's reaction to one byte; the frame's `stage` decides how."""
    match frame.stage:
        case "start":
            return start_value(frame, byte)
        case "key":
            return key_step(frame, byte)
        case "str":
            return string_step(frame, byte, "done")
        case "enum":
            return prefix_step(frame, byte)
        case "lit":
            return prefix_step(frame, byte)
        case "colon":
            return colon_step(frame, byte)
        case "afterArray":
            return after_step(frame, byte)
        case "afterObject":
            return after_step(frame, byte)
        case _:
            return number_step(frame, byte)


def start_value(frame, byte):
    """The first byte of a value: the schema's type, or the byte itself, picks the scanner."""
    members = ToolSchema.enum_values(frame.schema)
    if len(members) > 0:
        started = update(frame, stage="enum", text="", candidates=mapList(members, encoded_member))
        return prefix_step(started, byte)
    else:
        return typed_start(frame, byte, ToolSchema.kind(frame.schema))


def encoded_member(member):
    return unwrap_or(jsonEncode(member), "")


def typed_start(frame, byte, declared):
    """Begin the scanner for one declared JSON type."""
    match declared:
        case "string":
            return quoted_start(frame, byte)
        case "integer":
            return number_start(frame, byte)
        case "number":
            return number_start(frame, byte)
        case "object":
            return container_start(frame, byte, "{", "objFirst", "object")
        case "array":
            return container_start(frame, byte, "[", "arrFirst", "array")
        case "null":
            return literal_start(update(frame, stage="lit", text="", candidates=["null"]), byte)
        case "boolean":
            return literal_start(update(frame, stage="lit", text="", candidates=["true", "false"]), byte)
        case _:
            return inferred_start(frame, byte)


def inferred_start(frame, byte):
    """An unconstrained value: the byte picks the scanner, as JSON's grammar reads it."""
    if byte == of_char('"'):
        return quoted_start(frame, byte)
    elif byte == of_char("t") or byte == of_char("f"):
        return literal_start(update(frame, stage="lit", text="", candidates=["true", "false"]), byte)
    elif byte == of_char("n"):
        return literal_start(update(frame, stage="lit", text="", candidates=["null"]), byte)
    elif byte == of_char("{"):
        return container_start(frame, byte, "{", "objFirst", "object")
    elif byte == of_char("["):
        return container_start(frame, byte, "[", "arrFirst", "array")
    else:
        return number_start(frame, byte)


def quoted_start(frame, byte):
    """A JSON string value's opening quote."""
    if byte == of_char('"'):
        return Success(value=Turn(frame=update(frame, stage="str", scan="content"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "string", "expected a quote"))


def literal_start(frame, byte):
    """An enum member or literal: match its first byte."""
    return prefix_step(frame, byte)


def container_start(frame, byte, opener, stage, container):
    """An object or array value: its opening brace, then its elements."""
    if byte == of_char(opener):
        opened = update(frame, stage=stage, container=container, child=ToolSchema.items_schema(frame.schema))
        return Success(value=Turn(frame=opened, consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", container, f"expected {opener}"))


def string_step(frame, byte, done_stage):
    """A JSON string's content, escapes and four-digit escapes."""
    match frame.scan:
        case "esc":
            return escape_step(frame, byte)
        case "uni":
            return unicode_step(frame, byte)
        case _:
            return content_step(frame, byte, done_stage)


def content_step(frame, byte, done_stage):
    """String content: a quote ends it, a backslash escapes, a control byte is invalid."""
    if byte == of_char('"'):
        return Success(value=Turn(frame=update(frame, stage=done_stage), consumed=True, done=done_stage == "done"))
    elif byte == of_char("\\"):
        return Success(value=Turn(frame=update(frame, scan="esc"), consumed=True, done=False))
    elif byte < 32:
        return Error(message=ToolSchema.violation("json", "string", "unescaped control byte"))
    else:
        return Success(value=Turn(frame=frame, consumed=True, done=False))


def escape_step(frame, byte):
    """The character after a backslash."""
    if is_escape_char(byte):
        return Success(value=Turn(frame=update(frame, scan="content"), consumed=True, done=False))
    elif byte == of_char("u"):
        return Success(value=Turn(frame=update(frame, scan="uni", progress=0), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "string", "invalid escape"))


def unicode_step(frame, byte):
    """The four hex digits of a `\\u` escape."""
    if is_hex(byte) == False:
        return Error(message=ToolSchema.violation("json", "string", "invalid \\u escape"))
    elif frame.progress + 1 == 4:
        return Success(value=Turn(frame=update(frame, scan="content", progress=0), consumed=True, done=False))
    else:
        return Success(value=Turn(frame=update(frame, progress=frame.progress + 1), consumed=True, done=False))


def key_candidates(frame):
    """The property names a key may still name: the declared ones not written yet."""
    declared = ToolSchema.property_names(frame.schema)
    return filterList(declared, lambda name: len(filterList(frame.seen, lambda written: written == name)) == 0)


def key_step(frame, byte):
    """A key's text: only a declared, unwritten property name, closed by its quote."""
    longer = frame.text + char_of(byte)
    if len(filterList(frame.candidates, lambda candidate: startsWith(candidate, longer))) > 0:
        return Success(value=Turn(frame=update(frame, text=longer), consumed=True, done=False))
    elif byte == of_char('"') and len(filterList(frame.candidates, lambda candidate: candidate == frame.text)) > 0:
        return Success(value=Turn(frame=update(frame, stage="colon"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "key", "not a declared, unwritten property"))


def colon_step(frame, byte):
    """The colon after a key, which resolves the property's schema."""
    if byte != of_char(":"):
        return Error(message=ToolSchema.violation("json", "object", "expected :"))
    elif len(filterList(frame.seen, lambda name: name == frame.text)) > 0:
        return Error(message=ToolSchema.violation("json", "object", f"repeated key {frame.text}"))
    else:
        return declared_property(frame)


def declared_property(frame):
    """The value schema for the key just read, or a rejection when the schema forbids it."""
    declared = ToolSchema.property_schema(frame.schema, frame.text)
    if ToolSchema.kind_matches("null", declared) == False:
        return Success(value=Turn(frame=update(frame, stage="value", child=declared, seen=listAppend(frame.seen, frame.text)), consumed=True, done=False))
    elif ToolSchema.allows_extra(frame.schema):
        return Success(value=Turn(frame=update(frame, stage="value", child=JsonNull, seen=listAppend(frame.seen, frame.text)), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "object", f"undeclared property {frame.text}"))


def after_step(frame, byte):
    """After a value: a comma continues the container, a closer finishes it."""
    if byte == of_char(",") and frame.container == "array":
        return Success(value=Turn(frame=update(frame, stage="value"), consumed=True, done=False))
    elif byte == of_char(",") and frame.container == "object" and len(key_candidates(frame)) > 0:
        return Success(value=Turn(frame=update(frame, stage="keyNext"), consumed=True, done=False))
    elif byte == of_char(",") and frame.container == "object":
        return Error(message=ToolSchema.violation("json", "object", "no property left to write"))
    elif byte == of_char("]") and frame.container == "array":
        return Success(value=Turn(frame=frame, consumed=True, done=True))
    elif byte == of_char("}") and frame.container == "object" and required_met(frame):
        return Success(value=Turn(frame=frame, consumed=True, done=True))
    elif byte == of_char("}") and frame.container == "object":
        return Error(message=ToolSchema.violation("json", "object", f"missing {first_missing(frame)}"))
    else:
        return Error(message=ToolSchema.violation("json", frame.container, "expected , or a closer"))


def prefix_step(frame, byte):
    """Accept a byte that extends an accepted text; finish when the text is one and the
    byte belongs to none."""
    extended = filterList(frame.candidates, lambda candidate: startsWith(candidate, frame.text + char_of(byte)))
    if len(extended) > 0:
        return Success(value=Turn(frame=update(frame, text=frame.text + char_of(byte), progress=frame.progress + 1), consumed=True, done=False))
    elif len(filterList(frame.candidates, lambda candidate: candidate == frame.text)) > 0:
        return Success(value=Turn(frame=frame, consumed=False, done=True))
    else:
        return Error(message=ToolSchema.violation("json", "literal", "not an accepted member"))


def char_of(byte):
    """A byte as text, for comparing against ASCII literals."""
    return unwrap_or(fromCodePoint(byte), "")


# --- numbers --------------------------------------------------------------

def number_start(frame, byte):
    """The first byte of a JSON number: a sign, a zero or a leading digit."""
    if byte == of_char("-"):
        return Success(value=Turn(frame=update(frame, stage="numSign"), consumed=True, done=False))
    else:
        return number_digit(frame, byte)


def number_digit(frame, byte):
    """A digit after the sign: a leading zero may not be followed by another digit."""
    if byte == of_char("0"):
        return Success(value=Turn(frame=update(frame, stage="numZero"), consumed=True, done=False))
    elif is_digit(byte):
        return Success(value=Turn(frame=update(frame, stage="numInt"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected a digit"))


def number_step(frame, byte):
    """The number's integer, fractional and exponent parts, and where a number ends."""
    match frame.stage:
        case "numSign":
            return number_digit(frame, byte)
        case "numZero":
            return zero_step(frame, byte)
        case "numInt":
            return int_step(frame, byte)
        case "numDot":
            return dot_step(frame, byte)
        case "numFrac":
            return frac_step(frame, byte)
        case "numExp":
            return exp_step(frame, byte)
        case "numExpSign":
            return exp_sign_step(frame, byte)
        case _:
            return exp_digit_step(frame, byte)


def zero_step(frame, byte):
    """After a leading zero only a fraction, an exponent or the value's end may come."""
    if is_digit(byte):
        return Error(message=ToolSchema.violation("json", "number", "leading zero"))
    else:
        return frac_or_exp(frame, byte)


def int_step(frame, byte):
    """Inside the integer part."""
    if is_digit(byte):
        return Success(value=Turn(frame=frame, consumed=True, done=False))
    else:
        return frac_or_exp(frame, byte)


def frac_or_exp(frame, byte):
    """A fraction, an exponent, or the end of the number."""
    fractional = byte == of_char(".") and ToolSchema.kind(frame.schema) != "integer"
    exponential = (byte == of_char("e") or byte == of_char("E")) and ToolSchema.kind(frame.schema) != "integer"
    if fractional:
        return Success(value=Turn(frame=update(frame, stage="numDot"), consumed=True, done=False))
    elif exponential:
        return Success(value=Turn(frame=update(frame, stage="numExp"), consumed=True, done=False))
    else:
        return Success(value=Turn(frame=frame, consumed=False, done=True))


def dot_step(frame, byte):
    """The first fractional digit."""
    if is_digit(byte):
        return Success(value=Turn(frame=update(frame, stage="numFrac"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected a fractional digit"))


def frac_step(frame, byte):
    """Fractional digits, an exponent, or the end of the number."""
    if is_digit(byte):
        return Success(value=Turn(frame=frame, consumed=True, done=False))
    elif byte == of_char("e") or byte == of_char("E"):
        return Success(value=Turn(frame=update(frame, stage="numExp"), consumed=True, done=False))
    else:
        return Success(value=Turn(frame=frame, consumed=False, done=True))


def exp_step(frame, byte):
    """The exponent's sign or first digit."""
    if byte == of_char("+") or byte == of_char("-"):
        return Success(value=Turn(frame=update(frame, stage="numExpSign"), consumed=True, done=False))
    elif is_digit(byte):
        return Success(value=Turn(frame=update(frame, stage="numExpDigit"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected an exponent"))


def exp_sign_step(frame, byte):
    """The exponent's first digit after its sign."""
    if is_digit(byte):
        return Success(value=Turn(frame=update(frame, stage="numExpDigit"), consumed=True, done=False))
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected an exponent digit"))


def exp_digit_step(frame, byte):
    """Exponent digits, or the end of the number."""
    if is_digit(byte):
        return Success(value=Turn(frame=frame, consumed=True, done=False))
    else:
        return Success(value=Turn(frame=frame, consumed=False, done=True))


def is_space(byte):
    return byte == of_char(" ") or byte == of_char("\t") or byte == of_char("\n") or byte == of_char("\r")


def is_digit(byte):
    return byte >= 48 and byte <= 57


def is_hex(byte):
    return is_digit(byte) or (byte >= 97 and byte <= 102) or (byte >= 65 and byte <= 70)


def is_escape_char(byte):
    return contains("\"\\/bfnrt", char_of(byte))
