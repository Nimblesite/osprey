# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Value
"""The JSON-like value a Jinja2 template renders over, with the semantics the
chat-template subset needs.

A value is one of `none`, `bool`, `int`, `float`, `string`, `list`, `dict`
(insertion-ordered keys), `pair` (one `items()` element), `macro` (a name the
renderer's closure expands) or `undefined` (Jinja's default `Undefined`: it
renders empty, fails only when an operation needs its value, and compares
unequal to everything except another undefined). All operations are pure;
expected failure propagates as an error Value.
Implements [PORT-SCOPE] value semantics for @osprey/jinja2.
"""

from jinja2 import Parser as P

__all__ = [
    "Value", "Slice", "Env", "KwVal", "Macro",
    "none", "bool_", "int_", "float_", "text", "list_", "dict_", "typed_dict", "undefined", "pair", "typed_pair",
    "K_NONE", "K_BOOL", "K_INT", "K_FLOAT", "K_STRING", "K_LIST", "K_DICT", "K_UNDEFINED", "K_PAIR", "K_FUNC",
    "K_MACRO",
    "truthy", "equal", "less", "concat_text", "repr_text", "contains", "dict_get", "attr_get", "key_of", "chars",
    "str_len", "str_get", "slice_list", "slice_str", "json_text", "str_split", "str_split_sep",
    "py_mod", "py_floordiv", "capitalize", "lookup", "bind", "bind_found", "bind_at", "dict_of", "dict_set",
    "is_undefined", "is_none", "is_string", "is_number", "func", "num_of", "char_values", "halt", "is_error",
    "env_with", "bind_macro", "macro_ref", "find_macro",
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
K_MACRO = "macro"
K_ERROR = "error"


class Value:
    """One template value; only the fields its `kind` names are meaningful."""
    kind: str
    bval: bool
    ival: int
    fval: float
    sval: str
    elems: list[Value]
    keys: list[Value]
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
    """The renderer's scope chain; `frames[0]` is the outermost frame. `macros`
    holds the `{% macro %}` declarations the template has made so far."""
    frames: list[dict[str, Value]]
    macros: list[Macro]


class KwVal:
    """One keyword argument: its name and evaluated value."""
    name: str
    value: Value


class Macro:
    """A declared macro: its node, and the scope chain it was declared in, which
    its body resolves template globals through."""
    node: P.Node
    frames: list[dict[str, Value]]


class CharScan:
    """A codepoint scan: the characters decoded so far and how many continuation
    bytes the last one still covers."""
    acc: list[str]
    skip: int


class Bounds:
    """A slice's resolved Python window `[start, stop)` and its step."""
    start: int
    stop: int
    step: int


class Cut:
    """A separator scan: the parts so far, the byte offset the next part starts at,
    and whether the text is finished."""
    parts: list[str]
    start: int
    done: bool


class Words:
    """A whitespace split: the words so far and the word being read."""
    words: list[str]
    current: str


def make_value(kind, bval, ival, fval, sval, elems, keys, vals):
    """Build the shared typed layout of every template value."""
    return Value(kind=kind, bval=bval, ival=ival, fval=fval, sval=sval, elems=elems, keys=keys, vals=vals)


def none():
    """The `None` value."""
    return make_value(K_NONE, False, 0, 0.0, "", [], [], [])


def bool_(b):
    """A boolean value."""
    return make_value(K_BOOL, b, 0, 0.0, "", [], [], [])


def int_(n):
    """An integer value."""
    return make_value(K_INT, False, n, 0.0, "", [], [], [])


def float_(x):
    """A float value."""
    return make_value(K_FLOAT, False, 0, x, "", [], [], [])


def text(s):
    """A string value."""
    return make_value(K_STRING, False, 0, 0.0, s, [], [], [])


def list_(elems):
    """A list value."""
    return make_value(K_LIST, False, 0, 0.0, "", elems, [], [])


def dict_(keys, vals):
    """A dict value; keys stay in insertion order."""
    return typed_dict(toList(map(range(len(keys)), lambda i: text(unwrap_or(listGet(keys, i), "")))), vals)


def typed_dict(keys, vals):
    """A dictionary retaining each key's value type and insertion order."""
    return make_value(K_DICT, False, 0, 0.0, "", [], keys, vals)


def undefined(name):
    """An undefined value naming the variable or attribute that was missing."""
    return make_value(K_UNDEFINED, False, 0, 0.0, name, [], [], [])


def pair(key, value):
    """One `dict.items()` element with a string key."""
    return typed_pair(text(key), value)


def typed_pair(key, value):
    """One `dict.items()` element retaining its key's value type."""
    return make_value(K_PAIR, False, 0, 0.0, concat_text(key), [key], [], [value])


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
        return list_equal(a.elems, b.elems) and equal(unwrap_or(listGet(a.vals, 0), none()), unwrap_or(listGet(b.vals, 0), none()))
    else:
        return dict_equal(a, b)


def list_equal(xs, ys):
    """Element-wise equality of two lists."""
    if len(xs) != len(ys):
        return False
    else:
        return all_from(len(xs), lambda i: equal(unwrap_or(listGet(xs, i), none()), unwrap_or(listGet(ys, i), none())))


def dict_equal(a, b):
    """Key-set and value equality of two dicts (order-insensitive, as Python)."""
    if len(a.keys) != len(b.keys):
        return False
    else:
        return all_from(len(a.keys), lambda i: equal(dict_get(a, unwrap_or(listGet(a.keys, i), none())), dict_get(b, unwrap_or(listGet(a.keys, i), none()))))


def all_from(n, pred):
    """Whether `pred` holds for every index below `n` (never recursion on length)."""
    return fold(range(n), True, lambda held, i: held and pred(i))


def any_from(n, pred):
    """Whether `pred` holds for some index below `n`."""
    return fold(range(n), False, lambda found, i: found or pred(i))


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
            return "(" + repr_text(unwrap_or(listGet(v.elems, 0), none())) + ", " + repr_text(unwrap_or(listGet(v.vals, 0), none())) + ")"
        case _:
            return repr_dict(v)


def repr_list(elems):
    """`[1, 'a']`: a list as Python prints it."""
    return "[" + join(toList(map(range(len(elems)), lambda i: repr_text(unwrap_or(listGet(elems, i), none())))), ", ") + "]"


def repr_dict(v):
    """`{'k': 1}`: a dict as Python prints it."""
    entries = toList(map(range(len(v.keys)), lambda i: repr_text(unwrap_or(listGet(v.keys, i), none())) + ": " + repr_text(unwrap_or(listGet(v.vals, i), none()))))
    return "{" + join(entries, ", ") + "}"


def repr_text(v):
    """A value as Python prints it inside a container (strings are quoted)."""
    if v.kind == K_STRING:
        return quote(v.sval)
    else:
        return concat_text(v)


def quote(s):
    """A string as Python's repr prints it, choosing the least escaped delimiter."""
    delimiter = '"' if contains_text(s, "'") and not contains_text(s, '"') else "'"
    cs = chars(s)
    return delimiter + fold(range(len(cs)), "", lambda acc, i: acc + esc_char(unwrap_or(listGet(cs, i), ""), delimiter)) + delimiter


def esc_char(c, delimiter):
    """Escape the chosen repr delimiter, backslashes and control characters."""
    if c == delimiter:
        return "\\" + c
    else:
        match c:
            case "\\":
                return "\\\\"
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
            return any_from(len(container.elems), lambda i: equal(unwrap_or(listGet(container.elems, i), none()), item))
        case "dict":
            return any_from(len(container.keys), lambda i: equal(unwrap_or(listGet(container.keys, i), none()), item))
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
            return fold(range(len(v.keys)), undefined(concat_text(key)), lambda acc, i: unwrap_or(listGet(v.vals, i), none()) if equal(unwrap_or(listGet(v.keys, i), none()), key) else acc)
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
    return dict_get(v, text(key))


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
    """A string's codepoints as single-character strings, one per UTF-8 sequence.
    One fold over the byte offsets, so a megabyte string costs no stack."""
    return fold(range(length(s)), CharScan(acc=[], skip=0), lambda st, i: char_step(s, st, i)).acc


def char_values(s):
    """A string's codepoints as string Values (for iteration)."""
    cs = chars(s)
    return toList(map(range(len(cs)), lambda i: text(unwrap_or(listGet(cs, i), ""))))


def char_step(s, st, i):
    """Decode the codepoint starting at byte `i`, or step over a continuation byte."""
    if st.skip > 0:
        return CharScan(acc=st.acc, skip=st.skip - 1)
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        return CharScan(acc=listAppend(st.acc, unwrap_or(fromCodePoint(cp), "?")), skip=unwrap_or(codePointWidth(cp), 1) - 1)


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
    """The concrete index list of `x[start:stop:step]` (Python slice rules), built
    by one fold over the window rather than one frame per index."""
    step = sl.step if sl.has_step else 1
    if step == 0:
        return []
    else:
        b = slice_bounds(n, sl, step)
        return toList(map(range(slice_count(b)), lambda k: b.start + k * step))


def slice_bounds(n, sl, step):
    """Python's `slice.indices(n)` window for a non-zero `step`."""
    if step > 0:
        return Bounds(start=high_bound(sl.start if sl.has_start else 0, sl.has_start, n), stop=high_bound(sl.stop if sl.has_stop else n, sl.has_stop, n), step=step)
    else:
        return Bounds(start=low_bound(sl.start if sl.has_start else n - 1, sl.has_start, n), stop=low_bound(sl.stop if sl.has_stop else -1, sl.has_stop, n), step=step)


def high_bound(raw, given, n):
    """A positive-step bound: a written negative index counts from the end, then
    clamps to `0..n`. An absent bound is its default and is not shifted."""
    return clamp_index(raw + n if given and raw < 0 else raw, 0, n)


def low_bound(raw, given, n):
    """A negative-step bound: a written negative index counts from the end, then
    clamps to `-1..n-1`. An absent bound is its default and is not shifted."""
    return clamp_index(raw + n if given and raw < 0 else raw, -1, n - 1)


def clamp_index(value, lo, hi):
    """`value` held inside `[lo, hi]`."""
    if value < lo:
        return lo
    elif value > hi:
        return hi
    else:
        return value


def slice_count(b):
    """How many indices `range(b.start, b.stop, b.step)` holds."""
    if b.step > 0:
        return 0 if b.stop <= b.start else (b.stop - b.start + b.step - 1) // b.step
    elif b.stop >= b.start:
        return 0
    else:
        span = 0 - b.step
        return (b.start - b.stop + span - 1) // span


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


def json_value(v, list_text, dict_text):
    """JSON for a scalar; a list is written by `list_text` and anything else by `dict_text`."""
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
            return list_text(v)
        case _:
            return dict_text(v)


def json_compact(v):
    """Compact JSON with the default `', '` / `': '` separators."""
    return json_value(v,
        lambda l: "[" + join(toList(map(range(len(l.elems)), lambda i: json_compact(unwrap_or(listGet(l.elems, i), none())))), ", ") + "]",
        lambda d: "{" + join(toList(map(range(len(d.keys)), lambda i: json_quote(concat_text(unwrap_or(listGet(d.keys, i), none()))) + ": " + json_compact(unwrap_or(listGet(d.vals, i), none())))), ", ") + "}")


def json_pretty(v, indent, level):
    """Indented JSON: `indent` spaces per level, non-empty containers split onto lines."""
    return json_value(v, lambda l: json_pretty_list(l.elems, indent, level), lambda d: json_pretty_dict(d, indent, level))


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
        inner = join(toList(map(range(len(v.keys)), lambda i: pad(indent, level + 1) + json_quote(concat_text(unwrap_or(listGet(v.keys, i), none()))) + ": " + json_pretty(unwrap_or(listGet(v.vals, i), none()), indent, level + 1))), ",\n")
        return "{\n" + inner + "\n" + pad(indent, level) + "}"


def pad(indent, level):
    """`level` indent stops of `indent` spaces each."""
    return unwrap_or(repeat(" ", indent * level), "")


def json_quote(s):
    """A string as JSON prints it: the runtime's `jsonQuote`, which escapes quotes,
    backslashes and U+0001–U+001F with their shortest escape and leaves every other
    character as UTF-8 (`json.dumps(..., ensure_ascii=False)`)."""
    return unwrap_or(jsonQuote(s), "null")


def str_split(s):
    """Python `str.split()`: split on runs of whitespace, dropping empties."""
    cs = chars(s)
    st = fold(range(len(cs)), Words(words=[], current=""), lambda state, i: word_step(unwrap_or(listGet(cs, i), " "), state))
    return st.words if st.current == "" else listAppend(st.words, st.current)


def word_step(c, st):
    """Extend the current word, or close it at a whitespace character."""
    if is_ws_char(c):
        return Words(words=st.words, current="") if st.current == "" else Words(words=listAppend(st.words, st.current), current="")
    else:
        return Words(words=st.words, current=st.current + c)


def is_ws_char(c):
    """Whether the single-character string `c` is ASCII whitespace."""
    return c == " " or c == "\t" or c == "\n" or c == "\r" or c == "\x0b" or c == "\x0c"


def str_split_sep(s, sep):
    """Python `str.split(sep)`: split on every occurrence, keeping empties."""
    if sep == "":
        return [s]
    else:
        return fold(range(length(s) + 1), Cut(parts=[], start=0, done=False), lambda st, _k: cut_step(s, sep, st)).parts


def cut_step(s, sep, st):
    """Cut one occurrence of `sep` from the scan's offset, or finish the text."""
    if st.done:
        return st
    else:
        rest = unwrap_or(substring(s, st.start, length(s)), "")
        idx = unwrap_or(indexOf(rest, sep), -1)
        if idx < 0:
            return Cut(parts=listAppend(st.parts, rest), start=st.start, done=True)
        else:
            return Cut(parts=listAppend(st.parts, unwrap_or(substring(s, st.start, st.start + idx), "")), start=st.start + idx + length(sep), done=False)


def capitalize(s):
    """Python `str.capitalize()` over ASCII: first letter upper, the rest lower."""
    if s == "":
        return ""
    else:
        first = unwrap_or(substring(s, 0, 1), "")
        rest = unwrap_or(substring(s, 1, length(s)), "")
        return toUpperCase(first) + toLowerCase(rest)


def lookup(env, name):
    """The value of `name`: the innermost frame that binds it, else a macro, else undefined."""
    v = lookup_from(env, len(env.frames) - 1, name)
    if v.kind == K_UNDEFINED and binds_macro(env.macros, name):
        return macro_ref(name)
    else:
        return v


def binds_macro(macros, name):
    """Whether `macros` holds a macro named `name`."""
    return fold(range(len(macros)), False, lambda found, i: found or unwrap_or(listGet(macros, i), no_macro()).node.name == name)


def no_macro():
    """The placeholder a missing macro reads as; its name never matches a call."""
    return Macro(node=P.blank(""), frames=[])


def macro_ref(name):
    """A value naming a macro, which the evaluator expands through the environment."""
    return make_value(K_MACRO, False, 0, 0.0, name, [], [], [])


def bind_macro(env, node):
    """`env` with macro `node` declared in the current scope; a later declaration
    of the same name wins."""
    return Env(frames=env.frames, macros=listAppend(env.macros, Macro(node=node, frames=env.frames)))


def find_macro(env, i, name):
    """The macro declared as `name`, searching from the end so a redefinition
    wins; a missing name reads as a nameless placeholder."""
    if i < 0:
        return no_macro()
    else:
        m = unwrap_or(listGet(env.macros, i), no_macro())
        return m if m.node.name == name else find_macro(env, i - 1, name)


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
    return env_with(env, replace_at(env.frames, i, mapSet(frame, name, value)))


def env_with(env, frames):
    """`env` over `frames`, keeping its macros."""
    return Env(frames=frames, macros=env.macros)


def replace_at(xs, i, x):
    """`xs` with element `i` replaced by `x`."""
    return toList(map(range(len(xs)), lambda k: x if k == i else unwrap_or(listGet(xs, k), x)))


def dict_of(pairs):
    """A dict Value built from a list of (key, value) pairs."""
    return fold(range(len(pairs)), dict_([], []), lambda acc, i: dict_add(acc, unwrap_or(listGet(pairs, i), pair("", none()))))


def dict_add(v, p):
    """`v` with one (key, value) pair appended."""
    return make_value(K_DICT, False, 0, 0.0, "", [], listAppend(v.keys, unwrap_or(listGet(p.elems, 0), none())), listAppend(v.vals, unwrap_or(listGet(p.vals, 0), none())))


def dict_set(v, key, value):
    """`v` with `key` bound to `value`, replacing an existing key or appending a new one."""
    at = key_index(v, text(key))
    if at < 0:
        return make_value(K_DICT, False, 0, 0.0, "", [], listAppend(v.keys, text(key)), listAppend(v.vals, value))
    else:
        return make_value(K_DICT, False, 0, 0.0, "", [], replace_at(v.keys, at, text(key)), replace_at(v.vals, at, value))


def key_index(v, key):
    """The index of `key` in `v`'s keys, or -1 (one fold, never recursion on length)."""
    return fold(range(len(v.keys)), -1, lambda found, i: found if found >= 0 else (i if equal(unwrap_or(listGet(v.keys, i), none()), key) else -1))


def func(name):
    """A global/macro function reference, called by the evaluator."""
    return make_value(K_FUNC, False, 0, 0.0, name, [], [], [])


def halt(message):
    """An error value carrying `message`, propagated up through the evaluator and
    turned into the render `Result`'s `Error` payload by the renderer."""
    return make_value(K_ERROR, False, 0, 0.0, message, [], [], [])


def is_error(v):
    """Whether `v` is an error value (a fault to propagate, not to render)."""
    return v.kind == K_ERROR
