# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=ToolJsonFrame
"""One JSON frame's reaction to one byte, and the number scanners beneath it.

Implements [LLM-TOOL-CONSTRAIN] of docs/specs/applications/LocalCodingModel.md.
"""

from qwen import ToolSchema

__all__ = ["Frame", "ScanStep", "of_char", "step", "is_space", "key_candidates", "required_met", "first_missing"]


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
        case "colon":
            return colon_step(frame, byte)
        case _:
            return inner_step(frame, byte)


def inner_step(frame, byte):
    """One byte after a value's first: an enum or literal member, a container's
    separator or closer, or a number's next part."""
    match frame.stage:
        case "enum":
            return prefix_step(frame, byte)
        case "lit":
            return prefix_step(frame, byte)
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


def bare_start(frame, byte, candidates):
    """A literal scanner seeded with the spellings the value must match."""
    return literal_start(update(frame, stage="lit", text="", candidates=candidates), byte)


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
        case _:
            return keyword_start(frame, byte, declared)


def keyword_start(frame, byte, declared):
    """A declared type whose values are fixed words: `null` or a boolean."""
    match declared:
        case "null":
            return bare_start(frame, byte, ["null"])
        case "boolean":
            return bare_start(frame, byte, ["true", "false"])
        case _:
            return inferred_start(frame, byte)


def inferred_start(frame, byte):
    """An unconstrained value: the byte picks the scanner, as JSON's grammar reads it."""
    if byte == of_char('"'):
        return quoted_start(frame, byte)
    elif byte == of_char("t") or byte == of_char("f"):
        return bare_start(frame, byte, ["true", "false"])
    elif byte == of_char("n"):
        return bare_start(frame, byte, ["null"])
    elif byte == of_char("{"):
        return container_start(frame, byte, "{", "objFirst", "object")
    elif byte == of_char("["):
        return container_start(frame, byte, "[", "arrFirst", "array")
    else:
        return number_start(frame, byte)


def quoted_start(frame, byte):
    """A JSON string value's opening quote."""
    if byte == of_char('"'):
        return advanced(update(frame, stage="str", scan="content"))
    else:
        return Error(message=ToolSchema.violation("json", "string", "expected a quote"))


def literal_start(frame, byte):
    """An enum member or literal: match its first byte."""
    return prefix_step(frame, byte)


def container_start(frame, byte, opener, stage, container):
    """An object or array value: its opening brace, then its elements."""
    if byte == of_char(opener):
        opened = update(frame, stage=stage, container=container, child=ToolSchema.items_schema(frame.schema))
        return kept(opened)
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


def advanced(updated):
    """The turn after consuming this byte into the `updated` frame."""
    return Success(value=Turn(frame=updated, consumed=True, done=False))


def staged(frame, stage):
    """The turn after consuming this byte into the frame's next `stage`."""
    return advanced(update(frame, stage=stage))


def kept(frame):
    """The turn after consuming this byte without moving the frame."""
    return Success(value=Turn(frame=frame, consumed=True, done=False))


def accepted(frame):
    """The turn where the value ends: the next byte belongs to the document."""
    return Success(value=Turn(frame=frame, consumed=False, done=True))


def closed(frame):
    """The turn where the document ends at this byte."""
    return Success(value=Turn(frame=frame, consumed=True, done=True))


def content_step(frame, byte, done_stage):
    """String content: a quote ends it, a backslash escapes, a control byte is invalid."""
    if byte == of_char('"'):
        return Success(value=Turn(frame=update(frame, stage=done_stage), consumed=True, done=done_stage == "done"))
    elif byte == of_char("\\"):
        return advanced(update(frame, scan="esc"))
    elif byte < 32:
        return Error(message=ToolSchema.violation("json", "string", "unescaped control byte"))
    else:
        return kept(frame)


def escape_step(frame, byte):
    """The character after a backslash."""
    if is_escape_char(byte):
        return advanced(update(frame, scan="content"))
    elif byte == of_char("u"):
        return advanced(update(frame, scan="uni", progress=0))
    else:
        return Error(message=ToolSchema.violation("json", "string", "invalid escape"))


def unicode_step(frame, byte):
    """The four hex digits of a `\\u` escape."""
    if is_hex(byte) == False:
        return Error(message=ToolSchema.violation("json", "string", "invalid \\u escape"))
    elif frame.progress + 1 == 4:
        return advanced(update(frame, scan="content", progress=0))
    else:
        return advanced(update(frame, progress=frame.progress + 1))


def may_write_key(frame):
    """Whether one more key may be written: a declared name is left, or any name is allowed."""
    return ToolSchema.allows_extra(frame.schema) or len(key_candidates(frame)) > 0


def key_candidates(frame):
    """The property names a key may still name: the declared ones not written yet."""
    declared = ToolSchema.property_names(frame.schema)
    return filterList(declared, lambda name: len(filterList(frame.seen, lambda written: written == name)) == 0)


def key_step(frame, byte):
    """A key's text: only a declared, unwritten property name, closed by its quote. An
    object that admits undeclared properties takes any key instead."""
    if ToolSchema.allows_extra(frame.schema):
        return free_key_step(frame, byte)
    else:
        return declared_key_step(frame, byte)


def free_key_step(frame, byte):
    """A key of an object that admits undeclared properties: any text, closed by a quote."""
    if byte == of_char('"'):
        return staged(frame, "colon")
    elif byte < 32:
        return Error(message=ToolSchema.violation("json", "key", "unescaped control byte"))
    else:
        return advanced(update(frame, text=frame.text + char_of(byte)))


def declared_key_step(frame, byte):
    """A key confined to the object's own declared, unwritten property names."""
    longer = frame.text + char_of(byte)
    if len(filterList(frame.candidates, lambda candidate: startsWith(candidate, longer))) > 0:
        return advanced(update(frame, text=longer))
    elif byte == of_char('"') and len(filterList(frame.candidates, lambda candidate: candidate == frame.text)) > 0:
        return staged(frame, "colon")
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
        return advanced(update(frame, stage="value", child=declared, seen=listAppend(frame.seen, frame.text)))
    elif ToolSchema.allows_extra(frame.schema):
        return advanced(update(frame, stage="value", child=JsonNull, seen=listAppend(frame.seen, frame.text)))
    else:
        return Error(message=ToolSchema.violation("json", "object", f"undeclared property {frame.text}"))


def after_step(frame, byte):
    """After a value: a comma continues the container, a closer finishes it."""
    if byte == of_char(",") and frame.container == "array":
        return staged(frame, "value")
    elif byte == of_char(",") and frame.container == "object" and may_write_key(frame):
        return staged(frame, "keyNext")
    elif byte == of_char(",") and frame.container == "object":
        return Error(message=ToolSchema.violation("json", "object", "no property left to write"))
    elif byte == of_char("]") and frame.container == "array":
        return closed(frame)
    elif byte == of_char("}") and frame.container == "object" and required_met(frame):
        return closed(frame)
    elif byte == of_char("}") and frame.container == "object":
        return Error(message=ToolSchema.violation("json", "object", f"missing {first_missing(frame)}"))
    else:
        return Error(message=ToolSchema.violation("json", frame.container, "expected , or a closer"))


def prefix_step(frame, byte):
    """Accept a byte that extends an accepted text; finish when the text is one and the
    byte belongs to none."""
    extended = filterList(frame.candidates, lambda candidate: startsWith(candidate, frame.text + char_of(byte)))
    if len(extended) > 0:
        return advanced(update(frame, text=frame.text + char_of(byte), progress=frame.progress + 1))
    elif len(filterList(frame.candidates, lambda candidate: candidate == frame.text)) > 0:
        return accepted(frame)
    else:
        return Error(message=ToolSchema.violation("json", "literal", "not an accepted member"))


def char_of(byte):
    """A byte as text, for comparing against ASCII literals."""
    return unwrap_or(fromCodePoint(byte), "")


# --- numbers --------------------------------------------------------------

def number_start(frame, byte):
    """The first byte of a JSON number: a sign, a zero or a leading digit."""
    if byte == of_char("-"):
        return staged(frame, "numSign")
    else:
        return number_digit(frame, byte)


def number_digit(frame, byte):
    """A digit after the sign: a leading zero may not be followed by another digit."""
    if byte == of_char("0"):
        return staged(frame, "numZero")
    elif is_digit(byte):
        return staged(frame, "numInt")
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected a digit"))


def number_step(frame, byte):
    """The number's sign and integer part, or its fraction and exponent once begun."""
    match frame.stage:
        case "numSign":
            return number_digit(frame, byte)
        case "numZero":
            return zero_step(frame, byte)
        case "numInt":
            return int_step(frame, byte)
        case _:
            return number_tail_step(frame, byte)


def number_tail_step(frame, byte):
    """The fraction, the exponent, and where the number ends."""
    match frame.stage:
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
        return kept(frame)
    else:
        return frac_or_exp(frame, byte)


def frac_or_exp(frame, byte):
    """A fraction, an exponent, or the end of the number."""
    fractional = byte == of_char(".") and ToolSchema.kind(frame.schema) != "integer"
    exponential = is_exponent_marker(byte) and ToolSchema.kind(frame.schema) != "integer"
    if fractional:
        return staged(frame, "numDot")
    elif exponential:
        return staged(frame, "numExp")
    else:
        return accepted(frame)


def digit_required(frame, byte, stage, complaint):
    """A number stage whose next byte must be a digit."""
    if is_digit(byte):
        return staged(frame, stage)
    else:
        return Error(message=ToolSchema.violation("json", "number", complaint))


def dot_step(frame, byte):
    """The first fractional digit."""
    return digit_required(frame, byte, "numFrac", "expected a fractional digit")


def frac_step(frame, byte):
    """Fractional digits, an exponent, or the end of the number."""
    if is_digit(byte):
        return kept(frame)
    elif is_exponent_marker(byte):
        return staged(frame, "numExp")
    else:
        return accepted(frame)


def exp_step(frame, byte):
    """The exponent's sign or first digit."""
    if byte == of_char("+") or byte == of_char("-"):
        return staged(frame, "numExpSign")
    elif is_digit(byte):
        return staged(frame, "numExpDigit")
    else:
        return Error(message=ToolSchema.violation("json", "number", "expected an exponent"))


def exp_sign_step(frame, byte):
    """The exponent's first digit after its sign."""
    return digit_required(frame, byte, "numExpDigit", "expected an exponent digit")


def exp_digit_step(frame, byte):
    """Exponent digits, or the end of the number."""
    if is_digit(byte):
        return kept(frame)
    else:
        return accepted(frame)


def is_space(byte):
    """Whether `byte` is one of JSON's four whitespace bytes."""
    return byte == of_char(" ") or byte == of_char("\t") or byte == of_char("\n") or byte == of_char("\r")


def is_exponent_marker(byte):
    """The `e` or `E` that opens a number's exponent."""
    return byte == of_char("e") or byte == of_char("E")


def is_digit(byte):
    return byte >= 48 and byte <= 57


def is_hex(byte):
    return is_digit(byte) or (byte >= 97 and byte <= 102) or (byte >= 65 and byte <= 70)


def is_escape_char(byte):
    return contains("\"\\/bfnrt", char_of(byte))


# --- the object's required properties -------------------------------------


def unmet(frame):
    """The properties the schema requires that the object has not written yet."""
    return filterList(ToolSchema.required_names(frame.schema), lambda name: len(filterList(frame.seen, lambda written: written == name)) == 0)


def required_met(frame):
    """Whether every property the schema requires has already been written."""
    return len(unmet(frame)) == 0


def first_missing(frame):
    """The first required property the object has not written, or `""` when every one is."""
    return unwrap_or(listGet(unmet(frame), 0), "")
