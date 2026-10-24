# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=jinja2
# osprey: module=Value
"""The JSON-like value a Jinja2 template renders over, with the semantics the
chat-template subset needs.

A value is one of `none`, `bool`, `int`, `float`, `string`, `list`, `dict`
(insertion-ordered keys), `pair` (one `items()` element) or `undefined` (Jinja's
default `Undefined`: it renders empty, fails only when an operation needs its
value, and compares unequal to everything except another undefined). All
operations are pure; expected failure propagates as an error Value.
Implements [PORT-SCOPE] value semantics for @osprey/jinja2.
"""

__all__ = [
    "Value", "Slice", "Env", "KwVal",
    "none", "bool_", "int_", "float_", "text", "list_", "dict_", "undefined", "pair",
    "K_NONE", "K_BOOL", "K_INT", "K_FLOAT", "K_STRING", "K_LIST", "K_DICT", "K_UNDEFINED", "K_PAIR", "K_FUNC",
    "truthy", "equal", "less", "concat_text", "repr_text", "contains", "dict_get", "attr_get", "key_of", "chars",
    "str_len", "str_get", "slice_list", "slice_str", "json_text", "str_split", "str_split_sep",
    "py_mod", "py_floordiv", "capitalize", "lookup", "bind", "bind_found", "bind_at", "dict_of", "dict_set",
    "is_undefined", "is_none", "is_string", "is_number", "func", "num_of", "char_values", "halt", "is_error",
]


K_NONE = "none"
K_BOOL = "bool"
K_INT = "int"
K_FLOAT = "float"
K_STRING = "string"
K_LIST = "list"
K_DICT = "dict"
K_UNDEFINED = "undefined"
K_PAIR = "pair"
K_FUNC = "func"
K_ERROR = "error"


class Value:
    """One template value; only the fields its `kind` names are meaningful."""
    kind: str
    bval: bool
    ival: int
    fval: float
    sval: str
    elems: list[Value]
    keys: list[str]
    vals: list[Value]


class Slice:
    """A subscript slice `a[start:stop:step]`; an absent bound has its `has_` flag false."""
    start: int
    stop: int
    step: int
    has_start: bool
    has_stop: bool
    has_step: bool


class Env:
    """The renderer's scope chain; `frames[0]` is the outermost frame."""
    frames: list[dict[str, Value]]


class KwVal:
    """One keyword argument: its name and evaluated value."""
    name: str
    value: Value


def none():
    """The `None` value."""
    return Value(kind=K_NONE, bval=False, ival=0, fval=0.0, sval="", elems=[], keys=[], vals=[])


def bool_(b):
    """A boolean value."""
    return Value(kind=K_BOOL, bval=b, ival=0, fval=0.0, sval="", elems=[], keys=[], vals=[])


def int_(n):
    """An integer value."""
    return Value(kind=K_INT, bval=False, ival=n, fval=0.0, sval="", elems=[], keys=[], vals=[])


def float_(x):
    """A float value."""
    return Value(kind=K_FLOAT, bval=False, ival=0, fval=x, sval="", elems=[], keys=[], vals=[])


def text(s):
    """A string value."""
    return Value(kind=K_STRING, bval=False, ival=0, fval=0.0, sval=s, elems=[], keys=[], vals=[])


def list_(elems):
    """A list value."""
    return Value(kind=K_LIST, bval=False, ival=0, fval=0.0, sval="", elems=elems, keys=[], vals=[])


def dict_(keys, vals):
    """A dict value; keys stay in insertion order."""
    return Value(kind=K_DICT, bval=False, ival=0, fval=0.0, sval="", elems=[], keys=keys, vals=vals)


def undefined(name):
    """An undefined value naming the variable or attribute that was missing."""
    return Value(kind=K_UNDEFINED, bval=False, ival=0, fval=0.0, sval=name, elems=[], keys=[], vals=[])


def pair(key, value):
    """One `dict.items()` element."""
    return Value(kind=K_PAIR, bval=False, ival=0, fval=0.0, sval=key, elems=[], keys=[], vals=[value])


def is_undefined(v):
    """Whether `v` is undefined."""
    return v.kind == K_UNDEFINED


def is_none(v):
    """Whether `v` is `None` (distinct from undefined)."""
    return v.kind == K_NONE


def is_string(v):
    """Whether `v` is a string."""
    return v.kind == K_STRING


def is_number(v):
    """Whether `v` is an int or a float."""
    return v.kind == K_INT or v.kind == K_FLOAT


def truthy(v):
    """Jinja truthiness: `none`, `undefined`, `False`, zero, and empty
    strings/lists/dicts are falsy; everything else is truthy."""
    match v.kind:
        case "none":
            return False
        case "undefined":
            return False
        case "bool":
            return v.bval
        case "int":
            return v.ival != 0
        case "float":
            return v.fval != 0.0
        case "string":
            return v.sval != ""
        case "list":
            return len(v.elems) > 0
        case _:
            return len(v.keys) > 0


def num_of(v):
    """A numeric value as a float (0.0 otherwise), for cross-type comparisons."""
    if v.kind == K_INT:
        return toFloat(v.ival)
    elif v.kind == K_FLOAT:
        return v.fval
    else:
        return 0.0


def equal(a, b):
    """Jinja `==`: numeric equality across int/float, deep equality for lists and
    dicts, and undefined equal only to undefined."""
    if a.kind == K_UNDEFINED or b.kind == K_UNDEFINED:
        return a.kind == K_UNDEFINED and b.kind == K_UNDEFINED
    elif is_number(a) and is_number(b):
        return num_of(a) == num_of(b)
    elif a.kind != b.kind:
        return False
    elif a.kind == K_NONE:
        return True
    elif a.kind == K_BOOL:
        return a.bval == b.bval
    elif a.kind == K_STRING:
        return a.sval == b.sval
    elif a.kind == K_LIST:
        return list_equal(a.elems, b.elems)
    elif a.kind == K_PAIR:
        return a.sval == b.sval and equal(unwrap_or(listGet(a.vals, 0), none()), unwrap_or(listGet(b.vals, 0), none()))
    else:
        return dict_equal(a, b)


def list_equal(xs, ys):
    """Element-wise equality of two lists."""
    if len(xs) != len(ys):
        return False
    else:
        return all_from(len(xs), 0, lambda i: equal(unwrap_or(listGet(xs, i), none()), unwrap_or(listGet(ys, i), none())))


def dict_equal(a, b):
    """Key-set and value equality of two dicts (order-insensitive, as Python)."""
    if len(a.keys) != len(b.keys):
        return False
    else:
        return all_from(len(a.keys), 0, lambda i: equal(dict_get(a, text(unwrap_or(listGet(a.keys, i), ""))), dict_get(b, text(unwrap_or(listGet(a.keys, i), "")))))


def all_from(n, i, pred):
    """Whether `pred` holds for every index from `i` to `n - 1`."""
    if i >= n:
        return True
    elif pred(i):
        return all_from(n, i + 1, pred)
    else:
        return False


def any_from(n, i, pred):
    """Whether `pred` holds for some index from `i` to `n - 1`."""
    if i >= n:
        return False
    elif pred(i):
        return True
    else:
        return any_from(n, i + 1, pred)


def less(a, b):
    """Jinja `<`: numeric or string ordering, false for other pairs."""
    if is_number(a) and is_number(b):
        return num_of(a) < num_of(b)
    elif a.kind == K_STRING and b.kind == K_STRING:
        return a.sval < b.sval
    else:
        return False


def py_floordiv(a, b):
    """Python `//`: floor division (Osprey's `intDiv` truncates toward zero)."""
    if a % b == 0:
        return intDiv(a, b)
    elif a < 0 and b > 0:
        return intDiv(a, b) - 1
    elif a > 0 and b < 0:
        return intDiv(a, b) - 1
    else:
        return intDiv(a, b)


def py_mod(a, b):
    """Python `%`: the remainder with the divisor's sign."""
    return a - py_floordiv(a, b) * b


def concat_text(v):
    """How `{{ v }}` and `~` render a value at the top level."""
    match v.kind:
        case "none":
            return "None"
        case "bool":
            return "True" if v.bval else "False"
        case "int":
            return toString(v.ival)
        case "float":
            return toString(v.fval)
        case "string":
            return v.sval
        case "undefined":
            return ""
        case "list":
            return repr_list(v.elems)
        case "pair":
            return "(" + repr_text(text(v.sval)) + ", " + repr_text(unwrap_or(listGet(v.vals, 0), none())) + ")"
        case _:
            return repr_dict(v)


def repr_list(elems):
    """`[1, 'a']`: a list as Python prints it."""
    return "[" + join(toList(map(range(len(elems)), lambda i: repr_text(unwrap_or(listGet(elems, i), none())))), ", ") + "]"


def repr_dict(v):
    """`{'k': 1}`: a dict as Python prints it."""
    entries = toList(map(range(len(v.keys)), lambda i: repr_text(text(unwrap_or(listGet(v.keys, i), ""))) + ": " + repr_text(unwrap_or(listGet(v.vals, i), none()))))
    return "{" + join(entries, ", ") + "}"


def repr_text(v):
    """A value as Python prints it inside a container (strings are quoted)."""
    if v.kind == K_STRING:
        return quote(v.sval)
    else:
        return concat_text(v)


def quote(s):
    """A string as Python's repr prints it (single quotes, backslash escapes)."""
    return "'" + escape(s) + "'"


def escape(s):
    """Backslash-escape the characters Python's repr escapes in a string."""
    cs = chars(s)
    return fold(range(len(cs)), "", lambda acc, i: acc + esc_char(unwrap_or(listGet(cs, i), "")))


def esc_char(c):
    """The repr escape for one character."""
    match c:
        case "\\":
            return "\\\\"
        case "'":
            return "\\'"
        case "\n":
            return "\\n"
        case "\r":
            return "\\r"
        case "\t":
            return "\\t"
        case _:
            return c


def contains(container, item):
    """Jinja `in`: list/dict membership, or substring test for strings."""
    match container.kind:
        case "list":
            return any_from(len(container.elems), 0, lambda i: equal(unwrap_or(listGet(container.elems, i), none()), item))
        case "dict":
            return item.kind == K_STRING and any_from(len(container.keys), 0, lambda i: unwrap_or(listGet(container.keys, i), "") == item.sval)
        case "string":
            return item.kind == K_STRING and contains_text(container.sval, item.sval)
        case _:
            return False


def contains_text(s, needle):
    """Whether `needle` is a substring of `s`."""
    if needle == "":
        return True
    else:
        return unwrap_or(indexOf(s, needle), -1) >= 0


def dict_get(v, key):
    """`v[key]`: a list element, string codepoint or dict value; out of range or a
    missing key is undefined; a subscript on undefined faults."""
    match v.kind:
        case "dict":
            return key_of(v, key.sval)
        case "list":
            return list_get(v.elems, key)
        case "string":
            return str_get(v.sval, key)
        case "undefined":
            return halt("'" + v.sval + "' is undefined")
        case _:
            return undefined("")


def key_of(v, key):
    """The value of a dict key, or an undefined when absent."""
    return fold(range(len(v.keys)), undefined(key), lambda acc, i: unwrap_or(listGet(v.vals, i), none()) if unwrap_or(listGet(v.keys, i), "") == key else acc)


def list_get(elems, key):
    """`elems[key]` with Python negative indexing; out of range is undefined."""
    if key.kind == K_INT:
        i = key.ival if key.ival >= 0 else len(elems) + key.ival
        if i >= 0 and i < len(elems):
            return unwrap_or(listGet(elems, i), none())
        else:
            return undefined("")
    else:
        return undefined("")


def attr_get(v, name):
    """`v.name`: a dict key, or an undefined when absent (Jinja's getattr falls
    back to getitem for dicts); an attribute of undefined faults."""
    if v.kind == "undefined":
        return halt("'" + v.sval + "' is undefined")
    elif v.kind == K_DICT:
        return key_of(v, name)
    else:
        return undefined(name)


def chars(s):
    """A string's codepoints as single-character strings."""
    return chars_from(s, 0, [])


def char_values(s):
    """A string's codepoints as string Values (for iteration)."""
    cs = chars(s)
    return toList(map(range(len(cs)), lambda i: text(unwrap_or(listGet(cs, i), ""))))


def chars_from(s, i, acc):
    """Scan codepoints of `s` from byte index `i`."""
    if i >= length(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        return chars_from(s, i + unwrap_or(codePointWidth(cp), 1), listAppend(acc, unwrap_or(fromCodePoint(cp), "?")))


def str_len(s):
    """A string's codepoint count (Python's `len`, not Osprey's byte `length`)."""
    return fold(range(length(s)), 0, lambda n, i: n + lead_count(s, i))


def lead_count(s, i):
    """Whether byte `i` starts a UTF-8 codepoint, so `i` counts exactly once."""
    b = unwrap_or(byteAt(s, i), 0)
    if b < 128 or b >= 192:
        return 1
    else:
        return 0


def str_get(s, key):
    """`s[key]` by codepoint index; out of range is undefined."""
    if key.kind == K_INT:
        i = key.ival if key.ival >= 0 else str_len(s) + key.ival
        cs = chars(s)
        if i >= 0 and i < len(cs):
            return text(unwrap_or(listGet(cs, i), ""))
        else:
            return undefined("")
    else:
        return undefined("")


def slice_indices(n, sl):
    """The concrete index list of `x[start:stop:step]` (Python slice rules)."""
    step = sl.step if sl.has_step else 1
    if step == 0:
        return []
    else:
        start = start_of(n, sl, step)
        stop = sl.stop if sl.has_stop else ((-1) if step < 0 else n)
        return walk(n, start, stop, step, [])


def start_of(n, sl, step):
    """A slice's start bound with Python's negative-index clamping."""
    raw = sl.start if sl.has_start else (0 if step > 0 else n - 1)
    if raw < 0:
        return n + raw
    else:
        return raw


def walk(n, i, stop, step, acc):
    """Collect indices from `i` by `step` while inside `[0, n)` and before `stop`."""
    if step > 0:
        return walk_up(n, i, stop, step, acc)
    else:
        return walk_down(n, i, stop, step, acc)


def walk_up(n, i, stop, step, acc):
    """Indices ascending from `i` while inside `[0, n)` and `< stop`."""
    if i >= n or i >= stop:
        return acc
    elif i < 0:
        return walk_up(n, i + step, stop, step, acc)
    else:
        return walk_up(n, i + step, stop, step, listAppend(acc, i))


def walk_down(n, i, stop, step, acc):
    """Indices descending from `i` while inside `[0, n)` and `> stop`."""
    if i < 0 or i <= stop:
        return acc
    elif i >= n:
        return walk_down(n, i + step, stop, step, acc)
    else:
        return walk_down(n, i + step, stop, step, listAppend(acc, i))


def slice_list(elems, sl):
    """`elems[sl]` as Python slices it."""
    indices = slice_indices(len(elems), sl)
    return toList(map(range(len(indices)), lambda k: unwrap_or(listGet(elems, unwrap_or(listGet(indices, k), 0)), none())))


def slice_str(s, sl):
    """`s[sl]` by codepoints, as Python slices it."""
    cs = chars(s)
    indices = slice_indices(len(cs), sl)
    return join(toList(map(range(len(indices)), lambda k: unwrap_or(listGet(cs, unwrap_or(listGet(indices, k), 0)), ""))), "")


def json_text(v, indent):
    """`tojson` output: json.dumps with ensure_ascii=False and sort_keys=False."""
    if indent == 0:
        return json_compact(v)
    else:
        return json_pretty(v, indent, 0)


def json_compact(v):
    """Compact JSON with the default `', '` / `': '` separators."""
    match v.kind:
        case "none":
            return "null"
        case "undefined":
            return "null"
        case "bool":
            return "true" if v.bval else "false"
        case "int":
            return toString(v.ival)
        case "float":
            return toString(v.fval)
        case "string":
            return json_quote(v.sval)
        case "list":
            return "[" + join(toList(map(range(len(v.elems)), lambda i: json_compact(unwrap_or(listGet(v.elems, i), none())))), ", ") + "]"
        case _:
            return "{" + join(toList(map(range(len(v.keys)), lambda i: json_quote(unwrap_or(listGet(v.keys, i), "")) + ": " + json_compact(unwrap_or(listGet(v.vals, i), none())))), ", ") + "}"


def json_pretty(v, indent, level):
    """Indented JSON: `indent` spaces per level, non-empty containers split onto lines."""
    match v.kind:
        case "none":
            return "null"
        case "undefined":
            return "null"
        case "bool":
            return "true" if v.bval else "false"
        case "int":
            return toString(v.ival)
        case "float":
            return toString(v.fval)
        case "string":
            return json_quote(v.sval)
        case "list":
            return json_pretty_list(v.elems, indent, level)
        case _:
            return json_pretty_dict(v, indent, level)


def json_pretty_list(elems, indent, level):
    """An indented JSON list."""
    if len(elems) == 0:
        return "[]"
    else:
        inner = join(toList(map(range(len(elems)), lambda i: pad(indent, level + 1) + json_pretty(unwrap_or(listGet(elems, i), none()), indent, level + 1))), ",\n")
        return "[\n" + inner + "\n" + pad(indent, level) + "]"


def json_pretty_dict(v, indent, level):
    """An indented JSON dict."""
    if len(v.keys) == 0:
        return "{}"
    else:
        inner = join(toList(map(range(len(v.keys)), lambda i: pad(indent, level + 1) + json_quote(unwrap_or(listGet(v.keys, i), "")) + ": " + json_pretty(unwrap_or(listGet(v.vals, i), none()), indent, level + 1))), ",\n")
        return "{\n" + inner + "\n" + pad(indent, level) + "}"


def pad(indent, level):
    """`level` indent stops of `indent` spaces each."""
    return unwrap_or(repeat(" ", indent * level), "")


def json_quote(s):
    """A string as JSON prints it (double quotes, backslash escapes, raw unicode)."""
    return "\"" + json_escape(s) + "\""


def json_escape(s):
    """JSON's string escapes."""
    cs = chars(s)
    return fold(range(len(cs)), "", lambda acc, i: acc + json_esc_char(unwrap_or(listGet(cs, i), "")))


def json_esc_char(c):
    """The JSON escape for one character."""
    match c:
        case "\"":
            return "\\\""
        case "\\":
            return "\\\\"
        case "\n":
            return "\\n"
        case "\r":
            return "\\r"
        case "\t":
            return "\\t"
        case "\b":
            return "\\b"
        case "\f":
            return "\\f"
        case _:
            return c


def str_split(s):
    """Python `str.split()`: split on runs of whitespace, dropping empties."""
    return ws_split(chars(s), 0, [], "")


def ws_split(cs, i, acc, word):
    """Group codepoints of `cs` into whitespace-separated words."""
    if i >= len(cs):
        return acc if word == "" else listAppend(acc, word)
    else:
        c = unwrap_or(listGet(cs, i), " ")
        if is_ws_char(c):
            return ws_split(cs, i + 1, acc if word == "" else listAppend(acc, word), "")
        else:
            return ws_split(cs, i + 1, acc, word + c)


def is_ws_char(c):
    """Whether the single-character string `c` is ASCII whitespace."""
    return c == " " or c == "\t" or c == "\n" or c == "\r" or c == "\x0b" or c == "\x0c"


def str_split_sep(s, sep):
    """Python `str.split(sep)`: split on every occurrence, keeping empties."""
    if sep == "":
        return [s]
    else:
        return split_sep_from(s, sep, 0, [])


def split_sep_from(s, sep, i, acc):
    """Cut `s` on each occurrence of `sep` from byte offset `i`."""
    if i > length(s):
        return acc
    else:
        rest = unwrap_or(substring(s, i, length(s)), "")
        idx = unwrap_or(indexOf(rest, sep), -1)
        if idx < 0:
            return listAppend(acc, unwrap_or(substring(s, i, length(s)), ""))
        else:
            return split_sep_from(s, sep, i + idx + length(sep), listAppend(acc, unwrap_or(substring(s, i, i + idx), "")))


def capitalize(s):
    """Python `str.capitalize()` over ASCII: first letter upper, the rest lower."""
    if s == "":
        return ""
    else:
        first = unwrap_or(substring(s, 0, 1), "")
        rest = unwrap_or(substring(s, 1, length(s)), "")
        return toUpperCase(first) + toLowerCase(rest)


def lookup(env, name):
    """The value of `name` in the innermost frame that binds it, or undefined."""
    return lookup_from(env, len(env.frames) - 1, name)


def lookup_from(env, i, name):
    """Search frames from index `i` down to 0 for `name`."""
    if i < 0:
        return undefined(name)
    else:
        frame = unwrap_or(listGet(env.frames, i), {})
        if mapContains(frame, name):
            return unwrap_or(mapGet(frame, name), undefined(name))
        else:
            return lookup_from(env, i - 1, name)


def bind(env, name, value):
    """Bind `name` in the innermost frame."""
    return bind_at(env, len(env.frames) - 1, name, value)


def bind_found(env, name, value):
    """Bind `name` in the frame that already holds it (a namespace write), or the
    innermost frame when no frame holds it."""
    return bind_from(env, len(env.frames) - 1, name, value)


def bind_from(env, i, name, value):
    """Bind `name` in the first frame from `i` down that holds it, else the innermost."""
    if i < 0:
        return bind_at(env, len(env.frames) - 1, name, value)
    else:
        frame = unwrap_or(listGet(env.frames, i), {})
        if mapContains(frame, name):
            return bind_at(env, i, name, value)
        else:
            return bind_from(env, i - 1, name, value)


def bind_at(env, i, name, value):
    """Bind `name` in frame `i` (which must exist)."""
    frame = unwrap_or(listGet(env.frames, i), {})
    return Env(frames=replace_at(env.frames, i, mapSet(frame, name, value)))


def replace_at(xs, i, x):
    """`xs` with element `i` replaced by `x`."""
    return toList(map(range(len(xs)), lambda k: x if k == i else unwrap_or(listGet(xs, k), x)))


def dict_of(pairs):
    """A dict Value built from a list of (key, value) pairs."""
    return fold(range(len(pairs)), dict_([], []), lambda acc, i: dict_add(acc, unwrap_or(listGet(pairs, i), pair("", none()))))


def dict_add(v, p):
    """`v` with one (key, value) pair appended."""
    return Value(kind=K_DICT, bval=False, ival=0, fval=0.0, sval="", elems=[], keys=listAppend(v.keys, p.sval), vals=listAppend(v.vals, unwrap_or(listGet(p.vals, 0), none())))


def dict_set(v, key, value):
    """`v` with `key` bound to `value`, replacing an existing key or appending a new one."""
    return dict_set_at(v, key, value, 0)


def dict_set_at(v, key, value, i):
    """Walk `v`'s keys: replace `key` at its position, else append at the end."""
    if i >= len(v.keys):
        return Value(kind=K_DICT, bval=False, ival=0, fval=0.0, sval="", elems=[], keys=listAppend(v.keys, key), vals=listAppend(v.vals, value))
    elif unwrap_or(listGet(v.keys, i), "") == key:
        return Value(kind=K_DICT, bval=False, ival=0, fval=0.0, sval="", elems=[], keys=replace_at(v.keys, i, key), vals=replace_at(v.vals, i, value))
    else:
        return dict_set_at(v, key, value, i + 1)


def func(name):
    """A global/macro function reference, called by the evaluator."""
    return Value(kind=K_FUNC, bval=False, ival=0, fval=0.0, sval=name, elems=[], keys=[], vals=[])


def halt(message):
    """An error value carrying `message`, propagated up through the evaluator and
    turned into the render `Result`'s `Error` payload by the renderer."""
    return Value(kind=K_ERROR, bval=False, ival=0, fval=0.0, sval=message, elems=[], keys=[], vals=[])


def is_error(v):
    """Whether `v` is an error value (a fault to propagate, not to render)."""
    return v.kind == K_ERROR
