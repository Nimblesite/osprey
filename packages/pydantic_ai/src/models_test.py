# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=ModelsTest
"""pydantic_ai.models.test: `TestModel`, the deterministic model for tests.

Port of pydantic_ai/models/test.py (2.46.0). `TestModel(custom_output_text=,
call_tools=)` calls every allowed function tool once with arguments generated
from its JSON schema (`_JsonSchemaTestData` is `gen_any` and its helpers,
threading the seed through a `Gen` record instead of mutating `self`), then
answers the custom text, or the JSON of every tool return, or
`success (no tool calls)`. Output tools, structured output and streaming are
not ported.
"""
from pydantic_ai import Exceptions as E
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from pydantic_ai import Models
from pydantic_ai import Profiles
from pydantic_ai import Usage

__all__ = ["TestModel", "test_model", "gen_tool_args", "gen_any", "Gen", "char_of"]

CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
"""`string.ascii_letters + string.digits + string.punctuation`."""


class Gen:
    """A generated value and the seed after generating it."""
    value: Json
    seed: int


def TestModel(custom_output_text, call_tools):
    """`TestModel(custom_output_text=..., call_tools=...)`: empty text means CPython's `None`; `call_tools` names the tools to call, `["all"]` for every tool."""
    return test_model("test", custom_output_text, call_tools, 0)


def test_model(model_name, custom_output_text, call_tools, seed):
    """A `TestModel` with an explicit model name and seed."""
    return Models.model(model_name, "test", Profiles.model_profile(), lambda messages, _settings, parameters: request(model_name, custom_output_text, call_tools, seed, messages, parameters))


def calls_all(call_tools):
    return len(call_tools) == 1 and unwrap_or(listGet(call_tools, 0), "") == "all"


def tools_to_call(call_tools, parameters) -> Result[list[Json], str]:
    """`_get_tool_calls`: the tool definitions to call, in `call_tools` order."""
    defs = parameters.function_tools
    if calls_all(call_tools):
        return Success(value=mapList(defs, lambda d: JsonString(d.name)))
    else:
        unknown = filterList(call_tools, lambda name: len(filterList(defs, lambda d: d.name == name)) == 0)
        return Error(message=f"TestModel was configured to call unknown tool '{unwrap_or(listGet(unknown, 0), "")}'.") if len(unknown) > 0 else Success(value=mapList(call_tools, lambda n: JsonString(n)))


def schema_of(parameters, name):
    found = filterList(parameters.function_tools, lambda d: d.name == name)
    match listGet(found, 0):
        case Success(value=d):
            return d.parameters_json_schema
        case Error(message=_m):
            return JsonNull()


def tool_call_for(parameters, seed, name):
    return M.ToolCallPart(tool_name=name, args=J.encode(gen_tool_args(schema_of(parameters, name), seed)), tool_call_id=f"pyd_ai_tool_call_id__{name}")


def retry_names(messages):
    """The tool names of the retry prompts in the last request."""
    last = foldList(M.requests(messages), M.ModelRequest(parts=[], instructions=""), lambda _acc, r: r)
    return filterList(mapList(last.parts, retry_name), lambda n: n != "")


def retry_name(part):
    match part:
        case M.RetryPromptPart(tool_name=tool_name):
            return tool_name
        case M.SystemPromptPart():
            return ""
        case M.UserPromptPart():
            return ""
        case M.ToolReturnPart():
            return ""
        case M.InstructionPart():
            return ""


def tool_returns(messages):
    """`{tool_name: content}` over every tool return in the history (a repeated name keeps its last content)."""
    returns = foldList(M.requests(messages), [], lambda acc, r: listConcat(acc, filterList(mapList(r.parts, return_member), lambda m: J.member_name(m) != "")))
    return J.members_of(J.of_map(foldList(returns, {}, lambda acc, m: mapSet(acc, J.member_name(m), J.member_value(m)))))


def return_member(part):
    match part:
        case M.ToolReturnPart(tool_name=tool_name, content=content):
            return J.member(tool_name, JsonString(content))
        case M.SystemPromptPart():
            return J.member("", JsonNull())
        case M.UserPromptPart():
            return J.member("", JsonNull())
        case M.RetryPromptPart():
            return J.member("", JsonNull())
        case M.InstructionPart():
            return J.member("", JsonNull())


SEPARATORS = " \t\n\r\f\v\",.:"
"""`_TOKEN_SPLIT_PATTERN`: whitespace, quotes, commas, periods and colons split tokens."""


def is_separator(text, i):
    return contains(SEPARATORS, unwrap_or(substring(text, i, i + 1), "x"))


def estimate_string_tokens(text):
    """`_utils.estimate_string_tokens`: pieces after splitting the stripped text on separator runs; empty text is 0 tokens."""
    stripped = trim(text)
    if text == "":
        return 0
    else:
        runs = fold(range(length(stripped)), 0, lambda n, i: n + 1 if is_separator(stripped, i) and (i == 0 or not is_separator(stripped, i - 1)) else n)
        return runs + 1


def request_part_tokens(part):
    match part:
        case M.SystemPromptPart(content=content):
            return estimate_string_tokens(content)
        case M.UserPromptPart(content=content):
            return estimate_string_tokens(content)
        case M.ToolReturnPart(content=content):
            return estimate_string_tokens(content)
        case M.RetryPromptPart():
            return estimate_string_tokens(M.retry_model_response(part))
        case M.InstructionPart():
            return 0


def response_part_tokens(part):
    match part:
        case M.TextPart(content=content):
            return estimate_string_tokens(content)
        case M.ThinkingPart(content=content):
            return estimate_string_tokens(content)
        case M.ToolCallPart():
            return 1 + estimate_string_tokens(M.args_as_json_str(part))


def estimate_usage(messages, response) -> Usage.RequestUsage:
    """`_estimate_usage` over the history plus the new response: 50 input tokens plus every request part, output tokens over every response part."""
    requests_tokens = foldList(M.requests(messages), 50, lambda n, r: n + foldList(r.parts, 0, lambda m, p: m + request_part_tokens(p)))
    all_responses = listAppend(M.responses(messages), response)
    response_tokens = foldList(all_responses, 0, lambda n, r: n + foldList(r.parts, 0, lambda m, p: m + response_part_tokens(p)))
    zero = Usage.request_usage()
    return Usage.RequestUsage(input_tokens=requests_tokens, cache_write_tokens=0, cache_read_tokens=0, output_tokens=response_tokens, input_audio_tokens=0, cache_audio_read_tokens=0, output_audio_tokens=0, details=zero.details)


def with_usage(messages, response) -> M.ModelResponse:
    """`TestModel.request`'s usage and provider name on a `_request` response."""
    return M.ModelResponse(parts=response.parts, usage=estimate_usage(messages, response), model_name=response.model_name, provider_name="test", provider_response_id=response.provider_response_id, finish_reason=response.finish_reason)


def request(model_name, custom_output_text, call_tools, seed, messages, parameters):
    """`TestModel.request`: `_request` plus the estimated usage."""
    match raw_request(model_name, custom_output_text, call_tools, seed, messages, parameters):
        case E.Ok(value=response):
            return E.Ok(value=with_usage(messages, response))
        case E.Failed(error=error):
            return E.Failed(error=error)


def raw_request(model_name, custom_output_text, call_tools, seed, messages, parameters) -> E.Outcome[M.ModelResponse]:
    """`TestModel._request`."""
    match tools_to_call(call_tools, parameters):
        case Success(value=names):
            calls = mapList(names, lambda n: unwrap_or(jsonText(n), ""))
            retries = retry_names(messages)
            if len(calls) > 0 and len(M.responses(messages)) == 0:
                return E.Ok(value=M.model_response(mapList(calls, lambda n: tool_call_for(parameters, seed, n)), model_name))
            elif len(retries) > 0:
                return E.Ok(value=M.model_response(mapList(filterList(calls, lambda n: listContains(retries, n)), lambda n: tool_call_for(parameters, seed, n)), model_name))
            elif custom_output_text != "":
                return E.Ok(value=M.model_response([M.TextPart(content=custom_output_text)], model_name))
            else:
                returns = tool_returns(messages)
                output = J.encode(J.obj(returns)) if len(returns) > 0 else "success (no tool calls)"
                return E.Ok(value=M.model_response([M.TextPart(content=output)], model_name))
        case Error(message=message):
            return E.Failed(error=E.UserError(message=message))


def gen_tool_args(schema, seed) -> Json:
    """`TestModel.gen_tool_args`: arguments matching the tool's parameter schema."""
    return gen_any(schema, schema, seed).value


def char_of(seed) -> str:
    """`_char`: a name on the Excel-column principle, a-z, aa-az..."""
    n = len(CHARS)
    rem = intDiv(seed, n)
    return char_prefix(rem, n, "") + char_at(seed % n)


def char_at(i):
    return unwrap_or(substring(CHARS, i, i + 1), "a")


def char_prefix(rem, n, acc):
    return acc if rem <= 0 else char_prefix(intDiv(rem, n), n, char_at((rem - 1) % n) + acc)


def pick(items, seed):
    return unwrap_or(listGet(items, seed % len(items)), JsonNull())


def ref_name(ref):
    return unwrap_or(replace(ref, "#/$defs/", ""), ref)


def gen_any(root, schema, seed) -> Gen:
    """`_gen_any`: a value for any JSON schema."""
    enum = J.list_at(schema, "enum")
    examples = J.list_at(schema, "examples")
    any_of = J.list_at(schema, "anyOf")
    ref = J.text_at(schema, "$ref", "")
    if not J.is_null(J.object_at(schema, "const")):
        return Gen(value=J.object_at(schema, "const"), seed=seed)
    elif len(enum) > 0:
        return Gen(value=pick(enum, seed), seed=seed)
    elif len(examples) > 0:
        return Gen(value=pick(examples, seed), seed=seed)
    elif ref != "":
        return gen_any(root, J.object_at(J.object_at(root, "$defs"), ref_name(ref)), seed)
    elif len(any_of) > 0:
        return gen_any(root, pick(any_of, seed), seed)
    else:
        return gen_typed(root, schema, J.text_at(schema, "type", ""), seed)


def gen_typed(root, schema, type_, seed):
    if type_ == "object":
        return gen_object(root, schema, seed)
    elif type_ == "string":
        return Gen(value=JsonString(gen_str(schema, seed)), seed=seed)
    elif type_ == "integer":
        return Gen(value=JsonInt(gen_int(schema, seed)), seed=seed)
    elif type_ == "number":
        return Gen(value=JsonFloat(gen_number(schema, seed)), seed=seed)
    elif type_ == "boolean":
        return Gen(value=JsonBool(seed % 2 == 1), seed=seed)
    elif type_ == "array":
        return gen_array(root, schema, seed)
    elif type_ == "null":
        return Gen(value=JsonNull(), seed=seed)
    else:
        return Gen(value=JsonString(char_of(seed)), seed=seed)


class ObjectFold:
    """Members generated so far and the running seed."""
    members: list[JsonMember]
    seed: int


def gen_object(root, schema, seed) -> Gen:
    """`_object_gen`: every required property, plus one `additionalProperty` when the schema allows extras."""
    required = mapList(J.list_at(schema, "required"), lambda r: unwrap_or(jsonText(r), ""))
    props = filterList(J.members_of(J.object_at(schema, "properties")), lambda m: listContains(required, J.member_name(m)))
    folded = foldList(props, ObjectFold(members=[], seed=seed), lambda acc, m: object_step(root, acc, m))
    additional = J.object_at(schema, "additionalProperties")
    match additional:
        case JsonBool(allowed):
            return Gen(value=J.obj(listAppend(folded.members, J.member("additionalProperty", JsonString(char_of(folded.seed)))) if allowed else folded.members), seed=folded.seed)
        case JsonObject():
            extra = gen_any(root, additional, folded.seed)
            return Gen(value=J.obj(listAppend(folded.members, J.member("additionalProperty", extra.value))), seed=extra.seed)
        case JsonNull():
            return Gen(value=J.obj(folded.members), seed=folded.seed)
        case JsonInt():
            return Gen(value=J.obj(folded.members), seed=folded.seed)
        case JsonFloat():
            return Gen(value=J.obj(folded.members), seed=folded.seed)
        case JsonString():
            return Gen(value=J.obj(folded.members), seed=folded.seed)
        case JsonArray():
            return Gen(value=J.obj(folded.members), seed=folded.seed)


def object_step(root, acc, m) -> ObjectFold:
    generated = gen_any(root, J.member_value(m), acc.seed)
    return ObjectFold(members=listAppend(acc.members, J.member(J.member_name(m), generated.value)), seed=generated.seed)


def gen_str(schema, seed):
    """`_str_gen`: `minLength` copies of the seed's character, empty for `maxLength` 0, else the character."""
    min_len = J.int_at(schema, "minLength", -1)
    if min_len >= 0:
        return unwrap_or(repeat(char_of(seed), min_len), "")
    elif J.int_at(schema, "maxLength", -1) == 0:
        return ""
    else:
        return char_of(seed)


def gen_int(schema, seed):
    """`_int_gen`: within the schema's bounds, offset by the seed."""
    has_max = not J.is_null(J.object_at(schema, "maximum"))
    has_min = not J.is_null(J.object_at(schema, "minimum"))
    has_exc_max = not J.is_null(J.object_at(schema, "exclusiveMaximum"))
    has_exc_min = not J.is_null(J.object_at(schema, "exclusiveMinimum"))
    maximum = J.int_at(schema, "maximum", J.int_at(schema, "exclusiveMaximum", 0) - 1)
    minimum = J.int_at(schema, "minimum", J.int_at(schema, "exclusiveMinimum", 0) + 1)
    max_known = has_max or has_exc_max
    min_known = has_min or has_exc_min
    if has_min and has_max and maximum == minimum:
        return minimum
    elif min_known and max_known:
        span = maximum - minimum
        inclusive = not has_exc_min and not has_exc_max and span > 0
        return minimum if span == 0 else minimum + seed % (span + 1 if inclusive else span)
    elif min_known:
        return minimum + seed
    elif max_known:
        return maximum - seed
    else:
        return seed


def gen_number(schema, seed):
    """`_number_gen`: within the schema's bounds, offset by the seed."""
    has_max = not J.is_null(J.object_at(schema, "maximum")) or not J.is_null(J.object_at(schema, "exclusiveMaximum"))
    has_min = not J.is_null(J.object_at(schema, "minimum")) or not J.is_null(J.object_at(schema, "exclusiveMinimum"))
    has_exc_min = not J.is_null(J.object_at(schema, "exclusiveMinimum"))
    has_exc_max = not J.is_null(J.object_at(schema, "exclusiveMaximum"))
    minimum = bound_min(schema)
    maximum = bound_max(schema)
    if has_min and has_max:
        span = maximum - minimum
        value = minimum + toFloat(seed) % span if minimum != maximum and span > 0.0 else minimum
        return minimum + span / 2.0 if has_exc_min and value == J.float_at(schema, "exclusiveMinimum", value - 1.0) else value
    elif has_min:
        return minimum + toFloat(seed) + (1.0 if has_exc_min else 0.0)
    elif has_max:
        return maximum - toFloat(seed) - (1.0 if has_exc_max else 0.0)
    else:
        return toFloat(seed)


def bound_min(schema):
    minimum = J.float_at(schema, "minimum", 0.0)
    exclusive = J.float_at(schema, "exclusiveMinimum", 0.0)
    if J.is_null(J.object_at(schema, "exclusiveMinimum")):
        return minimum
    elif J.is_null(J.object_at(schema, "minimum")):
        return exclusive
    else:
        return minimum if minimum > exclusive else exclusive


def bound_max(schema):
    maximum = J.float_at(schema, "maximum", 0.0)
    exclusive = J.float_at(schema, "exclusiveMaximum", 0.0)
    if J.is_null(J.object_at(schema, "exclusiveMaximum")):
        return maximum
    elif J.is_null(J.object_at(schema, "maximum")):
        return exclusive
    else:
        return maximum if maximum < exclusive else exclusive


class ArrayFold:
    """Items generated so far and the running seed."""
    items: list[Json]
    seed: int


def gen_array(root, schema, seed) -> Gen:
    """`_array_gen`: the prefix items, then enough `items` to reach `minItems`, else one item under `maxItems`."""
    unique = J.bool_at(schema, "uniqueItems", False)
    prefix = foldList(J.list_at(schema, "prefixItems"), ArrayFold(items=[], seed=seed), lambda acc, item: array_step(root, acc, item, unique))
    items_schema = J.object_at(schema, "items")
    min_items = J.int_at(schema, "minItems", 0)
    max_items = J.int_at(schema, "maxItems", -1)
    if min_items > len(prefix.items):
        filled = fold(range(min_items - len(prefix.items)), prefix, lambda acc, _i: array_step(root, acc, items_schema, unique))
        return Gen(value=JsonArray(filled.items), seed=filled.seed)
    elif not J.is_null(items_schema) and (max_items < 0 or max_items > len(prefix.items)):
        one = array_step(root, prefix, items_schema, unique)
        return Gen(value=JsonArray(one.items), seed=one.seed)
    else:
        return Gen(value=JsonArray(prefix.items), seed=prefix.seed)


def array_step(root, acc, item_schema, unique) -> ArrayFold:
    generated = gen_any(root, item_schema, acc.seed)
    return ArrayFold(items=listAppend(acc.items, generated.value), seed=generated.seed + 1 if unique else generated.seed)
