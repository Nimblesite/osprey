# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=JsonUtil
"""Reading and building `Json` values the way the port's wire code needs.

Not a pydantic_ai module: it stands in for the dict/pydantic_core access the
CPython code does inline (`data.get('x')`, `to_json`, `from_json`).
"""

__all__ = [
    "text_at", "int_at", "float_at", "bool_at", "list_at", "object_at", "member", "obj",
    "encode", "decode", "members_of", "items_of", "of_map", "to_map", "is_null", "jtext", "jint", "member_name", "member_value",
]


def is_null(value) -> bool:
    match value:
        case JsonNull():
            return True
        case JsonBool():
            return False
        case JsonInt():
            return False
        case JsonFloat():
            return False
        case JsonString():
            return False
        case JsonArray():
            return False
        case JsonObject():
            return False


def text_at(value, name, fallback) -> str:
    """`value[name]` as text, `fallback` when absent or not a string."""
    return unwrap_or(jsonText(unwrap_or(jsonField(value, name), JsonNull())), fallback)


def int_at(value, name, fallback) -> int:
    """`value[name]` as an int, `fallback` when absent or not an integer."""
    return unwrap_or(jsonInt(unwrap_or(jsonField(value, name), JsonNull())), fallback)


def float_at(value, name, fallback) -> float:
    """`value[name]` as a float (an integer converts), `fallback` otherwise."""
    field = unwrap_or(jsonField(value, name), JsonNull())
    match field:
        case JsonInt(i):
            return toFloat(i)
        case JsonFloat(f):
            return f
        case JsonNull():
            return fallback
        case JsonBool():
            return fallback
        case JsonString():
            return fallback
        case JsonArray():
            return fallback
        case JsonObject():
            return fallback


def bool_at(value, name, fallback) -> bool:
    """`value[name]` as a bool, `fallback` when absent or not a boolean."""
    return unwrap_or(jsonBool(unwrap_or(jsonField(value, name), JsonNull())), fallback)


def items_of(value) -> list[Json]:
    """The elements of a JSON array, `[]` for anything else."""
    match value:
        case JsonArray(items):
            return items
        case JsonNull():
            return []
        case JsonBool():
            return []
        case JsonInt():
            return []
        case JsonFloat():
            return []
        case JsonString():
            return []
        case JsonObject():
            return []


def members_of(value) -> list[JsonMember]:
    """The members of a JSON object, `[]` for anything else."""
    match value:
        case JsonObject(members):
            return members
        case JsonNull():
            return []
        case JsonBool():
            return []
        case JsonInt():
            return []
        case JsonFloat():
            return []
        case JsonString():
            return []
        case JsonArray():
            return []


def list_at(value, name) -> list[Json]:
    """`value[name]` as an array's elements, `[]` when absent."""
    return items_of(unwrap_or(jsonField(value, name), JsonNull()))


def object_at(value, name) -> Json:
    """`value[name]`, `JsonNull()` when absent."""
    return unwrap_or(jsonField(value, name), JsonNull())


def member(name, value) -> JsonMember:
    return JsonMember(name, value)


def obj(members) -> Json:
    return JsonObject(members)


def jtext(text) -> Json:
    return JsonString(text)


def jint(i) -> Json:
    return JsonInt(i)


def encode(value) -> str:
    """Compact JSON text of a value; an unencodable value spells `null`."""
    return unwrap_or(jsonEncode(value), "null")


def decode(text) -> Result[Json, str]:
    """The value JSON text spells."""
    match jsonDecode(text):
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=message)


def member_name(m) -> str:
    match m:
        case JsonMember(name, _value):
            return name


def member_value(m) -> Json:
    match m:
        case JsonMember(_name, value):
            return value


def to_map(value) -> dict[str, Json]:
    """A JSON object as a map (a repeated name keeps its last value)."""
    empty: dict[str, Json] = {}
    return foldList(members_of(value), empty, lambda acc, m: mapSet(acc, member_name(m), member_value(m)))


def of_map(values) -> Json:
    """A map as a JSON object, members in key order."""
    return JsonObject(mapList(mapKeys(values), lambda key: JsonMember(key, unwrap_or(mapGet(values, key), JsonNull()))))
