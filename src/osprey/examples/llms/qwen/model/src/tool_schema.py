# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=ToolSchema
"""JSON Schema validation and parameter coercion for the Qwen3.5 tool-call format.

The Qwen chat template writes every tool call as `<tool_call>`, then one
`<function=NAME>`, then one `<parameter=NAME>` per argument, then the closing
tags. A string argument is written verbatim, so it may span lines and hold any
text the format can carry; every other argument is written as the JSON literal
`tojson` produced, so an object or array argument arrives as JSON text.
`coerce` reads one parameter text into a `Json` value by its declared type, and
`validate` checks a value against the tool schema sub-language: `type`, `enum`,
`items`, `properties`, `required` and `additionalProperties`.

Implements [LLM-TOOL-PARSE] and the schema half of [LLM-TOOL-REPAIR] of
docs/specs/applications/LocalCodingModel.md. A violation names the JSON Schema
keyword and the instance path as `keyword@path`, the pair `jsonschema` reports,
so the suite compares the port with the real package case by case.

Divergences from `jsonschema`, both on the strict side and recorded in the
README: `integer` accepts only a JSON integer literal (`jsonschema`'s integer
type also accepts `3.0`, which the template never writes for an integer
argument), and every failure is a `Result` rather than a raised exception.
"""

__all__ = [
    "kind",
    "kind_matches",
    "members",
    "property_names",
    "property_schema",
    "required_names",
    "items_schema",
    "allows_extra",
    "enum_values",
    "tool_list",
    "tool_name",
    "tool_named",
    "parameters",
    "validate",
    "coerce",
    "violation",
    "violation_keyword",
    "violation_path",
]


# --- schema readers -------------------------------------------------------

def kind(schema):
    """The declared JSON type of `schema`, or "" when it constrains none."""
    match jsonText(if_present(jsonField(schema, "type"))):
        case Success(value=declared):
            return declared
        case _:
            return ""


def if_present(field):
    """The value of a schema field, or `JsonNull` when it is absent."""
    match field:
        case Success(value=value):
            return value
        case _:
            return JsonNull


def members(schema):
    """The `properties` members of an object schema, in declared order."""
    match if_present(jsonField(schema, "properties")):
        case JsonObject(properties):
            return properties
        case _:
            return []


def property_names(schema):
    """The declared property names of an object schema, in declared order."""
    return mapList(members(schema), lambda member: member_name(member))


def property_schema(schema, name):
    """The schema of property `name`, or `JsonNull` when it declares none."""
    declared = filterList(members(schema), lambda member: member_name(member) == name)
    return foldList(declared, JsonNull, lambda _, member: member_value(member))


def required_names(schema):
    """The `required` property names, in declared order; none when absent."""
    return mapList(array_items(if_present(jsonField(schema, "required"))), quoted_text)


def items_schema(schema):
    """The `items` schema of an array, or `JsonNull` for an unconstrained item."""
    return if_present(jsonField(schema, "items"))


def allows_extra(schema):
    """False only when the schema declares `additionalProperties: false`."""
    match if_present(jsonField(schema, "additionalProperties")):
        case JsonBool(allowed):
            return allowed
        case _:
            return True


def enum_values(schema):
    """The `enum` members of a schema, or none when it declares no enum."""
    return array_items(if_present(jsonField(schema, "enum")))


def tool_list(tools):
    """The tool definitions of a request's tool list, in request order."""
    return array_items(tools)


def tool_name(tool):
    """A tool definition's `function.name`, or "" when it has none."""
    return quoted_text(if_present(jsonField(if_present(jsonField(tool, "function")), "name")))


def tool_named(tools, name):
    """The tool definition named `name` in a request's tool list, or `JsonNull`."""
    return foldList(
        tool_list(tools),
        JsonNull,
        lambda found, tool: found if is_null(found) == False else named_tool(tool, name),
    )


def named_tool(tool, name):
    """`tool` when its `function.name` is `name`, else `JsonNull`."""
    return tool if tool_name(tool) == name else JsonNull


def parameters(tool):
    """A tool definition's `function.parameters` schema."""
    return if_present(jsonField(if_present(jsonField(tool, "function")), "parameters"))


# --- Json readers ---------------------------------------------------------

def array_items(value):
    match value:
        case JsonArray(items):
            return items
        case _:
            return []


def object_members(value):
    match value:
        case JsonObject(items):
            return items
        case _:
            return []


def member_name(member):
    match member:
        case JsonMember(name, _value):
            return name
        case _:
            return ""


def member_value(member):
    match member:
        case JsonMember(_name, value):
            return value
        case _:
            return JsonNull


def quoted_text(value):
    """A JSON string value's text, or "" for anything else."""
    match jsonText(value):
        case Success(value=text):
            return text
        case _:
            return ""


# --- validation -----------------------------------------------------------

def kind_matches(declared, value):
    """Whether `value` is an instance of the JSON type `declared` ("" accepts all)."""
    match declared:
        case "":
            return True
        case "string":
            return holds(jsonText(value))
        case "boolean":
            return holds(jsonBool(value))
        case "integer":
            return holds(jsonInt(value))
        case "number":
            return holds(jsonInt(value)) or holds(jsonFloat(value))
        case _:
            return container_matches(declared, value)


def container_matches(declared, value):
    """The container and null types, whose instance test is a tag check."""
    match declared:
        case "object":
            return is_object(value)
        case "array":
            return is_array(value)
        case "null":
            return is_null(value)
        case _:
            return False


def holds(result):
    """Whether an accessor accepted its value."""
    match result:
        case Success(_accepted):
            return True
        case _:
            return False


def is_object(value):
    match value:
        case JsonObject(_members):
            return True
        case _:
            return False


def is_array(value):
    match value:
        case JsonArray(_items):
            return True
        case _:
            return False


def is_null(value):
    match value:
        case JsonNull:
            return True
        case _:
            return False


def child_path(path, step):
    """One instance path step, without a leading separator at the root."""
    return step if path == "" else f"{path}/{step}"


def violation(keyword, path, detail):
    """One schema violation as `keyword@path: detail`."""
    return f"{keyword}@{path}: {detail}"


def violation_keyword(message):
    """The JSON Schema keyword of a violation, or "" when the text names none."""
    index = unwrap_or(indexOf(message, "@"), -1)
    if index < 0:
        return ""
    else:
        return take(message, index)


def violation_path(message):
    """The instance path of a violation: the text between `@` and the detail."""
    at = unwrap_or(indexOf(message, "@"), -1)
    stop = unwrap_or(indexOf(message, ": "), -1)
    if at < 0 or stop < at:
        return ""
    else:
        return unwrap_or(substring(message, at + 1, stop), "")


def validate(schema, value):
    """`jsonschema`'s verdict on `value` for the ported schema subset."""
    return validate_at(schema, value, "")


def validate_at(schema, value, path):
    """One value against one schema, naming the first violation in jsonschema's
    order: `type`, then `enum`, then `items`, then `properties`, then `required`,
    then `additionalProperties`."""
    declared = kind(schema)
    if kind_matches(declared, value) == False:
        return Error(message=violation("type", path, f"want {declared}"))
    else:
        return after_type(schema, declared, value, path)


def after_type(schema, declared, value, path):
    """Everything a schema checks once the value's type is right."""
    verdict = enum_verdict(schema, value, path)
    if failed(verdict):
        return verdict
    else:
        return shape_verdict(schema, declared, value, path)


def enum_verdict(schema, value, path):
    """`enum` as a verdict: Success when the value is a member or the enum is absent."""
    allowed = enum_values(schema)
    if len(allowed) == 0 or contains_value(allowed, value):
        return Success(value=None)
    else:
        return Error(message=violation("enum", path, "not a declared member"))


def contains_value(values, value):
    """Whether `values` holds `value` by its JSON text, so an int matches an int."""
    wanted = unwrap_or(jsonEncode(value), "")
    return foldList(values, False, lambda found, candidate: found or unwrap_or(jsonEncode(candidate), "?") == wanted)


def failed(verdict):
    """True when a verdict is already an error, so a fold keeps the first one."""
    match verdict:
        case Error(_rejected):
            return True
        case _:
            return False


def shape_verdict(schema, declared, value, path):
    """The container rules: `items` for an array, the object rules for an object."""
    match declared:
        case "array":
            return validate_items(items_schema(schema), array_items(value), path)
        case "object":
            return validate_object(schema, value, path)
        case _:
            return Success(value=None)


def validate_items(schema, values, path):
    """`items` over one array's elements, the first violation winning."""
    return fold(
        range(len(values)),
        Success(value=None),
        lambda verdict, i: verdict if failed(verdict) else validate_at(schema, unwrap_or(listGet(values, i), JsonNull), child_path(path, toString(i))),
    )


def validate_object(schema, value, path):
    """Object rules in jsonschema's order: declared properties, then `required`,
    then `additionalProperties`."""
    present = object_members(value)
    checked = foldList(
        members(schema),
        Success(value=None),
        lambda verdict, declared: verdict if failed(verdict) else validate_member(declared, present, path),
    )
    return checked if failed(checked) else object_presence(schema, present, path)


def validate_member(declared, present, path):
    """One declared property: its value checked against its schema when present."""
    found = filterList(present, lambda member: member_name(member) == member_name(declared))
    if len(found) == 0:
        return Success(value=None)
    else:
        return validate_at(member_value(declared), member_value(unwrap_or(listGet(found, 0), declared)), child_path(path, member_name(declared)))


def object_presence(schema, present, path):
    """`required` and `additionalProperties` of one object value."""
    missing = filterList(required_names(schema), lambda name: declared_property(present, name) == False)
    extra = filterList(present, lambda member: declares(schema, member_name(member)) == False)
    if len(missing) > 0:
        return Error(message=violation("required", path, f"missing {unwrap_or(listGet(missing, 0), '')}"))
    elif allows_extra(schema) or len(extra) == 0:
        return Success(value=None)
    else:
        return Error(message=violation("additionalProperties", path, f"undeclared {member_name(unwrap_or(listGet(extra, 0), JsonMember('', JsonNull)))}"))


def declared_property(present, name):
    return len(filterList(present, lambda member: member_name(member) == name)) > 0


def declares(schema, name):
    """Whether an object schema declares the property `name` at all."""
    return is_null(property_schema(schema, name)) == False


# --- coercion -------------------------------------------------------------

def coerce(schema, text):
    """One parameter's wire text as a `Json` value of its declared type.

    A string argument is the text itself; every other type reads one JSON
    literal. The coerced value is then validated, so an enum outside the declared
    members, an undeclared nested property or a wrongly typed literal is an error
    and never a substituted value."""
    if kind(schema) == "string":
        return accepted(schema, JsonString(text))
    else:
        return coerce_literal(schema, text)


def coerce_literal(schema, text):
    """A non-string parameter: one JSON literal, checked by `validate`."""
    match jsonDecode(text):
        case Success(value=value):
            return accepted(schema, value)
        case _:
            return Error(message=violation("type", "", f"{shortened(text)} is not a JSON literal"))


def accepted(schema, value):
    """`value` as a result once the schema accepts it, else the violation."""
    match validate(schema, value):
        case Success(_accepted):
            return Success(value=value)
        case Error(message=message):
            return Error(message=message)


def shortened(text):
    """`text` quoted for a message, capped so a huge value cannot swamp it."""
    return unwrap_or(jsonQuote(take(text, 80)), "value")
