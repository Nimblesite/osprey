# osprey: flavor=python
"""@osprey/pydantic-ai's suite: values the real pydantic-ai 2.46.0 produced over
an httpx mock transport (src/osprey/packages/pydantic_ai/oracle/pydantic_ai.cpython), compared against the
port through @osprey/testing. Covers the Chat Completions request body for
eight scenarios, response parsing, `ModelResponse.text`, tool-call argument
helpers, retry prompts, the message wire form both ways, TestModel's
schema-driven arguments and agent runs, a scripted multi-step tool-calling
agent run on OpenAIChatModel (bodies, history, usage, a tool retry), HTTP and
usage-limit errors, profiles, settings, direct and streamed requests. The live
DeepSeek run lives in ../live."""
from pydantic_ai import Agent
from pydantic_ai import Direct
from pydantic_ai import Exceptions as E
from pydantic_ai import JsonSchema
from pydantic_ai import JsonUtil as J
from pydantic_ai import Mcp
from pydantic_ai import Messages as M
from pydantic_ai import Models
from pydantic_ai import ModelsOpenai
from pydantic_ai import ModelsTest
from pydantic_ai import Profiles
from pydantic_ai import Providers
from pydantic_ai import ProvidersHttp
from pydantic_ai import Settings
from pydantic_ai import Tools
from pydantic_ai import Usage
from pydantic_ai_expected import Expected
from testing import Check


def ats(xs, i):
    """The string element of a list at i, "" past the end."""
    return unwrap_or(listGet(xs, i), "")


def ati(xs, i):
    """The int element of a list at i, 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def atb(xs, i):
    """The bool element of a list at i, False past the end."""
    return unwrap_or(listGet(xs, i), False)


def atl(xs, i):
    """The list element of a list at i, [] past the end."""
    return unwrap_or(listGet(xs, i), [])


def same(name, got, want):
    """Structural equality of two lists whose elements the report cannot print."""
    return Check.truth(name, structuralEquals(got, want), "values differ")


def cases(n, f):
    """The outcomes of f(i) for i in 0..n-1."""
    return toList(map(range(n), f))


def concat(groups):
    """Flatten a list of outcome lists into one."""
    return foldList(groups, [], listConcat)


def decode(text):
    """The JSON a text spells, null when it does not parse."""
    return unwrap_or(jsonDecode(text), JsonNull())


def canonical(value):
    """The value with every object's members sorted by name, the form the oracle compares."""
    match value:
        case JsonObject(members):
            names = listSort(mapList(members, J.member_name))
            table = J.to_map(value)
            return JsonObject(mapList(names, lambda name: JsonMember(name, canonical(unwrap_or(mapGet(table, name), JsonNull())))))
        case JsonArray(items):
            return JsonArray(mapList(items, canonical))
        case JsonNull():
            return value
        case JsonBool():
            return value
        case JsonInt():
            return value
        case JsonFloat():
            return value
        case JsonString():
            return value


def canon(value):
    """Sorted-key compact JSON text."""
    return J.encode(canonical(value))


# Fixtures mirroring the oracle

def schema(text):
    return decode(text)


TEMPERATURE_SCHEMA = "{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\",\"description\":\"The city name\"}},\"required\":[\"city\"]}"
ADD_SCHEMA = "{\"type\":\"object\",\"properties\":{\"a\":{\"type\":\"integer\"},\"b\":{\"type\":\"integer\"}},\"required\":[\"a\",\"b\"]}"
LOOSE_SCHEMA = "{\"type\":\"object\",\"properties\":{\"q\":{\"type\":\"string\",\"minLength\":2},\"n\":{\"type\":\"integer\"}},\"required\":[\"q\"]}"


def tools():
    return [
        Tools.tool_definition("get_temperature", "The current temperature of a city in degrees Celsius.", schema(TEMPERATURE_SCHEMA)),
        Tools.tool_definition("add", "Adds two numbers.", schema(ADD_SCHEMA)),
        Tools.tool_definition("search", "", schema(LOOSE_SCHEMA)),
    ]


def two_of(xs, start):
    """The tool definition at `start` and the one after it, blank ones past the end."""
    return toList(map(range(start, start + 2), lambda i: unwrap_or(listGet(xs, i), Tools.tool_definition("", "", JsonNull()))))


def plain():
    return [M.of_request(M.ModelRequest(parts=[M.SystemPromptPart(content="sys"), M.UserPromptPart(content="hello")], instructions=""))]


def history():
    return [
        M.of_request(M.ModelRequest(parts=[M.SystemPromptPart(content="sys"), M.UserPromptPart(content="hello")], instructions="")),
        M.of_response(M.model_response([M.ThinkingPart(content="hmm"), M.TextPart(content="calling"), M.ToolCallPart(tool_name="add", args="{\"a\": 1, \"b\": 2}", tool_call_id="c1")], "")),
        M.of_request(M.ModelRequest(parts=[M.ToolReturnPart(tool_name="add", content="3", tool_call_id="c1"), M.RetryPromptPart(content="bad", tool_name="add", tool_call_id="c2"), M.RetryPromptPart(content="plain", tool_name="", tool_call_id="c0")], instructions="be brief")),
        M.of_response(M.model_response([M.TextPart(content="first"), M.TextPart(content="second"), M.ToolCallPart(tool_name="get_temperature", args="", tool_call_id="c3")], "")),
        M.of_request(M.ModelRequest(parts=[M.ToolReturnPart(tool_name="get_temperature", content="21", tool_call_id="c3"), M.UserPromptPart(content="and?")], instructions="")),
    ]


def openai_model(name, profile):
    return ModelsOpenai.OpenAIChatModel(name, ProvidersHttp.OpenAIProvider("http://x/v1", "k"), profile)


def params_with(function_tools, allow_text, instruction_parts):
    return Models.ModelRequestParameters(function_tools=function_tools, allow_text_output=allow_text, instruction_parts=instruction_parts)


class Scenario:
    """One request scenario: the model, its history, settings and parameters."""
    model: Models.Model
    messages: list[M.ModelMessage]
    settings: dict[str, Json]
    parameters: Models.ModelRequestParameters


def scenario(i):
    name = ats(Expected.SCENARIO_NAMES, i)
    if name == "plain":
        return Scenario(model=openai_model("gpt-4o", {}), messages=plain(), settings={}, parameters=Models.model_request_parameters())
    elif name == "tools_settings":
        settings = {"max_tokens": JsonInt(50), "temperature": JsonFloat(0.2), "top_p": JsonFloat(0.9), "seed": JsonInt(7), "stop_sequences": JsonArray([JsonString("END")]), "presence_penalty": JsonFloat(0.1), "frequency_penalty": JsonFloat(0.2), "extra_body": JsonObject([JsonMember("foo", JsonInt(1))])}
        return Scenario(model=openai_model("gpt-4o", {}), messages=plain(), settings=settings, parameters=Models.with_tools(tools()))
    elif name == "no_max_completion":
        return Scenario(model=openai_model("gpt-4o", {"openai_chat_supports_max_completion_tokens": JsonBool(False)}), messages=plain(), settings={"max_tokens": JsonInt(10)}, parameters=Models.model_request_parameters())
    elif name == "history":
        return Scenario(model=openai_model("gpt-4o", {}), messages=history(), settings={}, parameters=params_with(two_of(tools(), 0), True, [M.InstructionPart(content="inst")]))
    elif name == "deepseek_history":
        return Scenario(model=ModelsOpenai.OpenAIChatModel("deepseek-chat", ProvidersHttp.DeepSeekProvider("k"), {}), messages=history(), settings={}, parameters=Models.with_tools(two_of(tools(), 0)))
    elif name == "o1_mini":
        return Scenario(model=openai_model("o1-mini", {}), messages=plain(), settings={"temperature": JsonFloat(0.5), "max_tokens": JsonInt(5), "openai_reasoning_effort": JsonString("low")}, parameters=Models.model_request_parameters())
    elif name == "ollama":
        return Scenario(model=ModelsOpenai.OpenAIChatModel("llama3", ProvidersHttp.OllamaProvider("http://localhost:11434/v1"), {}), messages=plain(), settings={}, parameters=Models.with_tools(two_of(tools(), 1)))
    else:
        return Scenario(model=openai_model("gpt-4o", {}), messages=plain(), settings={}, parameters=params_with([unwrap_or(listGet(tools(), 0), Tools.tool_definition("", "", JsonNull()))], False, []))


def body_case(i):
    s = scenario(i)
    body = ModelsOpenai.completions_body(s.model.model_name, s.model.profile, s.messages, s.settings, s.parameters)
    return Check.equal(f"request.{ats(Expected.SCENARIO_NAMES, i)}", canon(body), ats(Expected.REQUEST_BODIES, i))


# Response parsing

def describe_part(part):
    match part:
        case M.TextPart(content=content):
            return f"text:{content}"
        case M.ThinkingPart(content=content):
            return f"thinking:{content}"
        case M.ToolCallPart(tool_name=tool_name, args=args, tool_call_id=tool_call_id):
            return f"tool-call:{tool_name}|{args}|{tool_call_id}"


def describe(response):
    details = join(mapList(listSort(mapKeys(response.usage.details)), lambda k: f"{k}={unwrap_or(mapGet(response.usage.details, k), 0)}"), ";")
    return [join(mapList(response.parts, describe_part), "|"), toString(response.usage.input_tokens), toString(response.usage.output_tokens), toString(response.usage.cache_read_tokens), details, response.model_name, response.provider_response_id, response.finish_reason]


def parse_case(i):
    provider = ats(Expected.REPLY_PROVIDERS, i)
    model = ModelsOpenai.OpenAIChatModel("deepseek-chat", ProvidersHttp.DeepSeekProvider("k"), {}) if provider == "deepseek" else openai_model("gpt-4o", {})
    expected_error = ats(Expected.PARSED_ERRORS, i)
    match ModelsOpenai.process_response(model.model_name, provider, model.profile, ats(Expected.REPLY_BODIES, i)):
        case E.Ok(value=response):
            return Check.truth(f"parse[{i}]", expected_error == "" and structuralEquals(describe(response), atl(Expected.PARSED_REPLIES, i)), join(describe(response), " / "))
        case E.Failed(error=error):
            return Check.truth(f"parse[{i}]", expected_error != "" and contains(E.message_of(error), "Invalid response"), E.message_of(error))


# ModelResponse.text, args, retries

def part_of(spec):
    kind = ats(unwrap_or(split(spec, ":"), []), 0)
    content = drop(spec, length(kind) + 1)
    if kind == "text":
        return M.TextPart(content=content)
    elif kind == "thinking":
        return M.ThinkingPart(content=content)
    else:
        return M.ToolCallPart(tool_name=content, args="{}", tool_call_id="")


def text_case(i):
    spec = ats(atl(Expected.TEXT_PARTS, i), 0)
    parts = [] if spec == "" else mapList(unwrap_or(split(spec, "|"), []), part_of)
    return Check.equal(f"text[{i}]", M.text(M.model_response(parts, "")), ats(Expected.TEXT_EXPECTED, i))


def args_cases(i):
    part = M.ToolCallPart(tool_name="f", args=ats(Expected.ARGS_INPUTS, i), tool_call_id="")
    return [
        Check.equal(f"args_as_dict[{i}]", canon(J.of_map(M.args_as_dict(part))), ats(Expected.ARGS_AS_DICT, i)),
        Check.equal(f"args_as_json_str[{i}]", M.args_as_json_str(part), ats(Expected.ARGS_AS_JSON, i)),
    ]


def retry_case(i):
    inputs = atl(Expected.RETRY_INPUTS, i)
    part = M.RetryPromptPart(content=ats(inputs, 0), tool_name=ats(inputs, 1), tool_call_id="x")
    return Check.equal(f"retry[{i}]", M.retry_model_response(part), ats(Expected.RETRY_EXPECTED, i))


# Wire form

def dump_case(i):
    dumped = J.items_of(M.dump_messages(history()))
    return Check.equal(f"dump[{i}]", canon(unwrap_or(listGet(dumped, i), JsonNull())), ats(Expected.DUMP_PROJECTED, i))


def validate_cases():
    match M.validate_messages(decode(Expected.DUMP_FULL)):
        case Success(value=messages):
            redumped = J.items_of(M.dump_messages(messages))
            return listPrepend(cases(len(Expected.DUMP_PROJECTED), lambda i: Check.equal(f"validate[{i}]", canon(unwrap_or(listGet(redumped, i), JsonNull())), ats(Expected.DUMP_PROJECTED, i))), Check.equal("validate.count", len(messages), len(Expected.DUMP_PROJECTED)))
        case Error(message=message):
            return [Check.failed("validate", message)]


def validate_error_cases():
    bad_part = M.validate_messages(decode("[{\"kind\":\"request\",\"parts\":[{\"part_kind\":\"mystery\"}]}]"))
    bad_kind = M.validate_messages(decode("[{\"kind\":\"other\"}]"))
    return [
        Check.equal("validate.bad_part", error_text(bad_part), "unknown request part_kind: mystery"),
        Check.equal("validate.bad_kind", error_text(bad_kind), "unknown message kind: other"),
        Check.equal("json_util.rewrap.error", error_text(J.rewrap(bad_kind, lambda messages: len(messages))), "unknown message kind: other"),
        Check.equal("json_util.rewrap.value", ok_value(J.rewrap(M.validate_messages(decode("[]")), lambda messages: len(messages) + 3), -1), 3),
        Check.ints_equal("messages.map_indexed", M.map_indexed(["a", "b"], lambda i: i * 5), [0, 5]),
    ]


def ok_value(result, fallback):
    match result:
        case Success(value=value):
            return value
        case Error(message=_m):
            return fallback


def error_text(result):
    match result:
        case Success(value=_v):
            return ""
        case Error(message=message):
            return message


# TestModel

def gen_case(i):
    return Check.equal(f"gen[{i}]", canon(ModelsTest.gen_tool_args(decode(ats(Expected.GEN_SCHEMAS, i)), ati(Expected.GEN_SEEDS, i))), ats(Expected.GEN_EXPECTED, i))


def temperature(args, retry):
    city = toLowerCase(unwrap_or(jsonText(unwrap_or(mapGet(args, "city"), JsonNull())), ""))
    if city == "paris":
        return Success(value="21")
    elif retry:
        return Error(message=f"Unknown city {unwrap_or(jsonText(unwrap_or(mapGet(args, "city"), JsonNull())), "")}; known cities are Paris and London")
    else:
        return Success(value="unknown")


def add(args):
    return Success(value=toString(J.int_at(J.of_map(args), "a", 0) + J.int_at(J.of_map(args), "b", 0)))


def build(model, retry):
    a = Agent.agent(model, "sys")
    with_temperature = Agent.tool_plain(a, Tools.tool("get_temperature", "The current temperature of a city in degrees Celsius.", decode(ats(Expected.TEST_AGENT_SCHEMAS, 0)), lambda args: temperature(args, retry)))
    return Agent.tool_plain(with_temperature, Tools.tool("add", "Adds two numbers.", decode(ats(Expected.TEST_AGENT_SCHEMAS, 1)), add))


def dumped_messages(messages):
    return mapList(J.items_of(M.dump_messages(messages)), canon)


def test_agent_cases():
    match Agent.run_sync(build(ModelsTest.TestModel("", ["all"]), False), "go", []):
        case E.Ok(value=result):
            return listConcat([
                Check.equal("test_agent.output", result.output, Expected.TEST_AGENT_OUTPUT),
                Check.equal("test_agent.message_count", len(result.messages), len(Expected.TEST_AGENT_MESSAGES)),
                Check.ints_equal("test_agent.usage", [result.usage.requests, result.usage.tool_calls], Expected.TEST_AGENT_USAGE),
                Check.equal("test_agent.new_messages", len(Agent.new_messages(result)), len(result.messages)),
            ], cases(len(Expected.TEST_AGENT_MESSAGES), lambda i: Check.equal(f"test_agent.messages[{i}]", ats(dumped_messages(result.messages), i), ats(Expected.TEST_AGENT_MESSAGES, i))))
        case E.Failed(error=error):
            return [Check.failed("test_agent", E.message_of(error))]


def test_agent_custom_cases():
    match Agent.run_sync(build(ModelsTest.TestModel("done", ["all"]), False), "go", []):
        case E.Ok(value=result):
            return [Check.equal("test_agent.custom_output", result.output, Expected.TEST_AGENT_CUSTOM_OUTPUT), Check.equal("test_agent.custom_requests", result.usage.requests, Expected.TEST_AGENT_CUSTOM_REQUESTS)]
        case E.Failed(error=error):
            return [Check.failed("test_agent.custom", E.message_of(error))]


def test_agent_no_tools_case():
    match Agent.run_sync(Agent.agent(ModelsTest.TestModel("", ["all"]), "sys"), "go", []):
        case E.Ok(value=result):
            return Check.equal("test_agent.no_tools", result.output, Expected.TEST_AGENT_NO_TOOLS_OUTPUT)
        case E.Failed(error=error):
            return Check.failed("test_agent.no_tools", E.message_of(error))


def test_agent_unknown_tool_case():
    match Agent.run_sync(build(ModelsTest.TestModel("", ["missing"]), False), "go", []):
        case E.Ok(value=_result):
            return Check.failed("test_agent.unknown_tool", "expected a UserError")
        case E.Failed(error=error):
            return Check.equal("test_agent.unknown_tool", E.message_of(error), "TestModel was configured to call unknown tool 'missing'.")


# Scripted OpenAIChatModel run

def replies(bodies, status):
    return mapList(bodies, lambda body: Providers.reply(status, body))


def scripted_model(bodies, status):
    return ModelsOpenai.OpenAIChatModel("gpt-4o", Providers.scripted("openai", replies(bodies, status)), {})


def prefix(messages, n):
    indexes = filterList(toList(map(range(len(messages)), lambda i: i)), lambda i: i < n)
    return mapList(indexes, lambda i: unwrap_or(listGet(messages, i), M.of_request(M.model_request([]))))


def scripted_cases():
    model = scripted_model(Expected.SCRIPTED_REPLIES, 200)
    agent = build(model, True)
    match Agent.run_sync(agent, "What is the temperature in Paris plus 10?", []):
        case E.Ok(value=result):
            parameters = Models.with_tools(mapList(agent.tools, Tools.tool_definition_of))
            bodies = cases(len(Expected.SCRIPTED_BODIES), lambda i: Check.equal(f"scripted.body[{i}]", canon(ModelsOpenai.completions_body("gpt-4o", model.profile, prefix(result.messages, 2 * i + 1), {}, parameters)), ats(Expected.SCRIPTED_BODIES, i)))
            messages = cases(len(Expected.SCRIPTED_MESSAGES), lambda i: Check.equal(f"scripted.messages[{i}]", ats(dumped_messages(result.messages), i), ats(Expected.SCRIPTED_MESSAGES, i)))
            return concat([[
                Check.equal("scripted.output", result.output, Expected.SCRIPTED_OUTPUT),
                Check.equal("scripted.message_count", len(result.messages), len(Expected.SCRIPTED_MESSAGES)),
                Check.ints_equal("scripted.usage", [result.usage.requests, result.usage.tool_calls, result.usage.input_tokens, result.usage.output_tokens], Expected.SCRIPTED_USAGE),
                Check.equal("scripted.total_tokens", Usage.run_total_tokens(result.usage), ati(Expected.SCRIPTED_USAGE, 2) + ati(Expected.SCRIPTED_USAGE, 3)),
            ], bodies, messages])
        case E.Failed(error=error):
            return [Check.failed("scripted", E.message_of(error))]


def status_error_cases():
    model = scripted_model(["{\"error\":{\"message\":\"rate limited\"}}"], 429)
    match Agent.run_sync(build(model, True), "go", []):
        case E.Ok(value=_result):
            return [Check.failed("status_error", "expected ModelHTTPError")]
        case E.Failed(error=error):
            return [
                Check.equal("status_error.status", E.status_code_of(error), Expected.HTTP_ERROR_STATUS),
                Check.equal("status_error.message", E.message_of(error), f"status_code: 429, model_name: {Expected.HTTP_ERROR_MODEL}, body: {{\"error\":{{\"message\":\"rate limited\"}}}}"),
                Check.equal("status_error.text", E.message_of(E.status_error(429, "gpt-4o", "{\"error\":\"x\"}")), Expected.HTTP_ERROR_MESSAGE),
            ]


def transport_error_case():
    provider = Providers.Provider(name="openai", base_url="x", api_key="", post=lambda _path, _body, _headers: Error(message="connection refused"))
    match Direct.model_request(ModelsOpenai.OpenAIChatModel("gpt-4o", provider, {}), plain(), {}, Models.model_request_parameters()):
        case E.Ok(value=_response):
            return Check.failed("transport_error", "expected ModelAPIError")
        case E.Failed(error=error):
            return Check.equal("transport_error", f"{E.message_of(error)}/{E.status_code_of(error)}", "connection refused/0")


HTTP_TEXTS = ["{\"ok\":1}", ""]
HTTP_READ_FAILURES = ["", "error httpResponseFree returned -1", "error httpResponseFree failed: unknown response handle"]
HTTP_CLOSE_OUTCOMES = ["reply 200 kept", "error httpCloseClient returned -1", "error httpCloseClient failed: unknown client"]


def http_outcome(result):
    match result:
        case Success(value=reply):
            return f"reply {reply.status} {reply.body}"
        case Error(message=message):
            return f"error {message}"


def http_code(k, missing):
    """Success(0), Success(-1) or Error(missing), the outcomes of a runtime free or close."""
    return Error(message=missing) if k == 2 else Success(value=-k)


def http_reply_case(i):
    """A failed body read or free is an Error, never a Success with an empty body."""
    status = ati([200, 429, 500], i // 9)
    body = (i // 3) % 3
    freed = i % 3
    text = Error(message="out of memory") if body == 2 else Success(value=ats(HTTP_TEXTS, body))
    clean = f"reply {status} {ats(HTTP_TEXTS, body)}" if body < 2 else "error httpResponseBody failed: out of memory"
    expected = clean if body == 2 or freed == 0 else ats(HTTP_READ_FAILURES, freed)
    return Check.equal(f"providers.http_reply_{i}", http_outcome(ProvidersHttp.reply_of(status, text, http_code(freed, "unknown response handle"))), expected)


def http_close_case(i):
    """A close failure fails a good request; a request's own error outranks the close."""
    request = Error(message="connection refused") if i >= 3 else Success(value=Providers.HttpReply(status=200, body="kept"))
    expected = ats(HTTP_CLOSE_OUTCOMES, i % 3) if i < 3 else "error connection refused"
    return Check.equal(f"providers.http_close_{i}", http_outcome(ProvidersHttp.after_close(request, http_code(i % 3, "unknown client"))), expected)


def usage_limit_case():
    model = scripted_model([ats(Expected.SCRIPTED_REPLIES, 0)], 200)
    match Agent.run_sync(build(model, False), "go", []):
        case E.Ok(value=_result):
            return Check.failed("usage_limit", "expected UsageLimitExceeded")
        case E.Failed(error=error):
            return Check.equal("usage_limit", E.message_of(error), Expected.USAGE_LIMIT_MESSAGE)


# Profiles, settings, usage, direct

def profile_cases():
    deepseek = Profiles.deepseek_model_profile("deepseek-reasoner")
    return [
        same("profile.flags", [
            Profiles.profile_bool(deepseek, "supports_thinking", False),
            Profiles.profile_bool(deepseek, "thinking_always_enabled", False),
            Profiles.profile_bool(deepseek, "openai_supports_tool_choice_required", True),
            Profiles.profile_bool(Profiles.ollama_model_profile("llama3"), "openai_supports_strict_tool_definition", True),
            Profiles.profile_bool(Profiles.openai_model_profile_with({"openai_chat_supports_max_completion_tokens": JsonBool(False)}), "openai_chat_supports_max_completion_tokens", True),
        ], Expected.PROFILE_FLAGS),
        same("profile.texts", [
            Profiles.profile_text(deepseek, "openai_chat_thinking_field", ""),
            Profiles.profile_text(deepseek, "openai_chat_send_back_thinking_parts", ""),
            Profiles.profile_text(Profiles.model_profile(), "openai_system_prompt_role", ""),
        ], Expected.PROFILE_TEXTS),
        Check.equal("profile.merge", Profiles.profile_bool(Profiles.merge_profile(Profiles.openai_model_profile("gpt-4o"), {"supports_tools": JsonBool(False)}), "supports_tools", True), False),
        Check.equal("profile.o1_mini_role", Profiles.profile_text(Profiles.openai_model_profile("o1-mini"), "openai_system_prompt_role", "system"), "user"),
    ]


def settings_cases():
    merged = Settings.merge_model_settings({"max_tokens": JsonInt(1), "temperature": JsonFloat(0.5)}, {"max_tokens": JsonInt(2)})
    return [
        Check.ints_equal("settings.merge", [Settings.setting_int(merged, "max_tokens", 0), Settings.setting_int(merged, "seed", 9)], [2, 9]),
        Check.close("settings.float", Settings.setting_float(merged, "temperature", 0.0), 0.5, 0.0, 0.0),
        Check.close("settings.int_as_float", Settings.setting_float(merged, "max_tokens", 0.0), 2.0, 0.0, 0.0),
        same("settings.has", [Settings.has_setting(merged, "temperature"), Settings.has_setting(Settings.model_settings(), "temperature")], [True, False]),
        Check.equal("settings.text", Settings.setting_text({"openai_reasoning_effort": JsonString("low")}, "openai_reasoning_effort", ""), "low"),
    ]


# Providers, profiles and the unified `thinking` setting: expected values read from
# upstream's providers/moonshotai.py, providers/zai.py, profiles/moonshotai.py,
# profiles/zai.py, models/zai.py, providers/deepseek.py and the thinking handling in
# models/openai.py and models/__init__.py (2.46.0), per docs/plans/0052-agent.md.

THINKING_VALUES = ["absent", "true", "false", "minimal", "low", "medium", "high", "xhigh"]
"""The unified `thinking` permutations, in THINKING_TABLE column order."""

THINKING_TABLE = [
    ["deepseek", "deepseek-chat", "||required;||required;||required;||required;||required;||required;||required;||required", ""],
    ["deepseek", "deepseek-reasoner", "||auto;medium||auto;||auto;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto", ""],
    ["deepseek", "deepseek-v4-flash", "||auto;medium||auto;none||required;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto", "off,on,minimal,low,medium,high,xhigh"],
    ["openai", "gpt-4o", "||required;||required;||required;||required;||required;||required;||required;||required", ""],
    ["openai", "o1-mini", "||required;medium||required;none||required;minimal||required;low||required;medium||required;high||required;xhigh||required", "off,minimal,low,medium,high,xhigh"],
    ["moonshotai", "kimi-k3", "||auto;medium||auto;||auto;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto", "minimal,low,medium,high"],
    ["moonshotai", "kimi-k2.6", "||auto;medium||auto;||auto;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto", "minimal,low,medium,high"],
    ["moonshotai", "kimi-thinking-preview", "||auto;medium||auto;||auto;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto", "minimal,low,medium,high"],
    ["moonshotai", "moonshot-v1-8k", "||auto;||auto;||auto;||auto;||auto;||auto;||auto;||auto", ""],
    ["zai", "glm-5.3", '|{"clear_thinking":false}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false}|required;low|{"clear_thinking":false,"type":"enabled"}|required;low|{"clear_thinking":false,"type":"enabled"}|required;high|{"clear_thinking":false,"type":"enabled"}|required;high|{"clear_thinking":false,"type":"enabled"}|required;max|{"clear_thinking":false,"type":"enabled"}|required', "low,high,max"],
    ["zai", "glm-5.2", '|{"clear_thinking":false}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"disabled"}|required;minimal|{"clear_thinking":false,"type":"enabled"}|required;low|{"clear_thinking":false,"type":"enabled"}|required;medium|{"clear_thinking":false,"type":"enabled"}|required;high|{"clear_thinking":false,"type":"enabled"}|required;xhigh|{"clear_thinking":false,"type":"enabled"}|required', "off,minimal,low,medium,high,xhigh"],
    ["zai", "glm-4.7", '|{"clear_thinking":false}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"disabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required', "off,on"],
    ["zai", "glm-5", '|{"clear_thinking":false}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"disabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required;|{"clear_thinking":false,"type":"enabled"}|required', "off,on"],
    ["zai", "glm-3-turbo", "||required;||required;||required;||required;||required;||required;||required;||required", ""],
]
"""Per model: the provider kind, the name, the eight `reasoning_effort|thinking|tool_choice` projections, and `thinking_levels`."""


def provider_of(kind):
    """The real provider of a kind, as the app would build it."""
    if kind == "deepseek":
        return ProvidersHttp.DeepSeekProvider("k")
    elif kind == "moonshotai":
        return ProvidersHttp.MoonshotAIProvider("k")
    elif kind == "zai":
        return ProvidersHttp.ZaiProvider("k")
    else:
        return ProvidersHttp.OpenAIProvider("", "k")


def one_tool():
    """One strict-compatible tool, so `tool_choice` and `strict` are exercised."""
    return unwrap_or(listGet(tools(), 0), Tools.tool_definition("", "", JsonNull()))


def thinking_settings(i):
    """The settings map of THINKING_VALUES[i]: absent, `true`, `false`, then each level."""
    values = [JsonNull(), JsonBool(True), JsonBool(False), JsonString("minimal"), JsonString("low"), JsonString("medium"), JsonString("high"), JsonString("xhigh")]
    value = unwrap_or(listGet(values, i), JsonNull())
    return {} if J.is_null(value) else {"thinking": value}


def body_projection(body):
    """The members a thinking permutation pins: `reasoning_effort`, `thinking`, `tool_choice`."""
    thinking = J.object_at(body, "thinking")
    thinking_json = "" if J.is_null(thinking) else canon(thinking)
    return J.text_at(body, "reasoning_effort", "") + "|" + thinking_json + "|" + J.text_at(body, "tool_choice", "")


def forced_tools():
    """One tool with text output disallowed, so a provider that supports forcing answers `tool_choice: required`."""
    return params_with([one_tool()], False, [])


def thinking_body_case(i):
    row = atl(THINKING_TABLE, i)
    model_name = ats(row, 1)
    model = ModelsOpenai.OpenAIChatModel(model_name, provider_of(ats(row, 0)), {})
    projections = join(cases(8, lambda j: body_projection(ModelsOpenai.completions_body(model_name, model.profile, plain(), thinking_settings(j), forced_tools()))), ";")
    return Check.equal(f"thinking.body.{model_name}", projections, ats(row, 2))


def thinking_levels_case(i):
    row = atl(THINKING_TABLE, i)
    model_name = ats(row, 1)
    profile = ModelsOpenai.OpenAIChatModel(model_name, provider_of(ats(row, 0)), {}).profile
    return Check.equal(f"thinking.levels.{model_name}", join(Profiles.thinking_levels(profile, ats(row, 0)), ","), ats(row, 3))


def thinking_setting_cases():
    return [
        Check.equal("thinking.setting.off", J.encode(Profiles.thinking_setting("off")), "false"),
        Check.equal("thinking.setting.on", J.encode(Profiles.thinking_setting("on")), "true"),
        Check.equal("thinking.setting.max", J.encode(Profiles.thinking_setting("max")), "\"xhigh\""),
        Check.equal("thinking.setting.minimal", J.encode(Profiles.thinking_setting("minimal")), "\"minimal\""),
        Check.equal("thinking.setting.unknown", J.encode(Profiles.thinking_setting("turbo")), "null"),
    ]


def pick_projection(model_name, profile, level):
    settings = {"thinking": Profiles.thinking_setting(level)}
    return body_projection(ModelsOpenai.completions_body(model_name, profile, plain(), settings, forced_tools()))


def thinking_pick_cases():
    glm53 = ModelsOpenai.OpenAIChatModel("glm-5.3", ProvidersHttp.ZaiProvider("k"), {})
    kimi = ModelsOpenai.OpenAIChatModel("kimi-k3", ProvidersHttp.MoonshotAIProvider("k"), {})
    glm47 = ModelsOpenai.OpenAIChatModel("glm-4.7", ProvidersHttp.ZaiProvider("k"), {})
    v4 = ModelsOpenai.OpenAIChatModel("deepseek-v4-flash", ProvidersHttp.DeepSeekProvider("k"), {})
    enabled = '{"clear_thinking":false,"type":"enabled"}'
    return [
        Check.equal("thinking.pick.glm-5.3", join(mapList(Profiles.thinking_levels(glm53.profile, "zai"), lambda level: pick_projection("glm-5.3", glm53.profile, level)), ";"), f"low|{enabled}|required;high|{enabled}|required;max|{enabled}|required"),
        Check.equal("thinking.pick.kimi-k3", join(mapList(Profiles.thinking_levels(kimi.profile, "moonshotai"), lambda level: pick_projection("kimi-k3", kimi.profile, level)), ";"), "minimal||auto;low||auto;medium||auto;high||auto"),
        Check.equal("thinking.pick.glm-4.7", join(mapList(Profiles.thinking_levels(glm47.profile, "zai"), lambda level: pick_projection("glm-4.7", glm47.profile, level)), ";"), '|{"clear_thinking":false,"type":"disabled"}|required;|' + enabled + "|required"),
        Check.equal("thinking.pick.deepseek-v4-flash", join(mapList(Profiles.thinking_levels(v4.profile, "deepseek"), lambda level: pick_projection("deepseek-v4-flash", v4.profile, level)), ";"), "none||required;medium||auto;minimal||auto;low||auto;medium||auto;high||auto;xhigh||auto"),
    ]


def provider_flag_cases():
    kimi = Profiles.moonshotai_provider_profile("kimi-k3")
    v1 = Profiles.moonshotai_provider_profile("moonshot-v1-8k")
    glm53 = Profiles.zai_provider_profile("glm-5.3")
    glm47 = Profiles.zai_provider_profile("glm-4.7")
    glm3 = Profiles.zai_provider_profile("glm-3-turbo")
    v4 = Profiles.deepseek_model_profile("deepseek-v4-flash")
    reasoner = Profiles.deepseek_model_profile("deepseek-reasoner")
    return [
        same("providers.kimi_flags", [
            Profiles.profile_bool(kimi, "openai_supports_tool_choice_required", True),
            Profiles.profile_bool(kimi, "thinking_always_enabled", False),
            Profiles.profile_bool(kimi, "supports_thinking", False),
            Profiles.profile_bool(kimi, "supports_json_object_output", False),
            Profiles.profile_bool(Profiles.moonshotai_model_profile("kimi-k3"), "supports_thinking", False),
            Profiles.profile_bool(Profiles.moonshotai_model_profile("kimi-k3"), "ignore_streamed_leading_whitespace", False),
            Profiles.profile_bool(Profiles.moonshotai_model_profile("moonshot-v1-8k"), "supports_thinking", True),
        ], [False, True, True, True, True, True, False]),
        same("providers.kimi_texts", [
            Profiles.profile_text(kimi, "json_schema_transformer", ""),
            Profiles.profile_text(kimi, "openai_chat_thinking_field", ""),
            Profiles.profile_text(kimi, "openai_chat_send_back_thinking_parts", ""),
        ], ["openai", "reasoning_content", "field"]),
        same("providers.kimi_v1", [Profiles.profile_bool(v1, "supports_thinking", True), Profiles.profile_bool(v1, "thinking_always_enabled", True), Profiles.profile_bool(v1, "openai_supports_tool_choice_required", True)], [False, False, False]),
        same("providers.zai_glm-5.3", [
            Profiles.profile_bool(glm53, "supports_thinking", False),
            Profiles.profile_bool(glm53, "thinking_always_enabled", False),
            Profiles.profile_bool(glm53, "zai_supports_reasoning_effort", False),
            Profiles.profile_bool(glm53, "zai_thinking_body", False),
        ], [True, True, True, True]),
        Check.equal("providers.zai_mapping", canon(unwrap_or(mapGet(glm53, "zai_reasoning_effort_mapping"), JsonNull())), "{\"high\":\"high\",\"low\":\"low\",\"medium\":\"high\",\"minimal\":\"low\",\"xhigh\":\"max\"}"),
        same("providers.zai_glm-4.7_texts", [
            Profiles.profile_text(glm47, "openai_chat_thinking_field", ""),
            Profiles.profile_text(glm47, "openai_chat_send_back_thinking_parts", ""),
        ], ["reasoning_content", "field"]),
        same("providers.zai_glm-4.7_flags", [
            Profiles.profile_bool(glm47, "supports_thinking", False),
            Profiles.profile_bool(glm47, "thinking_always_enabled", True),
            Profiles.profile_bool(glm47, "zai_supports_reasoning_effort", True),
            len(J.members_of(unwrap_or(mapGet(glm47, "zai_reasoning_effort_mapping"), JsonNull()))) == 0,
        ], [True, False, False, True]),
        same("providers.zai_glm-3-turbo", [Profiles.profile_bool(glm3, "supports_thinking", True), Profiles.profile_bool(glm3, "zai_thinking_body", False), Profiles.profile_bool(Profiles.zai_model_profile("glm-3-turbo"), "supports_thinking", True)], [False, True, False]),
        same("providers.deepseek_defaults", [
            Profiles.profile_bool(v4, "openai_reasoning_enabled_by_default", True),
            Profiles.profile_bool(v4, "openai_supports_forced_tool_choice_with_thinking", True),
            Profiles.profile_bool(reasoner, "openai_reasoning_enabled_by_default", True),
            Profiles.profile_bool(reasoner, "openai_supports_forced_tool_choice_with_thinking", True),
            Profiles.profile_bool(reasoner, "openai_supports_tool_choice_required", True),
        ], [True, False, True, True, False]),
        Check.equal("providers.dispatch", Profiles.profile_text(ModelsOpenai.OpenAIChatModel("glm-5.3", ProvidersHttp.ZaiProvider("k"), {}).profile, "json_schema_transformer", ""), "openai"),
        Check.equal("providers.moonshotai_url", ProvidersHttp.MoonshotAIProvider("k").base_url, "https://api.moonshot.ai/v1"),
        Check.equal("providers.zai_url", ProvidersHttp.ZaiProvider("k").base_url, "https://api.z.ai/api/paas/v4"),
    ]


def coding_plan_model():
    """A provider named differently but serving Z.AI's API, reusing the profile directly (the app's coding plan endpoint)."""
    provider = Providers.Provider(name="zai-coding", base_url="https://api.z.ai/api/coding/paas/v4", api_key="k", post=lambda _path, _body, _headers: Error(message="not called"))
    return ModelsOpenai.OpenAIChatModel("glm-5.3", provider, Profiles.zai_provider_profile("glm-5.3"))


def provider_reuse_cases():
    model = coding_plan_model()
    return [
        same("providers.reuse_flags", [Profiles.profile_bool(model.profile, "zai_thinking_body", False), Profiles.profile_bool(model.profile, "thinking_always_enabled", False)], [True, True]),
        Check.equal("providers.reuse_body", body_projection(ModelsOpenai.completions_body("glm-5.3", model.profile, plain(), {"thinking": JsonString("minimal")}, forced_tools())), 'low|{"clear_thinking":false,"type":"enabled"}|required'),
    ]


def thinking_extra_cases():
    glm53 = ModelsOpenai.OpenAIChatModel("glm-5.3", ProvidersHttp.ZaiProvider("k"), {}).profile
    glm52 = ModelsOpenai.OpenAIChatModel("glm-5.2", ProvidersHttp.ZaiProvider("k"), {}).profile
    glm47 = ModelsOpenai.OpenAIChatModel("glm-4.7", ProvidersHttp.ZaiProvider("k"), {}).profile
    glm3 = ModelsOpenai.OpenAIChatModel("glm-3-turbo", ProvidersHttp.ZaiProvider("k"), {}).profile
    v4 = ModelsOpenai.OpenAIChatModel("deepseek-v4-flash", ProvidersHttp.DeepSeekProvider("k"), {}).profile
    parameters = forced_tools()
    merged = ModelsOpenai.completions_body("glm-5.3", glm53, plain(), {"thinking": JsonBool(True), "extra_body": decode("{\"foo\":1}")}, parameters)
    clash = ModelsOpenai.completions_body("glm-5.3", glm53, plain(), {"thinking": JsonBool(True), "extra_body": decode("{\"thinking\":{\"type\":\"disabled\"}}")}, parameters)
    clear = ModelsOpenai.completions_body("glm-4.7", glm47, plain(), {"thinking": JsonBool(True), "zai_clear_thinking": JsonBool(True)}, parameters)
    explicit = ModelsOpenai.completions_body("deepseek-v4-flash", v4, plain(), {"thinking": JsonString("high"), "openai_reasoning_effort": JsonString("low")}, parameters)
    zai_explicit = ModelsOpenai.completions_body("glm-5.2", glm52, plain(), {"thinking": JsonString("low"), "openai_reasoning_effort": JsonString("high")}, parameters)
    nonthinking = ModelsOpenai.completions_body("glm-3-turbo", glm3, plain(), {"thinking": JsonBool(True), "zai_clear_thinking": JsonBool(True)}, parameters)
    return [
        Check.truth("thinking.extra_body_kept", contains(canon(merged), "\"foo\":1") and body_projection(merged) == '|{"clear_thinking":false,"type":"enabled"}|required', canon(merged)),
        Check.equal("thinking.extra_body_replaced", body_projection(clash), '|{"clear_thinking":false,"type":"enabled"}|required'),
        Check.equal("thinking.clear_override", body_projection(clear), '|{"clear_thinking":true,"type":"enabled"}|required'),
        Check.equal("thinking.explicit_effort_wins", body_projection(explicit), "low||auto"),
        Check.equal("thinking.zai_ignores_openai_effort", body_projection(zai_explicit), 'low|{"clear_thinking":false,"type":"enabled"}|required'),
        Check.equal("thinking.zai_clear_on_nonthinking", body_projection(nonthinking), '|{"clear_thinking":true}|required'),
    ]


def reasoning_round_trip_cases():
    reply = '{"id":"r1","model":"m","choices":[{"message":{"role":"assistant","content":"calling","reasoning_content":"hmm","tool_calls":[{"id":"c1","type":"function","function":{"name":"add","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":2}}'
    kimi = ModelsOpenai.OpenAIChatModel("kimi-k3", ProvidersHttp.MoonshotAIProvider("k"), {})
    glm = ModelsOpenai.OpenAIChatModel("glm-4.7", ProvidersHttp.ZaiProvider("k"), {})
    second = M.of_response(M.model_response([M.ToolCallPart(tool_name="get_temperature", args="{}", tool_call_id="c3")], ""))
    parameters = Models.with_tools([one_tool()])
    match ModelsOpenai.process_response("kimi-k3", "moonshotai", kimi.profile, reply):
        case E.Ok(value=response):
            messages = listConcat(history(), [M.of_response(response), M.of_request(M.ModelRequest(parts=[M.ToolReturnPart(tool_name="add", content="3", tool_call_id="c1")], instructions="")), second])
            body = ModelsOpenai.completions_body("kimi-k3", kimi.profile, messages, {}, parameters)
            mapped = J.list_at(body, "messages")
            kimi_assistant = canon(unwrap_or(listGet(mapped, 9), JsonNull()))
            kimi_backfill = canon(unwrap_or(listGet(mapped, 11), JsonNull()))
            glm_body = J.list_at(ModelsOpenai.completions_body("glm-4.7", glm.profile, messages, {}, parameters), "messages")
            glm_assistant = canon(unwrap_or(listGet(glm_body, 9), JsonNull()))
            return [
                Check.equal("thinking.reasoning_read", join(mapList(response.parts, describe_part), "|"), "thinking:hmm|text:calling|tool-call:add|{}|c1"),
                Check.truth("thinking.kimi_sends_field", contains(kimi_assistant, '\"reasoning_content\":\"hmm\"'), kimi_assistant),
                Check.truth("thinking.kimi_backfills", contains(kimi_backfill, '\"reasoning_content\":\"\"'), kimi_backfill),
                Check.truth("thinking.zai_sends_field", contains(glm_assistant, '\"reasoning_content\":\"hmm\"'), glm_assistant),
                Check.equal("thinking.strict_tools", J.bool_at(J.object_at(unwrap_or(listGet(J.list_at(body, "tools"), 0), JsonNull()), "function"), "strict", False), True),
            ]
        case E.Failed(error=error):
            return [Check.failed("thinking.reasoning_read", E.message_of(error))]


# The MCP client (pydantic_ai.mcp as a sans-IO port; the recorded flows follow upstream's
# mcp.py and the MCP specification, per docs/plans/0052-agent.md)

INIT_REPLY = '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{"listChanged":true},"logging":{},"experimental":{"e1":{}}},"serverInfo":{"name":"srv","title":"Server","version":"2.0"},"instructions":"be helpful"}}'
"""A recorded `initialize` reply."""


def incoming_text(incoming):
    """One `Incoming` as text: which kind, and what it carries."""
    match incoming:
        case Mcp.ResponseOk(id=id, result=result):
            return f"ok:{J.encode(id)}:{canon(result)}"
        case Mcp.ResponseError(id=id, error=error):
            return f"error:{J.encode(id)}:{Mcp.message_of(error)}"
        case Mcp.ServerRequest(id=id, method=method):
            return f"request:{J.encode(id)}:{method}"
        case Mcp.ServerNotification(method=method):
            return f"notification:{method}"
        case Mcp.Malformed(reason=reason):
            return f"malformed:{reason}"


def session_checks_of(result):
    """The checks on a recorded `initialize` reply's session."""
    match result:
        case Mcp.Ok(session=session):
            return [
                Check.equal("mcp.initialize_request", canon(Mcp.initialize_request(JsonInt(1), "osprey-agent", "1.2")), '{"id":1,"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"osprey-agent","version":"1.2"},"protocolVersion":"2025-06-18"}}'),
                Check.equal("mcp.server_info", session.server_info.name + "/" + session.server_info.title + "/" + session.server_info.version, "srv/Server/2.0"),
                same("mcp.capabilities", [session.capabilities.tools, session.capabilities.tools_list_changed, session.capabilities.logging, session.capabilities.prompts, session.capabilities.prompts_list_changed, session.capabilities.resources, session.capabilities.resources_list_changed, session.capabilities.completions], [True, True, True, False, False, False, False, False]),
                same("mcp.capabilities_experimental", session.capabilities.experimental, ["e1"]),
                Check.equal("mcp.instructions", session.instructions + "/" + session.protocol_version, "be helpful/2025-06-18"),
            ]
        case Mcp.Failed(error=error):
            return [Check.failed("mcp.initialize_result", Mcp.message_of(error))]


def initialize_error_text(result):
    """The message of a failed `initialize_result`, empty on success."""
    match result:
        case Mcp.Ok(session=_s):
            return ""
        case Mcp.Failed(error=error):
            return Mcp.message_of(error)


def mcp_tool_checks():
    """The tool-of/definition/name-round-trip checks, as one outcome list."""
    match Mcp.tool_of(decode('{"name":"add","description":"adds","inputSchema":{"type":"object","properties":{}}}')):
        case Success(value=tool):
            definition = Mcp.tool_definition_of("srv", tool)
            return [
                Check.equal("mcp.tool_names", join(mapList([definition.name, Mcp.original_tool_name("srv", definition.name), Mcp.original_tool_name("srv", "get"), Mcp.original_tool_name("srv", "unrelated")], toString), ","), "srv_add,add,get,unrelated"),
                Check.equal("mcp.tool_definition", definition.description + "/" + canon(definition.parameters_json_schema), 'adds/{"properties":{},"type":"object"}'),
                Check.equal("mcp.prefixed_own_prefix", Mcp.original_tool_name("srv", Mcp.tool_definition_of("srv", Mcp.Tool(name="srv_quote", description="", input_schema=J.obj([]))).name), "srv_quote"),
            ]
        case Error(message=message):
            return [Check.failed("mcp.tool_of", message)]


def mcp_session_cases():
    session_checks = session_checks_of(Mcp.initialize_result(J.object_at(decode(INIT_REPLY), "result")))
    bad_version = Mcp.initialize_result(decode('{"protocolVersion":"2024-11-05","serverInfo":{"name":"s","version":"1"},"capabilities":{}}'))
    page1 = Mcp.list_tools_result(decode('{"tools":[{"name":"add","description":"adds","inputSchema":{"type":"object"}},{"name":"broken"},{"name":"get","inputSchema":{"type":"object"},"description":""}],"nextCursor":"c2"}'))
    page2 = Mcp.list_tools_result(decode('{"tools":[{"name":"last","inputSchema":{"type":"array"}}]}'))
    call_reply = Mcp.call_result("add", decode('{"content":[{"type":"text","text":"3"}]}'))
    line = Mcp.encode_line(Mcp.call_tool_request(JsonInt(7), "log", {"message": JsonString("two\nlines")}))
    return listConcat(session_checks, listConcat([
        Check.equal("mcp.bad_version", initialize_error_text(bad_version), "Server requested protocol version 2024-11-05, which this client does not support (code: -32602)"),
        Check.equal("mcp.initialized", canon(Mcp.initialized_notification()), '{"jsonrpc":"2.0","method":"notifications/initialized"}'),
        Check.equal("mcp.list_tools_request", canon(Mcp.list_tools_request(JsonInt(2), "")) + "/" + canon(Mcp.list_tools_request(JsonInt(3), "c2")), '{"id":2,"jsonrpc":"2.0","method":"tools/list","params":{}}/{"id":3,"jsonrpc":"2.0","method":"tools/list","params":{"cursor":"c2"}}'),
        Check.equal("mcp.list_tools_page1", join(mapList(page1.tools, lambda t: t.name + ":" + t.description), ";") + "/" + page1.next_cursor + "/" + join(page1.problems, ";"), "add:adds;get:/c2/tool 'broken' needs an inputSchema object"),
        Check.equal("mcp.list_tools_page2", join(mapList(page2.tools, lambda t: t.name), ";") + "/" + page2.next_cursor, "last/"),
        Check.equal("mcp.call_tool_request", canon(Mcp.call_tool_request(JsonInt(4), "add", {"a": JsonInt(1), "b": JsonInt(2)})), '{"id":4,"jsonrpc":"2.0","method":"tools/call","params":{"arguments":{"a":1,"b":2},"name":"add"}}'),
        Check.equal("mcp.ping", canon(Mcp.ping_request(JsonInt(5))) + "/" + canon(Mcp.ping_response(JsonInt(5))), '{"id":5,"jsonrpc":"2.0","method":"ping","params":{}}/{"id":5,"jsonrpc":"2.0","result":{}}'),
        Check.equal("mcp.error_response", incoming_text(Mcp.read_message('{"jsonrpc":"2.0","id":6,"error":{"code":-32601,"message":"no such tool","data":{"k":1}}}')), 'error:6:no such tool (code: -32601, data: {"k":1})'),
        Check.equal("mcp.server_request", incoming_text(Mcp.read_message('{"jsonrpc":"2.0","id":9,"method":"roots/list","params":{}}')), "request:9:roots/list"),
        Check.equal("mcp.server_notification", incoming_text(Mcp.read_message('{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}')), "notification:notifications/tools/list_changed"),
        Check.equal("mcp.call_text", unwrap_or(call_reply, ""), "3"),
        Check.equal("mcp.call_json_text", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"text","text":"[1, 2]"}]}')), ""), "[1,2]"),
        Check.equal("mcp.call_bad_json_text", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"text","text":"{not json"}]}')), ""), "{not json"),
        Check.equal("mcp.call_structured", unwrap_or(Mcp.call_result("t", decode('{"structuredContent":{"result":7},"content":[{"type":"text","text":"7"}]}')), ""), "7"),
        Check.equal("mcp.call_structured_object", unwrap_or(Mcp.call_result("t", decode('{"structuredContent":{"a":1,"b":[2]}}')), ""), "{\"a\":1,\"b\":[2]}"),
        Check.equal("mcp.call_mixed", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"text","text":"a"},{"type":"image","mimeType":"image/png","data":"QUJD"}]}')), ""), "a\n[image: image/png, 3 bytes]"),
        Check.equal("mcp.call_audio", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"audio","mimeType":"audio/wav","data":"QQ=="}]}')), ""), "[audio: audio/wav, 1 bytes]"),
        Check.equal("mcp.call_resource", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"resource_link","uri":"file:///x"},{"type":"resource","resource":{"uri":"file:///y","mimeType":"text/plain","text":"hi"}},{"type":"resource","resource":{"uri":"file:///z","mimeType":"application/octet-stream","blob":"QUJD"}}]}')), ""), "file:///x\nhi\n[resource: application/octet-stream, 3 bytes]"),
        Check.equal("mcp.call_unsupported", unwrap_or(Mcp.call_result("t", decode('{"content":[{"type":"weird"}]}')), ""), "[unsupported content type: weird]"),
        Check.equal("mcp.call_is_error", error_text(Mcp.call_result("t", decode('{"isError":true,"content":[{"type":"text","text":"boom"}]}'))), "boom"),
        Check.equal("mcp.call_is_error_empty", error_text(Mcp.call_result("t", decode('{"isError":true}'))), "Tool 't' returned an error"),
        Check.equal("mcp.call_is_error_structured", error_text(Mcp.call_result("t", decode('{"isError":true,"structuredContent":{"why":"bad"},"content":[{"type":"image","mimeType":"image/png","data":"QUJD"}]}'))), "{\"why\":\"bad\"}"),
        Check.truth("mcp.encode_line", endsWith(line, "\n") and not contains(take(line, length(line) - 1), "\n") and incoming_text(Mcp.read_message(line)) == "request:7:tools/call", line),
        Check.equal("mcp.tool_of_bad", error_text(Mcp.tool_of(decode('{"name":""}'))) + "/" + error_text(Mcp.tool_of(decode('{"name":"x"}'))), "a tool needs a name/tool 'x' needs an inputSchema object"),
    ], mcp_tool_checks()))


MALFORMED_MESSAGES = [
    ["", "prefix:not JSON"],
    ["nope", "prefix:not JSON"],
    ["[1,2]", "exact:malformed:a message must be a JSON object"],
    ["42", "exact:malformed:a message must be a JSON object"],
    ['{"jsonrpc":"2.0"}', "exact:malformed:a message needs a method, a result or an error"],
    ['{"method":"x"}', "exact:notification:x"],
    ['{"method":1,"id":1}', "exact:malformed:method must be a string"],
    ['{"id":1,"result":{"a":1}}', "exact:ok:1:{\"a\":1}"],
    ['{"id":null,"result":1}', "exact:malformed:id must be a string or a number"],
    ['{"id":true,"result":1}', "exact:malformed:id must be a string or a number"],
    ['{"id":1.5,"result":1}', "exact:malformed:id must be a string or a number"],
    ['{"id":"s","result":null}', 'exact:ok:"s":null'],
    ['{"id":1,"error":{"message":"m"}}', "exact:malformed:an error response needs an integer code"],
    ['{"id":1,"error":{"code":"x","message":"m"}}', "exact:malformed:an error response needs an integer code"],
    ['{"id":1,"error":{"code":-32000,"message":"m"}}', "exact:error:1:m (code: -32000)"],
    ['{"id":123456789012345678901234567890,"result":1}', "prefix:not JSON"],
    ['{"id":9007199254740993,"result":1}', "exact:ok:9007199254740993:1"],
    ['{"id":"標","result":{"文":"🚀"}}', 'exact:ok:"標":{"文":"🚀"}'],
    ['  {"id":1,"result":1}  ', "exact:ok:1:1"],
]
"""Texts read through `read_message`: an exact want, or a prefix the decoder's own words finish."""


def matched(tag, got, want, exact):
    """`got` against the oracle's text: a `prefix:` want only has to be contained, any
    other must match exactly — `exact` is the text with its marker already removed."""
    if startsWith(want, "prefix:"):
        return Check.truth(tag, contains(got, drop(want, length("prefix:"))), got)
    else:
        return Check.equal(tag, got, exact)


def mcp_malformed_case(i):
    want = ats(atl(MALFORMED_MESSAGES, i), 1)
    text = ats(atl(MALFORMED_MESSAGES, i), 0)
    return matched(f"mcp.malformed[{i}]", incoming_text(Mcp.read_message(text)), want, drop(want, length("exact:")))


def framing_case(name, text, i, wants):
    """One framing split: `text` fed as two chunks cut at `i` yields the same complete lines."""
    first = Mcp.lines_of(Mcp.LineBuffer(pending=""), take(text, i))
    second = Mcp.lines_of(first.buffer, drop(text, i))
    lines = listConcat(first.lines, second.lines)
    return Check.truth(f"{name}[{i}]", structuralEquals(lines, wants), join(lines, "/"))


MCP_LINE_ONE = '{"id":1,"result":{}}'
MCP_LINE_TWO = '{"method":"notifications/initialized"}'


def mcp_framing_cases():
    text = MCP_LINE_ONE + "\n" + MCP_LINE_TWO + "\n"
    crlf = MCP_LINE_ONE + "\r\n" + MCP_LINE_TWO + "\r\n"
    open_ended = MCP_LINE_ONE + "\n" + MCP_LINE_TWO
    opened = Mcp.lines_of(Mcp.LineBuffer(pending=""), open_ended)
    last = Mcp.lines_of(opened.buffer, "")
    return concat([
        cases(length(text) + 1, lambda i: framing_case("mcp.framing", text, i, [MCP_LINE_ONE, MCP_LINE_TWO])),
        cases(length(crlf) + 1, lambda i: framing_case("mcp.framing_crlf", crlf, i, [MCP_LINE_ONE, MCP_LINE_TWO])),
        [
            Check.truth("mcp.framing_open", structuralEquals(opened.lines, [MCP_LINE_ONE]), open_ended),
            Check.equal("mcp.framing_pending", last.buffer.pending, MCP_LINE_TWO),
            Check.truth("mcp.framing_flush", structuralEquals(last.lines, []), join(last.lines, "/")),
        ],
    ])


SSE_FORMS = [
    ['data: {"id":1,"result":{"a":1}}\n\n', 'ok:1:{"a":1}'],
    ['data: {"id": 1,\ndata: "result": 2}\n\n', 'ok:1:2'],
    ['event: message\ndata: {"id":1,"result":1}\n\n', 'ok:1:1'],
    [': keepalive\n\ndata: {"id":1,"result":1}\n\n', 'ok:1:1'],
    ['data: {"id":1,"result":1}\n\ndata: {"id":2,"result":2}\n\n', 'ok:1:1;ok:2:2'],
    ['data: {"id":1,"result":1}\n', 'ok:1:1'],
    ['data: {"id":1,"result":1}\r\n\r\n', 'ok:1:1'],
    ['data:{"id":1,"result":1}\n\n', 'ok:1:1'],
    ['id: 7\ndata: {"id":1,"result":1}\n\n', 'ok:1:1'],
    ['', ''],
    [': only comments\n: nothing else\n\n', ''],
    ['data: nope\n\n', 'prefix:malformed:not JSON'],
]
"""Streamable-HTTP event-stream bodies and the messages each carries."""


def mcp_sse_case(i):
    body = ats(atl(SSE_FORMS, i), 0)
    want = ats(atl(SSE_FORMS, i), 1)
    got = join(mapList(Mcp.reply_messages("text/event-stream", body), incoming_text), ";")
    return matched(f"mcp.sse[{i}]", got, want, want)


def mcp_sse_broken_case():
    """A good event followed by a malformed one: the first is kept, the second typed as `Malformed`."""
    messages = Mcp.reply_messages("text/event-stream", 'data: {"id":1,"result":1}\n\ndata: broken\n\n')
    return [
        Check.equal("mcp.sse_broken_first", incoming_text(unwrap_or(listGet(messages, 0), Mcp.Malformed(reason=""))), "ok:1:1"),
        Check.truth("mcp.sse_broken_second", contains(incoming_text(unwrap_or(listGet(messages, 1), Mcp.Malformed(reason=""))), "malformed:not JSON"), "the second event was not typed as Malformed"),
    ]


def mcp_http_cases():
    json_body = '{"id":1,"result":1}'
    return [
        Check.equal("mcp.http_json", join(mapList(Mcp.reply_messages("application/json", json_body), incoming_text), ";"), "ok:1:1"),
        same("mcp.http_empty", [len(Mcp.reply_messages("application/json", "")), len(Mcp.reply_messages("application/json", "  \n")), len(Mcp.reply_messages("text/event-stream", ""))], [0, 0, 0]),
        Check.equal("mcp.http_headers", Mcp.request_headers("", ""), "Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-06-18"),
        Check.equal("mcp.http_headers_session", Mcp.request_headers("s1", ""), "Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-06-18\r\nMcp-Session-Id: s1"),
        Check.equal("mcp.http_headers_extra", Mcp.request_headers("s1", "Authorization: Bearer t"), "Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-06-18\r\nMcp-Session-Id: s1\r\nAuthorization: Bearer t"),
    ]


def describe_config(config):
    """One parsed server entry as text."""
    match config:
        case Mcp.StdioServer(name=name, command=command, args=args, env=env, cwd=cwd):
            args_text = join(args, ",")
            return f"stdio:{name}:{command}:{args_text}:{map_text(env)}:{cwd}"
        case Mcp.HttpServer(name=name, url=url, headers=headers):
            return f"http:{name}:{url}:{map_text(headers)}"


def map_text(values):
    """A string map as sorted `k=v` pairs joined by `;`."""
    return join(mapList(listSort(mapKeys(values)), lambda key: key + "=" + unwrap_or(mapGet(values, key), "")), ";")


def config_load_text(load):
    """A config load's servers and problems as text."""
    return join(mapList(load.servers, describe_config), ";") + "##" + join(load.problems, ";")


CONFIG_TEXTS = [
    '{"mcpServers":{"a":{"command":"npx","args":["-y","x"],"env":{"K":"V"},"cwd":"/tmp"}}}',
    '{"mcpServers":{"b":{"url":"https://x/mcp","headers":{"H":"1"}}}}',
    '{"mcpServers":{"c":{"command":"c","url":"u"}}}',
    '{"mcpServers":{"c":{}}}',
    '{"mcpServers":{"d":{"command":"c","args":null,"env":null,"cwd":null}}}',
    '{"mcpServers":{"e":{"url":null}}}',
    '{"mcpServers":{"f":{"command":"c","disabled":true,"type":"stdio","unknown":7}}}',
    '{"mcpServers":{"g":{"command":"${BIN}","args":["${FLAG:-v}"]}}}',
    '{"mcpServers":{"h":{"command":"${NOPE}"}}}',
    '{"mcpServers":{"i":{"command":"${X:-}"}}}',
    '{"mcpServers":{"j":{"command":"c","env":{"TOKEN":"${T}","DEF":"${D:-d}","EMPTY":"${D:-}"}}}}',
    '{"mcpServers":{"k":{"command":"${A:-${B}}"}}}',
    '{"mcpServers":{"l":{"command":"${A"}}}',
    '{"mcpServers":{"m":{"command":"${}","args":["${P:Q}"]}}}',
    '{"mcpServers":{"good":{"command":"c"},"bad":{"args":[]}}}',
    '{"mcpServers":{"n":null}}',
    '{"mcpServers":{"o":"x"}}',
    '{not json',
    '',
    '{"other":{}}',
    '{"mcpServers":null}',
    '{"mcpServers":{"p":{"command":"c","args":["ok",7,null],"env":{"A":"a","B":3}}}}',
]
"""The config texts, in CONFIG_FORM order."""

CONFIG_ENVIRONMENTS = [
    {},
    {},
    {},
    {},
    {},
    {},
    {},
    {"BIN": "run"},
    {},
    {},
    {"T": "secret"},
    {"B": "x"},
    {},
    {},
    {},
    {},
    {},
    {},
    {},
    {},
    {},
    {},
]
"""The environment of each config text, in CONFIG_FORM order."""

CONFIG_WANTS = [
    'stdio:a:npx:-y,x:K=V:/tmp##',
    'http:b:https://x/mcp:H=1##',
    'stdio:c:c:::##',
    "##MCP server config 'c' must have either 'command' or 'url'",
    'stdio:d:c:::##',
    "##MCP server config 'e' must have either 'command' or 'url'",
    'stdio:f:c:::##',
    'stdio:g:run:v::##',
    "##MCP server config 'h': Environment variable ${NOPE} is not defined",
    'stdio:i::::##',
    'stdio:j:c::DEF=d;EMPTY=;TOKEN=secret:##',
    'stdio:k:${B}:::##',
    'stdio:l:${A:::##',
    'stdio:m:${}:${P:Q}::##',
    "stdio:good:c:::##MCP server config 'bad' must have either 'command' or 'url'",
    "##MCP server config 'n' must be an object",
    "##MCP server config 'o' must be an object",
    "prefix:not valid JSON",
    "prefix:not valid JSON",
    '##the configuration must have an mcpServers object',
    '##the configuration must have an mcpServers object',
    'stdio:p:c:ok:A=a:##',
]
"""The servers and problems each config text reports."""



def atd(xs, i):
    """The dict element of a list at i, {} past the end."""
    empty: dict[str, str] = {}
    return unwrap_or(listGet(xs, i), empty)


def mcp_config_case(i):
    load = Mcp.parse_config(ats(CONFIG_TEXTS, i), atd(CONFIG_ENVIRONMENTS, i))
    want = ats(CONFIG_WANTS, i)
    return matched(f"mcp.config[{i}]", config_load_text(load), want, want)


def mcp_merge_cases():
    global_servers = Mcp.parse_config('{"mcpServers":{"a":{"command":"global"},"b":{"url":"https://b/mcp"}}}', {}).servers
    project_servers = Mcp.parse_config('{"mcpServers":{"a":{"command":"project"}}}', {}).servers
    merged = Mcp.merge_configs(global_servers, project_servers)
    return [
        Check.equal("mcp.merge", join(mapList(merged, describe_config), ";"), 'http:b:https://b/mcp:;stdio:a:project:::'),
        Check.equal("mcp.merge_empty", len(Mcp.merge_configs([], [])), 0),
    ]


def initialized_version(result: Mcp.InitializeResult) -> str:
    """The protocol version of a successful `initialize_result`, empty on failure."""
    match result:
        case Mcp.Ok(session=session):
            return session.protocol_version
        case Mcp.Failed(error=_e):
            return ""


def completed_lines(chunk: Mcp.Lines) -> str:
    return join(chunk.lines, "|") + "/" + chunk.buffer.pending


def listed_names(listing: Mcp.ToolList) -> str:
    return join(mapList(listing.tools, lambda tool: tool.name), ",") + "/" + listing.next_cursor


def server_names(load: Mcp.ConfigLoad) -> str:
    return join(mapList(load.servers, server_name), ",")


def server_name(server: Mcp.MCPServerConfig) -> str:
    return Mcp.name_of(server)


def capability_flags(found: Mcp.ServerCapabilities) -> list[bool]:
    return [found.logging, found.prompts, found.prompts_list_changed, found.resources, found.resources_list_changed, found.tools, found.tools_list_changed, found.completions]


def collected_text(result) -> str:
    match result:
        case Success(value=items):
            return join(mapList(items, toString), ",")
        case Error(message=message):
            return f"error:{message}"


def positive(value):
    return Success(value=value) if value > 0 else Error(message=f"not positive: {value}")


def mcp_surface_cases():
    found = Mcp.capabilities_of(decode('{"tools":{"listChanged":true},"prompts":{},"resources":{"listChanged":false},"experimental":{"b":{},"a":{}}}'))
    empty = Mcp.capabilities_of(decode('{}'))
    me = Mcp.Implementation(name="osprey", title="Osprey", version="1")
    return [
        Check.equal("mcp.protocol_version", Mcp.PROTOCOL_VERSION, "2025-06-18"),
        Check.equal("mcp.error_text", Mcp.message_of(Mcp.MCPError(message="boom", code=-32000, data=JsonNull())), "boom (code: -32000)"),
        Check.equal("mcp.error_text_data", Mcp.message_of(Mcp.MCPError(message="boom", code=1, data=decode('{"a":1}'))), 'boom (code: 1, data: {"a":1})'),
        Check.equal("mcp.implementation", me.name + "/" + me.title + "/" + me.version, "osprey/Osprey/1"),
        same("mcp.capabilities_of", capability_flags(found), [False, True, False, True, False, True, True, False]),
        same("mcp.capabilities_of_experimental", found.experimental, ["a", "b"]),
        same("mcp.capabilities_of_empty", capability_flags(empty), [False, False, False, False, False, False, False, False]),
        Check.equal("mcp.capabilities_of_empty_experimental", len(empty.experimental), 0),
        Check.equal("mcp.initialize_version", initialized_version(Mcp.initialize_result(decode('{"protocolVersion":"2025-06-18","serverInfo":{"name":"s","version":"1"},"capabilities":{}}'))), Mcp.PROTOCOL_VERSION),
        Check.equal("mcp.initialize_version_refused", initialized_version(Mcp.initialize_result(decode('{"protocolVersion":"1999-01-01"}'))), ""),
        Check.equal("mcp.lines_named", completed_lines(Mcp.lines_of(Mcp.LineBuffer(pending="a"), "b\r\nc\nd")), "ab|c/d"),
        Check.equal("mcp.tool_list_named", listed_names(Mcp.list_tools_result(decode('{"tools":[{"name":"add","inputSchema":{"type":"object"}}],"nextCursor":"n"}'))), "add/n"),
        Check.equal("mcp.config_names", server_names(Mcp.parse_config('{"mcpServers":{"a":{"command":"c"},"b":{"url":"https://b/mcp"}}}', {})), "a,b"),
        Check.equal("messages.collect", collected_text(M.collect([1, 2, 3], positive)), "1,2,3"),
        Check.equal("messages.collect_empty", collected_text(M.collect([], positive)), ""),
        Check.equal("messages.collect_first_error", collected_text(M.collect([1, 0, -1], positive)), "error:not positive: 0"),
    ]


def usage_cases():
    first = Usage.RequestUsage(input_tokens=10, cache_write_tokens=1, cache_read_tokens=2, output_tokens=3, input_audio_tokens=0, cache_audio_read_tokens=0, output_audio_tokens=0, details={"reasoning_tokens": 4})
    second = Usage.RequestUsage(input_tokens=20, cache_write_tokens=0, cache_read_tokens=0, output_tokens=5, input_audio_tokens=0, cache_audio_read_tokens=0, output_audio_tokens=0, details={"reasoning_tokens": 1, "other": 2})
    run = Usage.incr(Usage.incr(Usage.run_usage(), first), second)
    return [
        Check.ints_equal("usage.incr", [run.requests, run.input_tokens, run.output_tokens, run.cache_write_tokens, run.cache_read_tokens, Usage.run_total_tokens(run)], [2, 30, 8, 1, 2, 38]),
        Check.ints_equal("usage.details", [unwrap_or(mapGet(run.details, "reasoning_tokens"), 0), unwrap_or(mapGet(run.details, "other"), 0)], [5, 2]),
        Check.equal("usage.total", Usage.total_tokens(first), 13),
    ]


def direct_cases():
    model = ModelsTest.TestModel("streamed", ["all"])
    parameters = Models.model_request_parameters()
    match Direct.model_request_stream(model, plain(), {}, parameters):
        case E.Ok(value=stream):
            response = Direct.get(stream)
            first = unwrap_or(listGet(stream.events, 0), M.FinalResultEvent(tool_name="", tool_call_id=""))
            return [
                Check.equal("direct.stream_text", M.text(response), "streamed"),
                Check.equal("direct.event_count", len(stream.events), 2 * len(response.parts)),
                Check.equal("direct.first_event", event_text(first), "start:0:streamed"),
                Check.equal("direct.last_event", event_text(unwrap_or(listGet(stream.events, 1), first)), "end:0:streamed"),
                Check.equal("direct.request_text", direct_text(model, parameters), "streamed"),
                Check.equal("direct.instructions", Models.instructions_text(history(), params_with([], True, [M.InstructionPart(content="inst"), M.UserPromptPart(content="ignored")])), "inst"),
            ]
        case E.Failed(error=error):
            return [Check.failed("direct", E.message_of(error))]


def direct_text(model, parameters):
    match Direct.model_request(model, plain(), {}, parameters):
        case E.Ok(value=response):
            return M.text(response)
        case E.Failed(error=error):
            return E.message_of(error)


def event_text(event):
    match event:
        case M.PartStartEvent(index=index, part=part):
            return f"start:{index}:{M.part_text(part)}"
        case M.PartEndEvent(index=index, part=part):
            return f"end:{index}:{M.part_text(part)}"
        case M.PartDeltaEvent(index=index):
            return f"delta:{index}"
        case M.FinalResultEvent(tool_name=tool_name):
            return f"final:{tool_name}"


def exception_cases():
    return [
        same("exceptions.messages", mapList([
            E.ModelAPIError(model_name="m", message="boom"), E.UserError(message="misuse"), E.UnexpectedModelBehavior(message="odd", body=""), E.UnexpectedModelBehavior(message="odd", body="raw"), E.UsageLimitExceeded(message="limit"),
        ], E.message_of), ["boom", "misuse", "odd", "odd, body:\nraw", "limit"]),
        same("exceptions.outcome_ok", [E.is_ok(E.Ok(value=1)), E.is_ok(E.Failed(error=E.UserError(message="x")))], [True, False]),
        Check.equal("exceptions.outcome_message", E.error_message(E.Failed(error=E.UserError(message="x"))) + "|" + E.error_message(E.Ok(value=1)), "x|"),
        Check.equal("exceptions.error_of", E.message_of(E.error_of(E.Ok(value=1))), "ok"),
    ]


def schema_cases():
    loose = JsonSchema.transform(schema(LOOSE_SCHEMA))
    strict = JsonSchema.transform(schema(ADD_SCHEMA))
    untyped = JsonSchema.transform(decode("{\"type\":\"object\",\"title\":\"T\",\"properties\":{\"xs\":{\"type\":\"array\",\"items\":{}}},\"required\":[\"xs\"]}"))
    return [
        same("schema.strict_flags", [loose.is_strict_compatible, strict.is_strict_compatible, untyped.is_strict_compatible], [False, True, False]),
        Check.equal("schema.loose", canon(loose.schema), "{\"additionalProperties\":false,\"properties\":{\"n\":{\"type\":\"integer\"},\"q\":{\"minLength\":2,\"type\":\"string\"}},\"required\":[\"q\"],\"type\":\"object\"}"),
        Check.equal("schema.strict", canon(strict.schema), "{\"additionalProperties\":false,\"properties\":{\"a\":{\"type\":\"integer\"},\"b\":{\"type\":\"integer\"}},\"required\":[\"a\",\"b\"],\"type\":\"object\"}"),
        Check.equal("schema.untyped_array", canon(untyped.schema), "{\"additionalProperties\":false,\"properties\":{\"xs\":{\"items\":{},\"type\":\"array\"}},\"required\":[\"xs\"],\"type\":\"object\"}"),
    ]


def review_cases():
    no_message = ModelsOpenai.process_response("m", "openai", openai_model("gpt-4o", {}).profile, "{\"choices\":[{}]}")
    override = J.encode(ModelsOpenai.completions_body("gpt-4o", openai_model("gpt-4o", {}).profile, plain(), {"extra_body": decode("{\"stream\":true,\"extra\":1}")}, Models.model_request_parameters()))
    nested = JsonSchema.transform(decode("{\"type\":\"object\",\"properties\":{\"box\":{\"$defs\":{\"S\":{\"type\":\"string\"}},\"$ref\":\"#/properties/box/$defs/S\"}},\"required\":[\"box\"]}"))
    return [
        Check.truth("review.missing_message_refused", outcome_failed(no_message), "accepted a choice with no message"),
        Check.truth("review.extra_body_replaces", contains(override, "\"stream\":true") and not contains(override, "\"stream\":false") and contains(override, "\"extra\":1"), override),
        Check.equal("review.validate_refuses_object", error_text(M.validate_messages(decode("{}"))), "messages must be a JSON array"),
        Check.truth("review.nested_defs_kept", contains(canon(nested.schema), "\"$defs\":{\"S\""), canon(nested.schema)),
    ]


def outcome_failed(outcome):
    match outcome:
        case E.Failed():
            return True
        case E.Ok():
            return False


def accessor_cases():
    request = M.of_request(M.model_request([M.InstructionPart(content="be brief")]))
    response = M.of_response(M.model_response([M.TextPart(content="hi")], "m"))
    thinking = M.ThinkingPart(content="hm")
    call = M.ToolCallPart(tool_name="f", args="{}", tool_call_id="c")
    round_trip = M.validate_messages(M.dump_messages([request]))
    numeric = M.validate_messages(decode("[{\"kind\":\"request\",\"parts\":[{\"part_kind\":\"tool-return\",\"tool_name\":\"f\",\"tool_call_id\":\"c\",\"content\":3},{\"part_kind\":\"tool-return\",\"tool_name\":\"g\",\"tool_call_id\":\"d\"}]}]"))
    return [
        Check.equal("access.call_id", M.generate_tool_call_id(7), "pyd_ai_7"),
        same("access.is_response", [M.is_response(response), M.is_response(request)], [True, False]),
        same("access.is_request", [M.is_request(request), M.is_request(response)], [True, False]),
        same("access.request_parts", [len(M.request_parts(request)), len(M.request_parts(response))], [1, 0]),
        same("access.response_parts", [len(M.response_parts(response)), len(M.response_parts(request))], [1, 0]),
        same("access.has_calls", [M.has_tool_calls(M.model_response([call], "m")), M.has_tool_calls(M.model_response([thinking], "m"))], [True, False]),
        Check.equal("access.non_call_fields", M.tool_name_of(thinking) + M.tool_call_id_of(thinking) + M.part_text(call), ""),
        Check.equal("access.instruction_round_trip", error_text(round_trip), ""),
        Check.truth("access.scalar_contents", contains(canon(M.dump_messages(unwrap_or(numeric, []))), "\"content\":\"3\"") and contains(canon(M.dump_messages(unwrap_or(numeric, []))), "\"content\":\"\""), "contents must read as text"),
        Check.equal("access.bad_response_part", error_text(M.validate_messages(decode("[{\"kind\":\"response\",\"parts\":[{\"part_kind\":\"mystery\"}]}]"))), "unknown response part_kind: mystery"),
        Check.truth("access.instruction_stays_off_the_messages", not contains(J.encode(ModelsOpenai.completions_body("gpt-4o", openai_model("gpt-4o", {}).profile, [request], {}, Models.model_request_parameters())), "be brief"), "instructions travel through the parameters"),
        same("access.finish_reasons", [ModelsOpenai.map_finish_reason("function_call"), ModelsOpenai.map_finish_reason("mystery")], ["tool_call", ""]),
        Check.equal("access.float_from_int", J.float_at(decode("{\"n\":2}"), "n", 0.0), 2.0),
        Check.equal("access.json_makers", J.encode(J.jtext("a")) + J.encode(J.jint(3)), "\"a\"3"),
    ]


def schema_walk_cases():
    defs = JsonSchema.transform(decode("{\"type\":\"object\",\"properties\":{\"a\":{\"$ref\":\"#/$defs/A\"}},\"required\":[\"a\"],\"$defs\":{\"A\":{\"type\":\"object\",\"title\":\"A\",\"properties\":{},\"required\":[]}}}"))
    union = JsonSchema.transform(decode("{\"anyOf\":[{\"type\":\"string\",\"title\":\"S\"},{\"type\":\"integer\"}]}"))
    prefix = JsonSchema.transform(decode("{\"type\":\"array\",\"prefixItems\":[{\"type\":\"string\",\"title\":\"P\"}]}"))
    additional = JsonSchema.transform(decode("{\"type\":\"object\",\"properties\":{},\"additionalProperties\":{\"type\":\"string\",\"title\":\"V\"}}"))
    return [
        Check.truth("walk.defs_kept_and_walked", contains(canon(defs.schema), "\"$defs\":{\"A\"") and not contains(canon(defs.schema), "\"title\":\"A\""), canon(defs.schema)),
        Check.truth("walk.union_members", contains(canon(union.schema), "anyOf") and not contains(canon(union.schema), "\"title\""), canon(union.schema)),
        Check.truth("walk.prefix_items", contains(canon(prefix.schema), "prefixItems") and not contains(canon(prefix.schema), "\"title\""), canon(prefix.schema)),
        Check.equal("walk.additional_not_strict", additional.is_strict_compatible, False),
        Check.truth("walk.additional_walked", not contains(canon(additional.schema), "\"title\""), canon(additional.schema)),
    ]


def export_cases():
    response = M.model_response([M.TextPart(content="hi"), M.ToolCallPart(tool_name="f", args="{}", tool_call_id="c")], "m")
    request = M.model_request([M.UserPromptPart(content="hello")])
    message = M.of_request(request)
    answer = M.of_response(response)
    profile = openai_model("gpt-4o", {}).profile
    tool = Tools.tool_definition("f", "does f", schema(ADD_SCHEMA))
    value = decode("{\"t\":\"x\",\"b\":true,\"l\":[1],\"n\":null}")
    built = J.obj([J.member("k", JsonInt(1))])
    return [
        Check.equal("export.request_limit", Agent.REQUEST_LIMIT, 50),
        Check.truth("export.strict_keys", listContains(JsonSchema.STRICT_INCOMPATIBLE_KEYS, "minLength") and listContains(JsonSchema.STRICT_COMPATIBLE_STRING_FORMATS, "date-time"), "the strict tables lost their entries"),
        Check.equal("export.text_at", J.text_at(value, "t", "") + J.text_at(value, "gone", "fallback"), "xfallback"),
        same("export.bool_at", [J.bool_at(value, "b", False), J.bool_at(value, "gone", False)], [True, False]),
        Check.equal("export.list_at", len(J.list_at(value, "l")) + len(J.list_at(value, "gone")), 1),
        same("export.is_null", [J.is_null(J.object_at(value, "n")), J.is_null(J.object_at(value, "gone")), J.is_null(J.object_at(value, "t"))], [True, True, False]),
        Check.equal("export.member_built", J.encode(built), "{\"k\":1}"),
        Check.equal("export.member_value", J.encode(J.member_value(unwrap_or(listGet(J.members_of(built), 0), J.member("", JsonNull())))), "1"),
        Check.equal("export.members_of_other", len(J.members_of(JsonInt(1))), 0),
        Check.equal("export.dump_message", canon(M.dump_message(message)), canon(M.dump_request(request))),
        Check.equal("export.dump_response", canon(M.dump_message(answer)), canon(M.dump_response(response))),
        Check.equal("export.validate_message", error_text(M.validate_message(M.dump_message(answer))), ""),
        Check.equal("export.validate_message_bad", error_text(M.validate_message(decode("{\"kind\":\"other\"}"))), "unknown message kind: other"),
        same("export.is_text", mapList(response.parts, M.is_text), [True, False]),
        same("export.is_tool_call", mapList(response.parts, M.is_tool_call), [False, True]),
        Check.equal("export.responses", len(M.responses([message, answer, message])), 1),
        Check.equal("export.map_request", canon(unwrap_or(listGet(ModelsOpenai.map_request(profile, request), 0), JsonNull())), "{\"content\":\"hello\",\"role\":\"user\"}"),
        Check.truth("export.map_response", contains(canon(unwrap_or(listGet(ModelsOpenai.map_response(profile, response), 0), JsonNull())), "\"tool_calls\""), "the response lost its call"),
        Check.equal("export.map_messages", len(ModelsOpenai.map_messages(profile, [message, answer], Models.model_request_parameters())), 2),
        Check.equal("export.map_tool_definition", J.text_at(J.object_at(ModelsOpenai.map_tool_definition(profile, tool), "function"), "name", ""), "f"),
        Check.equal("export.map_usage", ModelsOpenai.map_usage(decode("{\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}")).input_tokens, 3),
        chat_request_case(profile),
        Check.equal("export.char_of", ModelsTest.char_of(0) + ModelsTest.char_of(1), "ab"),
        Check.equal("export.test_model", ModelsTest.test_model("named", "hi", [], 0).model_name, "named"),
        Check.equal("export.request_usage", Usage.request_usage().input_tokens, 0),
        Check.equal("export.details_merge", unwrap_or(mapGet(Usage.details_merge({"a": 1}, {"a": 2, "b": 3}), "a"), 0) + unwrap_or(mapGet(Usage.details_merge({"a": 1}, {"a": 2, "b": 3}), "b"), 0), 6),
        post_json_case(),
    ]


def chat_request_case(profile):
    provider = Providers.scripted("openai", replies(["{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}]}"], 200))
    match ModelsOpenai.chat_request("gpt-4o", provider, profile, plain(), {}, Models.model_request_parameters()):
        case E.Ok(value=response):
            return Check.equal("export.chat_request", M.text(response), "done")
        case E.Failed(error=error):
            return Check.failed("export.chat_request", E.message_of(error))


def post_json_case():
    """A real request to a port nothing listens on: a refused connection is an error, never a reply."""
    match ProvidersHttp.post_json("http://127.0.0.1:1", "/", "{}", ""):
        case Success(value=reply):
            return Check.failed("export.post_json_refused", f"got status {reply.status}")
        case Error(message=message):
            return Check.truth("export.post_json_refused", message != "", "no reason given")


def typed_output(outcome: E.Outcome[Agent.AgentRunResult]) -> str:
    match outcome:
        case E.Ok(value=result):
            return f"{result.output}:{len(Agent.all_messages(result))}"
        case E.Failed(error=error):
            return E.message_of(error)


def typed_error(error: E.ModelError) -> str:
    return E.message_of(error)


def typed_first_request_part(parts: list[M.ModelRequestPart]) -> M.ModelRequestPart:
    return unwrap_or(listGet(parts, 0), M.UserPromptPart(content="none"))


def typed_first_response_part(parts: list[M.ModelResponsePart]) -> M.ModelResponsePart:
    return unwrap_or(listGet(parts, 0), M.TextPart(content="none"))


def typed_delta(delta: M.ModelResponsePartDelta) -> str:
    match delta:
        case M.TextPartDelta(content_delta=content_delta):
            return content_delta
        case _:
            return ""


def typed_events(stream: Direct.StreamedResponse) -> list[M.ModelResponseStreamEvent]:
    return Direct.events_of(stream.response)


def typed_gen(generated: ModelsTest.Gen) -> str:
    return J.encode(generated.value)


def typed_profile(profile: Profiles.ModelProfile) -> Profiles.ModelProfile:
    return profile


def typed_reply(reply: Providers.HttpReply) -> int:
    return reply.status


def typed_settings() -> Settings.ModelSettings:
    return {"seed": JsonInt(1)}


def typed_definition(definition: Tools.ToolDefinition) -> str:
    return definition.name


def typed_tool() -> Tools.Tool:
    return Tools.tool("f", "does f", schema(ADD_SCHEMA), lambda _args: Success(value="ok"))


def typed_run_usage(usage: Usage.RunUsage) -> int:
    return usage.requests


def typed_export_cases():
    run = Agent.run_sync(Agent.agent(ModelsTest.TestModel("hi", ["all"]), "sys"), "go", [])
    stream = Direct.StreamedResponse(events=[], response=M.model_response([M.TextPart(content="a")], "m"))
    transformed = JsonSchema.Transformed(schema=JsonNull(), is_strict_compatible=True)
    generated = ModelsTest.gen_any(decode("{}"), decode("{\"enum\":[\"only\"]}"), 0)
    return [
        Check.equal("typed.agent_run_result", typed_output(run), "hi:2"),
        Check.truth("typed.model_error", contains(typed_error(E.ModelHTTPError(status_code=500, model_name="m", body="b")), "500"), "the status is missing"),
        Check.truth("typed.request_part", structuralEquals(typed_first_request_part([]), M.UserPromptPart(content="none")), "the fallback part changed"),
        Check.equal("typed.response_part", M.part_text(typed_first_response_part([])), "none"),
        Check.equal("typed.delta", typed_delta(M.TextPartDelta(content_delta="more")), "more"),
        Check.equal("typed.stream_events", len(typed_events(stream)), 2),
        Check.equal("typed.transformed", transformed.is_strict_compatible, True),
        Check.equal("typed.gen", typed_gen(generated), "\"only\""),
        Check.equal("typed.gen_seed", generated.seed, 0),
        Check.truth("typed.profile", structuralEquals(typed_profile(Profiles.model_profile()), Profiles.model_profile()), "profiles differ"),
        Check.equal("typed.http_reply", typed_reply(Providers.reply(201, "x")), 201),
        Check.equal("typed.settings", Settings.setting_int(typed_settings(), "seed", 0), 1),
        Check.equal("typed.tool_definition", typed_definition(Tools.tool_definition_of(typed_tool())), "f"),
        Check.equal("typed.run_usage", typed_run_usage(Usage.run_usage()), 0),
    ]


def suite():
    return concat([
        cases(len(Expected.SCENARIO_NAMES), body_case),
        cases(len(Expected.REPLY_BODIES), parse_case),
        cases(len(Expected.TEXT_PARTS), text_case),
        concat(cases(len(Expected.ARGS_INPUTS), args_cases)),
        cases(len(Expected.RETRY_INPUTS), retry_case),
        cases(len(Expected.DUMP_PROJECTED), dump_case),
        validate_cases(),
        validate_error_cases(),
        cases(len(Expected.GEN_SCHEMAS), gen_case),
        test_agent_cases(),
        test_agent_custom_cases(),
        [test_agent_no_tools_case(), test_agent_unknown_tool_case()],
        scripted_cases(),
        status_error_cases(),
        [transport_error_case(), usage_limit_case()],
        cases(27, http_reply_case),
        cases(6, http_close_case),
        profile_cases(),
        settings_cases(),
        cases(len(THINKING_TABLE), thinking_body_case),
        cases(len(THINKING_TABLE), thinking_levels_case),
        thinking_setting_cases(),
        thinking_pick_cases(),
        provider_flag_cases(),
        provider_reuse_cases(),
        thinking_extra_cases(),
        reasoning_round_trip_cases(),
        mcp_session_cases(),
        cases(len(MALFORMED_MESSAGES), mcp_malformed_case),
        mcp_framing_cases(),
        cases(len(SSE_FORMS), mcp_sse_case),
        mcp_sse_broken_case(),
        mcp_http_cases(),
        cases(len(CONFIG_TEXTS), mcp_config_case),
        mcp_merge_cases(),
        mcp_surface_cases(),
        usage_cases(),
        direct_cases(),
        exception_cases(),
        schema_cases(),
        review_cases(),
        accessor_cases(),
        schema_walk_cases(),
        export_cases(),
        typed_export_cases(),
    ])


def main():
    return Check.run(suite)
