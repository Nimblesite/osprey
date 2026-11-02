# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=JsonSchema
"""pydantic_ai._json_schema and profiles.openai: the OpenAI strict-mode
schema transformer.

Port of `JsonSchemaTransformer.walk` with `OpenAIJsonSchemaTransformer.transform`
(2.46.0) for a tool whose `strict` is unset (CPython's `None`): every object
gains `additionalProperties: false`, `title` and `$schema` are dropped, and the
walk records whether the schema is strict-compatible (every property required,
no incompatible keyword, typed array items, no `default`, no `oneOf`), which
becomes the tool's `strict`. `$ref`/`$defs` are walked in place, not inlined.
"""
from pydantic_ai import JsonUtil as J

__all__ = ["Transformed", "transform", "STRICT_INCOMPATIBLE_KEYS", "STRICT_COMPATIBLE_STRING_FORMATS"]

STRICT_INCOMPATIBLE_KEYS = [
    "minLength", "maxLength", "patternProperties", "unevaluatedProperties", "propertyNames", "minProperties",
    "maxProperties", "unevaluatedItems", "contains", "minContains", "maxContains", "uniqueItems",
]
"""`_STRICT_INCOMPATIBLE_KEYS`: keywords OpenAI strict mode rejects."""

STRICT_COMPATIBLE_STRING_FORMATS = ["date-time", "time", "date", "duration", "email", "hostname", "ipv4", "ipv6", "uuid"]
"""`_STRICT_COMPATIBLE_STRING_FORMATS`: the string formats strict mode accepts."""

TYPE_BEARING_KEYS = ["type", "$ref", "anyOf", "oneOf", "allOf", "enum", "const"]
"""`_TYPE_BEARING_KEYS`: keys that give array items a type."""

DROPPED_KEYS = ["title", "$schema", "discriminator"]


class Transformed:
    """A walked schema and whether it is strict-compatible."""
    schema: Json
    is_strict_compatible: bool


def transform(schema) -> Transformed:
    """`OpenAIJsonSchemaTransformer(schema).walk()` plus `is_strict_compatible`."""
    walked = handle(schema)
    defs = J.object_at(schema, "$defs")
    if J.is_null(defs):
        return walked
    else:
        walked_defs = foldList(J.members_of(defs), Transformed(schema=J.obj([]), is_strict_compatible=walked.is_strict_compatible), def_step)
        return Transformed(schema=J.obj(listAppend(J.members_of(walked.schema), J.member("$defs", walked_defs.schema))), is_strict_compatible=walked_defs.is_strict_compatible)


def def_step(acc, m):
    inner = handle(J.member_value(m))
    return Transformed(schema=J.obj(listAppend(J.members_of(acc.schema), J.member(J.member_name(m), inner.schema))), is_strict_compatible=acc.is_strict_compatible and inner.is_strict_compatible)


def without(members, keys) -> list[JsonMember]:
    return filterList(members, lambda m: not listContains(keys, J.member_name(m)))


def handle(schema) -> Transformed:
    """`_handle`: walk the children by type, then `transform` this node."""
    match schema:
        case JsonObject():
            type_ = J.text_at(schema, "type", "")
            base = without(J.members_of(schema), ["$defs"])
            children = handle_object(base) if type_ == "object" else (handle_array(base) if type_ == "array" else handle_unions(base, True))
            unions = children if type_ == "" else handle_unions_of(children.schema, children.is_strict_compatible)
            return transform_node(unions.schema, unions.is_strict_compatible)
        case JsonBool():
            return Transformed(schema=schema, is_strict_compatible=True)
        case JsonNull():
            return Transformed(schema=schema, is_strict_compatible=True)
        case JsonInt():
            return Transformed(schema=schema, is_strict_compatible=True)
        case JsonFloat():
            return Transformed(schema=schema, is_strict_compatible=True)
        case JsonString():
            return Transformed(schema=schema, is_strict_compatible=True)
        case JsonArray():
            return Transformed(schema=schema, is_strict_compatible=True)


def handle_unions(members, compatible) -> Transformed:
    return handle_unions_of(J.obj(members), compatible)


def handle_unions_of(schema, compatible) -> Transformed:
    """Walk every member of `allOf`, `anyOf` and `oneOf`."""
    return foldList(["allOf", "anyOf", "oneOf"], Transformed(schema=schema, is_strict_compatible=compatible), union_step)


def union_step(acc, kind):
    items = J.list_at(acc.schema, kind)
    if len(items) == 0:
        return acc
    else:
        walked = foldList(items, Transformed(schema=JsonArray([]), is_strict_compatible=acc.is_strict_compatible), item_step)
        return Transformed(schema=J.obj(listAppend(without(J.members_of(acc.schema), [kind]), J.member(kind, walked.schema))), is_strict_compatible=walked.is_strict_compatible)


def item_step(acc, item):
    inner = handle(item)
    return Transformed(schema=JsonArray(listAppend(J.items_of(acc.schema), inner.schema)), is_strict_compatible=acc.is_strict_compatible and inner.is_strict_compatible)


def handle_object(members) -> Transformed:
    """`_handle_object`: walk `properties` and an object-valued `additionalProperties`."""
    schema = J.obj(members)
    props = J.members_of(J.object_at(schema, "properties"))
    walked = foldList(props, Transformed(schema=J.obj([]), is_strict_compatible=True), def_step)
    with_props = J.obj(listAppend(without(members, ["properties"]), J.member("properties", walked.schema))) if len(props) > 0 else schema
    additional = J.object_at(schema, "additionalProperties")
    match additional:
        case JsonObject():
            inner = handle(additional)
            return Transformed(schema=J.obj(listAppend(without(J.members_of(with_props), ["additionalProperties"]), J.member("additionalProperties", inner.schema))), is_strict_compatible=walked.is_strict_compatible and inner.is_strict_compatible)
        case JsonBool():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)
        case JsonNull():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)
        case JsonInt():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)
        case JsonFloat():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)
        case JsonString():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)
        case JsonArray():
            return Transformed(schema=with_props, is_strict_compatible=walked.is_strict_compatible)


def handle_array(members) -> Transformed:
    """`_handle_array`: walk `prefixItems` and `items`."""
    schema = J.obj(members)
    prefix = J.list_at(schema, "prefixItems")
    walked_prefix = foldList(prefix, Transformed(schema=JsonArray([]), is_strict_compatible=True), item_step)
    with_prefix = J.obj(listAppend(without(members, ["prefixItems"]), J.member("prefixItems", walked_prefix.schema))) if len(prefix) > 0 else schema
    items = J.object_at(schema, "items")
    match items:
        case JsonObject():
            inner = handle(items)
            return Transformed(schema=J.obj(listAppend(without(J.members_of(with_prefix), ["items"]), J.member("items", inner.schema))), is_strict_compatible=walked_prefix.is_strict_compatible and inner.is_strict_compatible)
        case JsonNull():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)
        case JsonBool():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)
        case JsonInt():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)
        case JsonFloat():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)
        case JsonString():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)
        case JsonArray():
            return Transformed(schema=with_prefix, is_strict_compatible=walked_prefix.is_strict_compatible)


def has_key(schema, key) -> bool:
    return len(filterList(J.members_of(schema), lambda m: J.member_name(m) == key)) > 0


def transform_node(schema, compatible) -> Transformed:
    """`OpenAIJsonSchemaTransformer.transform` with `strict=None`."""
    members = without(J.members_of(schema), DROPPED_KEYS)
    type_ = J.text_at(schema, "type", "")
    incompatible = len(filterList(STRICT_INCOMPATIBLE_KEYS, lambda key: has_key(schema, key))) > 0
    format_ = J.text_at(schema, "format", "")
    bad_format = format_ != "" and not listContains(STRICT_COMPATIBLE_STRING_FORMATS, format_)
    still = compatible and not has_key(schema, "default") and not incompatible and not bad_format and not has_key(schema, "oneOf")
    if type_ == "object":
        additional = J.object_at(schema, "additionalProperties")
        allows_extra = not J.is_null(additional) and not J.encode(additional) == "false"
        props = J.members_of(J.object_at(schema, "properties"))
        required = mapList(J.list_at(schema, "required"), lambda r: unwrap_or(jsonText(r), ""))
        all_required = has_key(schema, "properties") and has_key(schema, "required") and len(filterList(props, lambda m: not listContains(required, J.member_name(m)))) == 0
        with_props = members if has_key(schema, "properties") else listAppend(members, J.member("properties", J.obj([])))
        closed = listAppend(without(with_props, ["additionalProperties"]), J.member("additionalProperties", JsonBool(False))) if not allows_extra else with_props
        return Transformed(schema=J.obj(closed), is_strict_compatible=still and not allows_extra and all_required)
    elif type_ == "array":
        items = J.object_at(schema, "items")
        typed = len(filterList(TYPE_BEARING_KEYS, lambda key: has_key(items, key))) > 0
        return Transformed(schema=J.obj(members), is_strict_compatible=still and (typed or has_key(schema, "prefixItems")))
    else:
        return Transformed(schema=J.obj(members), is_strict_compatible=still)
