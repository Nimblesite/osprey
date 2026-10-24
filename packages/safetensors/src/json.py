# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=safetensors
# osprey: module=JsonParser
"""A small RFC 8259 JSON parser for the safetensors header.

The Python flavor cannot lower the compiler's `jsonDecode` `Json` union, and the
`jsonParse`/`jsonGet`/`jsonLength` handle API cannot enumerate an object's keys.
The header is a JSON object whose keys are tensor names, so this module parses
it into a tagged record and returns every member in order.
"""

__all__ = ["JsonValue", "JsonMember", "parse"]


class JsonValue:
    """One JSON value: exactly one of the fields is meaningful for `kind`."""
    kind: str
    b: bool
    i: int
    f: float
    s: str
    items: list[JsonValue]
    members: list[JsonMember]


class JsonMember:
    """A named member of a JSON object."""
    name: str
    value: JsonValue


class Parsed:
    """A parsed value and the index just past it."""
    value: JsonValue
    next: int


def jstr(s):
    return JsonValue(kind="str", b=False, i=0, f=0.0, s=s, items=[], members=[])


def jint(i):
    return JsonValue(kind="int", b=False, i=i, f=0.0, s="", items=[], members=[])


def jfloat(f):
    return JsonValue(kind="float", b=False, i=0, f=f, s="", items=[], members=[])


def jbool(b):
    return JsonValue(kind="bool", b=b, i=0, f=0.0, s="", items=[], members=[])


def jnull():
    return JsonValue(kind="null", b=False, i=0, f=0.0, s="", items=[], members=[])


def jarr(items):
    return JsonValue(kind="arr", b=False, i=0, f=0.0, s="", items=items, members=[])


def jobj(members):
    return JsonValue(kind="obj", b=False, i=0, f=0.0, s="", items=[], members=members)


def skip_ws(text, pos) -> int:
    """The first index >= pos that is not a JSON whitespace byte."""
    if pos >= len(text):
        return pos
    else:
        c = unwrap_or(byteAt(text, pos), 0)
        if c == 32 or c == 9 or c == 10 or c == 13:
            return skip_ws(text, pos + 1)
        else:
            return pos


def hex_digit(c):
    """The value of a hex digit, or -1 when `c` is not one."""
    if c >= 48 and c <= 57:
        return c - 48
    elif c >= 97 and c <= 102:
        return c - 87
    elif c >= 65 and c <= 70:
        return c - 55
    else:
        return -1


def hex_value(s):
    """The value of a 4-hex-digit string, or -1 when malformed."""
    if len(s) != 4:
        return -1
    else:
        d0 = hex_digit(unwrap_or(byteAt(s, 0), -1))
        d1 = hex_digit(unwrap_or(byteAt(s, 1), -1))
        d2 = hex_digit(unwrap_or(byteAt(s, 2), -1))
        d3 = hex_digit(unwrap_or(byteAt(s, 3), -1))
        if d0 < 0 or d1 < 0 or d2 < 0 or d3 < 0:
            return -1
        else:
            return d0 * 4096 + d1 * 256 + d2 * 16 + d3


def is_number_char(c):
    """Whether byte `c` can appear inside a JSON number token."""
    return (c >= 48 and c <= 57) or c == 45 or c == 43 or c == 46 or c == 101 or c == 69


def is_float_mark(c):
    """Whether byte `c` marks a JSON float (fraction dot or exponent letter)."""
    return c == 46 or c == 101 or c == 69


def is_float_token(token):
    """Whether the number token holds a fraction or exponent (so it is a float)."""
    return fold(range(len(token)), False, lambda found, i: found or is_float_mark(unwrap_or(byteAt(token, i), 0)))


def number_end(text, pos) -> int:
    """The index just past the number token starting at `pos`."""
    if pos >= len(text):
        return pos
    else:
        c = unwrap_or(byteAt(text, pos), 0)
        if is_number_char(c):
            return number_end(text, pos + 1)
        else:
            return pos


def prefix_at(text, pos, word):
    """Whether `text` has the bytes of `word` starting at `pos`."""
    n = len(word)
    if pos + n > len(text):
        return False
    else:
        ok = unwrap_or(substring(text, pos, pos + n), "") == word
        return ok


def parse_literal(text, pos, word, value):
    """Parse the literal `word` starting at `pos`."""
    if prefix_at(text, pos, word):
        return Success(value=Parsed(value=value, next=pos + len(word)))
    else:
        return Error(message="invalid JSON: bad literal")


def escaped_char(c):
    """The character a simple backslash escape names, or Error for unknown ones."""
    match c:
        case 34:
            return Success(value="\"")
        case 92:
            return Success(value="\\")
        case 47:
            return Success(value="/")
        case 98:
            return Success(value=unwrap_or(fromCodePoint(8), ""))
        case 102:
            return Success(value=unwrap_or(fromCodePoint(12), ""))
        case 110:
            return Success(value=unwrap_or(fromCodePoint(10), ""))
        case 114:
            return Success(value=unwrap_or(fromCodePoint(13), ""))
        case 116:
            return Success(value=unwrap_or(fromCodePoint(9), ""))
        case _:
            return Error(message="invalid JSON: bad escape")


def string_body(text, pos, pieces) -> Result[Parsed, str]:
    """Parse the string body from `pos`, collecting decoded chars in `pieces`."""
    match codePointAt(text, pos):
        case Success(value=value):
            return string_char(text, pos, value, pieces)
        case Error(message=message):
            return Error(message="invalid JSON: unterminated string")


def string_char(text, pos, cp, pieces):
    """Handle one string-body codepoint `cp` at `pos`."""
    if cp == 34:
        return Success(value=Parsed(value=jstr(join(pieces, "")), next=pos + 1))
    elif cp == 92:
        return string_escape(text, pos, pieces)
    else:
        w = unwrap_or(codePointWidth(cp), 1)
        piece = unwrap_or(fromCodePoint(cp), "")
        return string_body(text, pos + w, listAppend(pieces, piece))


def string_escape(text, pos, pieces):
    """Parse the escape whose backslash is at `pos`, appending to `pieces`."""
    match codePointAt(text, pos + 1):
        case Success(value=value):
            return escape_char(text, pos, value, pieces)
        case Error(message=message):
            return Error(message="invalid JSON: unterminated escape")


def escape_char(text, pos, c, pieces):
    """Handle the escape letter `c` (after the backslash at `pos`)."""
    if c == 117:
        return unicode_escape(text, pos, pieces)
    else:
        return simple_escape(text, pos, c, pieces)


def simple_escape(text, pos, c, pieces):
    """Handle a simple backslash escape letter `c`."""
    match escaped_char(c):
        case Success(value=value):
            return string_body(text, pos + 2, listAppend(pieces, value))
        case Error(message=message):
            return Error(message=message)


def unicode_escape(text, pos, pieces):
    """Parse a \\uXXXX escape (backslash at `pos`) and append the codepoint."""
    hex = unwrap_or(substring(text, pos + 2, pos + 6), "")
    cp = hex_value(hex)
    if cp < 0:
        return Error(message="invalid JSON: bad unicode escape")
    else:
        return string_body(text, pos + 6, listAppend(pieces, unwrap_or(fromCodePoint(cp), "")))


def parse_string(text, pos):
    """Parse the string starting at `pos` (an opening quote)."""
    return string_body(text, pos + 1, [])


def array_items(text, pos, items) -> Result[Parsed, str]:
    """Parse `items` of an array from `pos`, or the closing bracket."""
    p = skip_ws(text, pos)
    c = unwrap_or(byteAt(text, p), 0)
    if c == 93:
        return Success(value=Parsed(value=jarr(items), next=p + 1))
    else:
        return array_item(text, p, items)


def array_item(text, p, items):
    """Parse one array item at `p` and continue the array."""
    match skip_value(text, p):
        case Success(value=value):
            return array_continue(text, value.next, listAppend(items, value.value))
        case Error(message=message):
            return Error(message=message)


def array_continue(text, pos, items):
    """After one array item, parse a comma and the next item or the close."""
    p = skip_ws(text, pos)
    c = unwrap_or(byteAt(text, p), 0)
    if c == 93:
        return Success(value=Parsed(value=jarr(items), next=p + 1))
    elif c == 44:
        return array_items(text, p + 1, items)
    else:
        return Error(message="invalid JSON: expected ',' or ']'")


def parse_array(text, pos):
    """Parse the array starting at `pos` (an opening bracket)."""
    return array_items(text, pos + 1, [])


def object_members(text, pos, members) -> Result[Parsed, str]:
    """Parse `members` of an object from `pos`, or the closing brace."""
    p = skip_ws(text, pos)
    c = unwrap_or(byteAt(text, p), 0)
    if c == 125:
        return Success(value=Parsed(value=jobj(members), next=p + 1))
    elif c == 34:
        return object_key(text, p, members)
    else:
        return Error(message="invalid JSON: expected member name")


def object_key(text, p, members):
    """Parse the member name whose opening quote is at `p`, then its value."""
    match parse_string(text, p):
        case Success(value=value):
            return member_value(text, value.value.s, value.next, members)
        case Error(message=message):
            return Error(message=message)


def member_value(text, name, pos, members):
    """Parse `: value` after the member named `name`, appending to `members`."""
    p = skip_ws(text, pos)
    if unwrap_or(byteAt(text, p), 0) != 58:
        return Error(message="invalid JSON: expected ':'")
    else:
        return member_body(text, name, p + 1, members)


def member_body(text, name, pos, members):
    """Parse the member's value at `pos` and continue the object."""
    match skip_value(text, pos):
        case Success(value=value):
            member = JsonMember(name=name, value=value.value)
            return object_after(text, value.next, listAppend(members, member))
        case Error(message=message):
            return Error(message=message)


def object_after(text, pos, members):
    """After a member's value: a comma and the next member, or the close."""
    p = skip_ws(text, pos)
    c = unwrap_or(byteAt(text, p), 0)
    if c == 44:
        return object_members(text, p + 1, members)
    elif c == 125:
        return Success(value=Parsed(value=jobj(members), next=p + 1))
    else:
        return Error(message="invalid JSON: expected ',' or '}'")


def parse_object(text, pos):
    """Parse the object starting at `pos` (an opening brace)."""
    return object_members(text, pos + 1, [])


def float_number(token, end):
    """Parse a float token's text into a float value."""
    match parseFloat(token):
        case Success(value=value):
            return Success(value=Parsed(value=jfloat(value), next=end))
        case Error(message=message):
            return Error(message="invalid JSON: bad number")


def int_number(token, end):
    """Parse an integer token's text into an int value."""
    match parseInt(token):
        case Success(value=value):
            return Success(value=Parsed(value=jint(value), next=end))
        case Error(message=message):
            return Error(message="invalid JSON: bad number")


def number_value(token, end):
    """Parse a number token's text into an int or float value."""
    if is_float_token(token):
        return float_number(token, end)
    else:
        return int_number(token, end)


def parse_number(text, pos):
    """Parse the number starting at `pos` into an int or float value."""
    end = number_end(text, pos)
    token = unwrap_or(substring(text, pos, end), "")
    return number_value(token, end)


def skip_value(text, pos):
    """Parse the value starting at `pos` (after whitespace)."""
    p = skip_ws(text, pos)
    c = unwrap_or(byteAt(text, p), 0)
    if c == 123:
        return parse_object(text, p)
    elif c == 91:
        return parse_array(text, p)
    elif c == 34:
        return parse_string(text, p)
    elif c == 116:
        return parse_literal(text, p, "true", jbool(True))
    elif c == 102:
        return parse_literal(text, p, "false", jbool(False))
    elif c == 110:
        return parse_literal(text, p, "null", jnull())
    elif c == 45 or (c >= 48 and c <= 57):
        return parse_number(text, p)
    else:
        return Error(message="invalid JSON: unexpected character")


def ensure_end(text, p):
    """The parse succeeds only when nothing but whitespace follows the value."""
    end = skip_ws(text, p.next)
    if end == len(text):
        return Success(value=p.value)
    else:
        return Error(message="invalid JSON: trailing characters")


def parse(text):
    """Parse `text` as one complete JSON value, or Error with a reason."""
    match skip_value(text, 0):
        case Success(value=value):
            return ensure_end(text, value)
        case Error(message=message):
            return Error(message=message)
