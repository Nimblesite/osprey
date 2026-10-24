# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Filters
"""Filters, tests and methods of the chat-template subset, over `Value`.

`apply_filter` answers `x | name(args)`, `apply_test` answers `x is name(args)`
(and `selectattr`/`rejectattr`/`map`/`select`/`reject` internals), and
`call_method` answers `x.name(args)`. Names and semantics follow Jinja 3.1.6;
`tojson` follows transformers' override (json.dumps with ensure_ascii=False).
"""

from jinja2.Value import halt
from jinja2 import Value as V

__all__ = ["apply_filter", "apply_test", "call_method"]


def arg(args, i):
    """Argument `i`, or `none` when absent."""
    return unwrap_or(listGet(args, i), V.none())


def drop(xs, n):
    """`xs[n:]`."""
    return V.slice_list(xs, V.Slice(start=n, stop=len(xs), step=1, has_start=True, has_stop=True, has_step=True))


def has_kw(kwargs, name):
    """Whether `kwargs` holds `name`."""
    return fold(range(len(kwargs)), False, lambda acc, i: acc or unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none())).name == name)


def kw_get(kwargs, name):
    """The value of keyword `name`, or `none` when absent."""
    return fold(range(len(kwargs)), V.none(), lambda acc, i: unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none())).value if unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none())).name == name else acc)


def f_trim(v):
    """`|trim`: strip ASCII whitespace; undefined stays undefined (renders empty)."""
    if v.kind == "string":
        return V.text(trim(v.sval))
    else:
        return v


def f_upper(v):
    """`|upper`: ASCII uppercase."""
    if v.kind == "string":
        return V.text(toUpperCase(v.sval))
    else:
        return v


def f_lower(v):
    """`|lower`: ASCII lowercase."""
    if v.kind == "string":
        return V.text(toLowerCase(v.sval))
    else:
        return v


def f_capitalize(v):
    """`|capitalize`: ASCII capitalize."""
    if v.kind == "string":
        return V.text(V.capitalize(v.sval))
    else:
        return v


def f_length(v):
    """`|length`: codepoints for strings, elements for lists/dicts, 0 for undefined."""
    match v.kind:
        case "string":
            return V.int_(V.str_len(v.sval))
        case "list":
            return V.int_(len(v.elems))
        case "dict":
            return V.int_(len(v.keys))
        case _:
            return V.int_(0)


def f_tojson(v, kwargs):
    """`|tojson`: transformers' json.dumps override."""
    indent = kw_get(kwargs, "indent")
    n = 0 if indent.kind == V.K_NONE else indent.ival
    return V.text(V.json_text(v, n))


def f_join(v, args):
    """`|join(sep)`: concatenate the elements' text with `sep` (default "")."""
    sep = "" if len(args) == 0 else arg(args, 0).sval
    elems = iterable_elements(v)
    return V.text(join(toList(map(range(len(elems)), lambda i: V.concat_text(unwrap_or(listGet(elems, i), V.none())))), sep))


def f_replace(v, args):
    """`|replace(old, new)`: replace every occurrence."""
    if v.kind == V.K_STRING:
        return V.text(unwrap_or(replace(v.sval, arg(args, 0).sval, arg(args, 1).sval), v.sval))
    else:
        return v


def f_default(v, args):
    """`|default(d, boolean=false)`: `d` when the value is undefined (or falsy
    when `boolean`), else the value itself."""
    d = V.text("") if len(args) == 0 else arg(args, 0)
    boolean = len(args) > 1 and V.truthy(arg(args, 1))
    if boolean:
        return d if not V.truthy(v) else v
    else:
        return d if v.kind == V.K_UNDEFINED else v


def f_first(v):
    """`|first`: the first element or character; undefined when empty."""
    elems = iterable_elements(v)
    if len(elems) == 0:
        return V.undefined("No first item, sequence was empty.")
    else:
        return unwrap_or(listGet(elems, 0), V.none())


def f_last(v):
    """`|last`: the last element or character; undefined when empty."""
    elems = iterable_elements(v)
    if len(elems) == 0:
        return V.undefined("No last item, sequence was empty.")
    else:
        return unwrap_or(listGet(elems, len(elems) - 1), V.none())


def f_list(v):
    """`|list`: a string's characters, a dict's keys, else the elements."""
    return V.list_(iterable_elements(v))


def f_string(v):
    """`|string`: soft_str — the text form of the value."""
    return V.text(V.concat_text(v))


def f_items(v):
    """`|items`: a dict's (key, value) pairs; undefined yields an empty list."""
    if v.kind == "dict":
        return V.list_(pairs_of(v))
    else:
        return V.list_([])


def iterable_elements(v):
    """The values `for`/`map`/`join`/`list`/`first`/`last` iterate: list elements,
    dict keys (as strings), string characters, or nothing."""
    match v.kind:
        case "list":
            return v.elems
        case "dict":
            return toList(map(range(len(v.keys)), lambda i: V.text(unwrap_or(listGet(v.keys, i), ""))))
        case "string":
            return V.char_values(v.sval)
        case _:
            return []


def pairs_of(v):
    """A dict's `(key, value)` pair values."""
    return toList(map(range(len(v.keys)), lambda i: V.pair(unwrap_or(listGet(v.keys, i), ""), unwrap_or(listGet(v.vals, i), V.none()))))


def select_or_reject(v, args, lookup_attr, keep):
    """The shared body of select/selectattr (keep) and reject/rejectattr (not keep)."""
    elems = iterable_elements(v)
    return V.list_(filterList(elems, lambda item: keep == select_keep(item, args, lookup_attr)))


def select_keep(item, args, lookup_attr):
    """Whether one item survives: its (attribute's) test, or its truthiness."""
    if lookup_attr:
        return select_test(V.attr_get(item, arg(args, 0).sval), args, 1)
    else:
        return select_test(item, args, 0)


def select_test(sub, args, off):
    """Apply the test named at `off` (or truthiness when absent) to `sub`."""
    if len(args) > off:
        testname = arg(args, off).sval
        return V.truthy(apply_test(testname, sub, drop(args, off + 1)))
    else:
        return V.truthy(sub)


def f_map(v, args, kwargs):
    """`|map`: `map(attribute=...)`, or `map('filter', *args)` per element."""
    elems = iterable_elements(v)
    if len(args) == 0 and has_kw(kwargs, "attribute"):
        attr = kw_get(kwargs, "attribute").sval
        default = kw_get(kwargs, "default")
        return V.list_(mapList(elems, lambda item: attr_default(item, attr, default)))
    else:
        name = arg(args, 0).sval
        rest = drop(args, 1)
        return V.list_(mapList(elems, lambda item: apply_filter(name, item, rest, [])))


def attr_default(item, attr, default):
    """`item.attr`, or `default` when the attribute is absent."""
    r = V.attr_get(item, attr)
    if r.kind == V.K_UNDEFINED:
        return default
    else:
        return r


def apply_filter(name, value, args, kwargs):
    """Apply the named filter to `value`."""
    match name:
        case "trim":
            return f_trim(value)
        case "upper":
            return f_upper(value)
        case "lower":
            return f_lower(value)
        case "capitalize":
            return f_capitalize(value)
        case "length":
            return f_length(value)
        case "count":
            return f_length(value)
        case "tojson":
            return f_tojson(value, kwargs)
        case "join":
            return f_join(value, args)
        case "replace":
            return f_replace(value, args)
        case "default":
            return f_default(value, args)
        case "d":
            return f_default(value, args)
        case "first":
            return f_first(value)
        case "last":
            return f_last(value)
        case "list":
            return f_list(value)
        case "string":
            return f_string(value)
        case "items":
            return f_items(value)
        case "selectattr":
            return select_or_reject(value, args, True, True)
        case "rejectattr":
            return select_or_reject(value, args, True, False)
        case "select":
            return select_or_reject(value, args, False, True)
        case "reject":
            return select_or_reject(value, args, False, False)
        case "map":
            return f_map(value, args, kwargs)
        case _:
            return halt("No filter named '" + name + "'")


def apply_test(name, value, args):
    """Apply the named test to `value`, yielding a boolean Value."""
    match name:
        case "defined":
            return V.bool_(value.kind != V.K_UNDEFINED)
        case "undefined":
            return V.bool_(value.kind == V.K_UNDEFINED)
        case "none":
            return V.bool_(value.kind == V.K_NONE)
        case "string":
            return V.bool_(value.kind == V.K_STRING)
        case "number":
            return V.bool_(V.is_number(value))
        case "integer":
            return V.bool_(value.kind == V.K_INT)
        case "float":
            return V.bool_(value.kind == V.K_FLOAT)
        case "boolean":
            return V.bool_(value.kind == V.K_BOOL)
        case "equalto":
            return V.bool_(V.equal(value, arg(args, 0)))
        case "eq":
            return V.bool_(V.equal(value, arg(args, 0)))
        case "==":
            return V.bool_(V.equal(value, arg(args, 0)))
        case "ne":
            return V.bool_(not V.equal(value, arg(args, 0)))
        case "!=":
            return V.bool_(not V.equal(value, arg(args, 0)))
        case "in":
            return V.bool_(V.contains(arg(args, 0), value))
        case "mapping":
            return V.bool_(value.kind == V.K_DICT)
        case "iterable":
            return V.bool_(value.kind == V.K_LIST or value.kind == V.K_DICT or value.kind == V.K_STRING)
        case "sequence":
            return V.bool_(value.kind == V.K_LIST or value.kind == V.K_STRING)
        case "sameas":
            return V.bool_(V.equal(value, arg(args, 0)))
        case _:
            return halt("No test named '" + name + "'")


def call_method(obj, name, args):
    """Apply a method to a value (string and dict methods of the subset)."""
    if obj.kind == "string":
        return str_method(obj, name, args)
    elif obj.kind == "dict":
        return dict_method(obj, name, args)
    else:
        return halt("method `" + name + "` is not supported on this value")


def str_method(obj, name, args):
    """String methods: strip, split, startswith, endswith, lower, upper, ..."""
    s = obj.sval
    match name:
        case "strip":
            return V.text(trim(s))
        case "lstrip":
            return V.text(trimStart(s))
        case "rstrip":
            return V.text(trimEnd(s))
        case "split":
            if len(args) == 0:
                return V.list_(toList(map(range(len(V.str_split(s))), lambda i: V.text(unwrap_or(listGet(V.str_split(s), i), "")))))
            else:
                return V.list_(toList(map(range(len(V.str_split_sep(s, arg(args, 0).sval))), lambda i: V.text(unwrap_or(listGet(V.str_split_sep(s, arg(args, 0).sval), i), "")))))
        case "startswith":
            return V.bool_(startsWith(s, arg(args, 0).sval))
        case "endswith":
            return V.bool_(endsWith(s, arg(args, 0).sval))
        case "lower":
            return V.text(toLowerCase(s))
        case "upper":
            return V.text(toUpperCase(s))
        case "capitalize":
            return V.text(V.capitalize(s))
        case "replace":
            return V.text(unwrap_or(replace(s, arg(args, 0).sval, arg(args, 1).sval), s))
        case _:
            return halt("method `" + name + "` is not supported on strings")


def dict_method(obj, name, args):
    """Dict methods: items, keys, values, get."""
    match name:
        case "items":
            return V.list_(pairs_of(obj))
        case "keys":
            return V.list_(toList(map(range(len(obj.keys)), lambda i: V.text(unwrap_or(listGet(obj.keys, i), "")))))
        case "values":
            return V.list_(obj.vals)
        case "get":
            r = V.key_of(obj, arg(args, 0).sval)
            if r.kind == V.K_UNDEFINED and len(args) > 1:
                return arg(args, 1)
            else:
                return r
        case _:
            return halt("method `" + name + "` is not supported on dicts")
