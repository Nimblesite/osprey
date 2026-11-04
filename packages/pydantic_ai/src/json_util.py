# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=JsonUtil
"""Reading and building `Json` values the way the port's wire code needs.

Not a pydantic_ai module: it stands in for the dict/pydantic_core access the
CPython code does inline (`data.get('x')`, `to_json`, `from_json`).
"""

__all__ = [
    "text_at", "int_at", "float_at", "bool_at", "list_at", "object_at", "member", "obj",
    "encode", "decode", "members_of", "items_of", "of_map", "to_map", "is_null", "has", "jtext", "jint", "member_name", "member_value",
]


def is_null(value) -> bool:
    """Whether a value is JSON null."""
    match value:
        case JsonNull():
            return True
        case _:
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
        case _:
            return fallback


def bool_at(value, name, fallback) -> bool:
    """`value[name]` as a bool, `fallback` when absent or not a boolean."""
    return unwrap_or(jsonBool(unwrap_or(jsonField(value, name), JsonNull())), fallback)


def items_of(value) -> list[Json]:
    """The elements of a JSON array, `[]` for anything else."""
    match value:
        case JsonArray(items):
            return items
        case _:
            return []


def members_of(value) -> list[JsonMember]:
    """The members of a JSON object, `[]` for anything else."""
    match value:
        case JsonObject(members):
            return members
        case _:
            return []


def list_at(value, name) -> list[Json]:
    """`value[name]` as an array's elements, `[]` when absent."""
    return items_of(unwrap_or(jsonField(value, name), JsonNull()))


def object_at(value, name) -> Json:
    """`value[name]`, `JsonNull()` when absent."""
    return unwrap_or(jsonField(value, name), JsonNull())


def has(value, name) -> bool:
    """Whether an object carries a member `name`; a JSON null member is present, an absent one is not."""
    return len(filterList(members_of(value), lambda m: member_name(m) == name)) > 0


def member(name, value) -> JsonMember:
    """`JsonMember(name, value)`."""
    return JsonMember(name, value)


def obj(members) -> Json:
    """`JsonObject(members)`."""
    return JsonObject(members)


def jtext(text) -> Json:
    """`JsonString(text)`."""
    return JsonString(text)


def jint(i) -> Json:
    """`JsonInt(i)`."""
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
    """The name of an object member."""
    match m:
        case JsonMember(name, _value):
            return name


def member_value(m) -> Json:
    """The value of an object member."""
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
