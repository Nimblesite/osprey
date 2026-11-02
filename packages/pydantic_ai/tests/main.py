# osprey: flavor=python
"""@osprey/pydantic_ai's suite: values the real pydantic-ai 2.46.0 produced over
an httpx mock transport (scripts/oracles/pydantic_ai.py), compared against the
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


def first_two(xs):
    return [unwrap_or(listGet(xs, 0), Tools.tool_definition("", "", JsonNull())), unwrap_or(listGet(xs, 1), Tools.tool_definition("", "", JsonNull()))]


def last_two(xs):
    return [unwrap_or(listGet(xs, 1), Tools.tool_definition("", "", JsonNull())), unwrap_or(listGet(xs, 2), Tools.tool_definition("", "", JsonNull()))]


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
        return Scenario(model=openai_model("gpt-4o", {}), messages=history(), settings={}, parameters=params_with(first_two(tools()), True, [M.InstructionPart(content="inst")]))
    elif name == "deepseek_history":
        return Scenario(model=ModelsOpenai.OpenAIChatModel("deepseek-chat", ProvidersHttp.DeepSeekProvider("k"), {}), messages=history(), settings={}, parameters=Models.with_tools(first_two(tools())))
    elif name == "o1_mini":
        return Scenario(model=openai_model("o1-mini", {}), messages=plain(), settings={"temperature": JsonFloat(0.5), "max_tokens": JsonInt(5), "openai_reasoning_effort": JsonString("low")}, parameters=Models.model_request_parameters())
    elif name == "ollama":
        return Scenario(model=ModelsOpenai.OpenAIChatModel("llama3", ProvidersHttp.OllamaProvider("http://localhost:11434/v1"), {}), messages=plain(), settings={}, parameters=Models.with_tools(last_two(tools())))
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
    ]


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


def http_error_cases():
    model = scripted_model(["{\"error\":{\"message\":\"rate limited\"}}"], 429)
    match Agent.run_sync(build(model, True), "go", []):
        case E.Ok(value=_result):
            return [Check.failed("http_error", "expected ModelHTTPError")]
        case E.Failed(error=error):
            return [
                Check.equal("http_error.status", E.status_code_of(error), Expected.HTTP_ERROR_STATUS),
                Check.equal("http_error.message", E.message_of(error), f"status_code: 429, model_name: {Expected.HTTP_ERROR_MODEL}, body: {{\"error\":{{\"message\":\"rate limited\"}}}}"),
                Check.equal("http_error.text", E.message_of(E.http_error(429, "gpt-4o", "{\"error\":\"x\"}")), Expected.HTTP_ERROR_MESSAGE),
            ]


def transport_error_case():
    provider = Providers.Provider(name="openai", base_url="x", api_key="", post=lambda _path, _body, _headers: Error(message="connection refused"))
    match Direct.model_request(ModelsOpenai.OpenAIChatModel("gpt-4o", provider, {}), plain(), {}, Models.model_request_parameters()):
        case E.Ok(value=_response):
            return Check.failed("transport_error", "expected ModelAPIError")
        case E.Failed(error=error):
            return Check.equal("transport_error", f"{E.message_of(error)}/{E.status_code_of(error)}", "connection refused/0")


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
        http_post_case(),
    ]


def chat_request_case(profile):
    provider = Providers.scripted("openai", replies(["{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}]}"], 200))
    match ModelsOpenai.chat_request("gpt-4o", provider, profile, plain(), {}, Models.model_request_parameters()):
        case E.Ok(value=response):
            return Check.equal("export.chat_request", M.text(response), "done")
        case E.Failed(error=error):
            return Check.failed("export.chat_request", E.message_of(error))


def http_post_case():
    """A real request to a port nothing listens on: a refused connection is an error, never a reply."""
    match ProvidersHttp.http_post("http://127.0.0.1:1", "/", "{}", ""):
        case Success(value=reply):
            return Check.failed("export.http_post_refused", f"got status {reply.status}")
        case Error(message=message):
            return Check.truth("export.http_post_refused", message != "", "no reason given")


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
        http_error_cases(),
        [transport_error_case(), usage_limit_case()],
        profile_cases(),
        settings_cases(),
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


def report_suite():
    return Check.report(suite())


def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
