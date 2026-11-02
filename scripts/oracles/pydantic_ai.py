#!/usr/bin/env python3
"""CPython oracle for @osprey/pydantic_ai.

Runs the real pydantic-ai (2.46.0) with the OpenAI SDK over an httpx mock
transport, so every Chat Completions request body the library sends and every
reply it parses is recorded without a network, and writes
packages/pydantic_ai/tests/expected.py. Covers request bodies for eight
message/setting/profile scenarios, response parsing of nine canned replies,
`ModelResponse.text`, tool-call argument helpers, retry prompt text, the
`ModelMessagesTypeAdapter` wire form, `TestModel`'s schema-driven arguments,
an agent run on `TestModel`, a scripted multi-step tool-calling agent run on
`OpenAIChatModel`, and exception messages. Deterministic. Run from the
repository root with a Python that has pydantic-ai 2.46.0, openai and httpx:

    python -I scripts/oracles/oracle.py pydantic_ai
"""

from __future__ import annotations

import asyncio
import json
import os
import warnings

os.environ["PYDANTIC_AI_NO_BANNER"] = "1"
warnings.simplefilter("ignore")

import httpx  # noqa: E402
import openai  # noqa: E402
import pydantic_ai  # noqa: E402
from openai import AsyncOpenAI  # noqa: E402
from pydantic_ai import Agent, ModelRetry  # noqa: E402
from pydantic_ai.exceptions import ModelHTTPError, UsageLimitExceeded  # noqa: E402
from pydantic_ai.messages import (  # noqa: E402
    InstructionPart,
    ModelMessagesTypeAdapter,
    ModelRequest,
    ModelResponse,
    RetryPromptPart,
    SystemPromptPart,
    TextPart,
    ThinkingPart,
    ToolCallPart,
    ToolReturnPart,
    UserPromptPart,
)
from pydantic_ai.models import ModelRequestParameters  # noqa: E402
from pydantic_ai.models.openai import OpenAIChatModel  # noqa: E402
from pydantic_ai.models.test import TestModel  # noqa: E402
from pydantic_ai.profiles import ModelProfile  # noqa: E402
from pydantic_ai.profiles.openai import OpenAIModelProfile  # noqa: E402
from pydantic_ai.providers.deepseek import DeepSeekProvider  # noqa: E402
from pydantic_ai.providers.ollama import OllamaProvider  # noqa: E402
from pydantic_ai.providers.openai import OpenAIProvider  # noqa: E402
from pydantic_ai.settings import ModelSettings  # noqa: E402
from pydantic_ai.tools import ToolDefinition  # noqa: E402
from pydantic_ai.usage import UsageLimits  # noqa: E402

import expected_module  # noqa: E402

TEMPERATURE_SCHEMA = {"type": "object", "properties": {"city": {"type": "string", "description": "The city name"}}, "required": ["city"]}
ADD_SCHEMA = {"type": "object", "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}}, "required": ["a", "b"]}
LOOSE_SCHEMA = {"type": "object", "properties": {"q": {"type": "string", "minLength": 2}, "n": {"type": "integer"}}, "required": ["q"]}
TOOLS = [
    ToolDefinition(name="get_temperature", description="The current temperature of a city in degrees Celsius.", parameters_json_schema=TEMPERATURE_SCHEMA),
    ToolDefinition(name="add", description="Adds two numbers.", parameters_json_schema=ADD_SCHEMA),
    ToolDefinition(name="search", description="", parameters_json_schema=LOOSE_SCHEMA),
]

# The keys of the wire form the port carries; the real dump adds timestamps, ids and provider fields.
PORT_KEYS = {
    "request": ["parts", "instructions", "kind"],
    "response": ["parts", "usage", "model_name", "kind", "provider_name", "provider_response_id", "finish_reason"],
    "system-prompt": ["content", "part_kind"],
    "user-prompt": ["content", "part_kind"],
    "tool-return": ["tool_name", "content", "tool_call_id", "part_kind"],
    "retry-prompt": ["content", "tool_name", "tool_call_id", "part_kind"],
    "instruction": ["content", "part_kind"],
    "text": ["content", "part_kind"],
    "thinking": ["content", "part_kind"],
    "tool-call": ["tool_name", "args", "tool_call_id", "part_kind"],
}
USAGE_KEYS = ["input_tokens", "cache_write_tokens", "cache_read_tokens", "output_tokens", "input_audio_tokens", "cache_audio_read_tokens", "output_audio_tokens", "details"]


def canonical(value):
    """Compact JSON with sorted keys, the form both sides compare."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def project(message):
    """A dumped message reduced to the keys the port carries, args as JSON text."""
    kind = message["kind"]
    out = {k: message[k] for k in PORT_KEYS[kind]}
    parts = []
    for part in message["parts"]:
        p = {k: part[k] for k in PORT_KEYS[part["part_kind"]]}
        if p["part_kind"] == "tool-call" and not isinstance(p["args"], str):
            p["args"] = canonical(p["args"])
        if p["part_kind"] == "tool-return" and not isinstance(p["content"], str):
            p["content"] = canonical(p["content"])
        parts.append(p)
    out["parts"] = parts
    if kind == "response":
        out["usage"] = {k: message["usage"][k] for k in USAGE_KEYS}
    return out


def reply(text=None, tool_calls=None, usage=None, finish="stop", extra=None, model="m", id_="r1"):
    message = {"role": "assistant", "content": text}
    if tool_calls:
        message["tool_calls"] = [{"id": i, "type": "function", "function": {"name": n, "arguments": a}} for n, a, i in tool_calls]
    if extra:
        message.update(extra)
    choice = {"index": 0, "message": message}
    if finish is not None:
        choice["finish_reason"] = finish
    body = {"id": id_, "object": "chat.completion", "created": 1, "model": model, "choices": [choice]}
    body["usage"] = usage or {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
    return json.dumps(body)


class Transport:
    """Answers each POST with the reply chosen by how many tool messages the request carries, recording bodies."""

    def __init__(self, replies):
        self.replies = replies
        self.bodies = []

    def __call__(self, request):
        body = json.loads(request.content)
        self.bodies.append(body)
        n = sum(1 for m in body["messages"] if m.get("role") == "tool")
        text = self.replies[min(n, len(self.replies) - 1)]
        try:
            status = 200
            json.loads(text)
        except ValueError:
            return httpx.Response(200, content=text.encode(), headers={"content-type": "application/json"})
        if text.startswith('{"error"'):
            status = 429
        return httpx.Response(status, content=text.encode(), headers={"content-type": "application/json"})


def chat_model(name, transport, provider="openai", profile=None):
    http = httpx.AsyncClient(transport=httpx.MockTransport(transport))
    if provider == "deepseek":
        prov = DeepSeekProvider(api_key="k", http_client=http)
    elif provider == "ollama":
        prov = OllamaProvider(base_url="http://localhost:11434/v1", http_client=http)
    else:
        prov = OpenAIProvider(openai_client=AsyncOpenAI(api_key="k", base_url="http://x/v1", http_client=http))
    return OpenAIChatModel(name, provider=prov, profile=profile)


HISTORY = [
    ModelRequest(parts=[SystemPromptPart(content="sys"), UserPromptPart(content="hello")]),
    ModelResponse(parts=[ThinkingPart(content="hmm"), TextPart(content="calling"), ToolCallPart(tool_name="add", args='{"a": 1, "b": 2}', tool_call_id="c1")]),
    ModelRequest(parts=[ToolReturnPart(tool_name="add", content="3", tool_call_id="c1"), RetryPromptPart(content="bad", tool_name="add", tool_call_id="c2"), RetryPromptPart(content="plain", tool_call_id="c0")], instructions="be brief"),
    ModelResponse(parts=[TextPart(content="first"), TextPart(content="second"), ToolCallPart(tool_name="get_temperature", args="", tool_call_id="c3")]),
    ModelRequest(parts=[ToolReturnPart(tool_name="get_temperature", content="21", tool_call_id="c3"), UserPromptPart(content="and?")]),
]
PLAIN = [ModelRequest(parts=[SystemPromptPart(content="sys"), UserPromptPart(content="hello")])]
SCENARIOS = [
    ("plain", "gpt-4o", "openai", None, PLAIN, None, ModelRequestParameters()),
    ("tools_settings", "gpt-4o", "openai", None, PLAIN,
     ModelSettings(max_tokens=50, temperature=0.2, top_p=0.9, seed=7, stop_sequences=["END"], presence_penalty=0.1, frequency_penalty=0.2, extra_body={"foo": 1}),
     ModelRequestParameters(function_tools=TOOLS)),
    ("no_max_completion", "gpt-4o", "openai", OpenAIModelProfile(openai_chat_supports_max_completion_tokens=False), PLAIN, ModelSettings(max_tokens=10), ModelRequestParameters()),
    ("history", "gpt-4o", "openai", None, HISTORY, None, ModelRequestParameters(function_tools=TOOLS[:2], instruction_parts=[InstructionPart(content="inst")])),
    ("deepseek_history", "deepseek-chat", "deepseek", None, HISTORY, None, ModelRequestParameters(function_tools=TOOLS[:2])),
    ("o1_mini", "o1-mini", "openai", None, PLAIN, ModelSettings(temperature=0.5, max_tokens=5, openai_reasoning_effort="low"), ModelRequestParameters()),
    ("ollama", "llama3", "ollama", None, PLAIN, None, ModelRequestParameters(function_tools=TOOLS[1:])),
    ("required", "gpt-4o", "openai", None, PLAIN, None, ModelRequestParameters(function_tools=TOOLS[:1], allow_text_output=False)),
]

REPLIES = [
    reply(text="hi"),
    reply(text=None, tool_calls=[("get_temperature", '{"city":"Paris"}', "call_1"), ("add", '{"a":21,"b":10}', "call_2")],
          usage={"prompt_tokens": 40, "completion_tokens": 9, "total_tokens": 49, "prompt_tokens_details": {"cached_tokens": 16}, "completion_tokens_details": {"reasoning_tokens": 4, "accepted_prediction_tokens": 0}}, finish="tool_calls"),
    reply(text="answer", extra={"reasoning_content": "deep thought"}),
    reply(text="<think>\nplan\n</think>\n\nresult"),
    reply(text="cut", finish="length"),
    reply(text="none", finish=None),
    reply(text="before<think>mid", finish="content_filter", model="m2", id_="r9"),
    '{"id":"r1","object":"chat.completion","created":1,"model":"m","choices":[],"usage":null}',
    "not json",
]
REPLY_PROFILES = ["openai", "openai", "deepseek", "openai", "openai", "openai", "openai", "openai", "openai"]


def describe_response(resp):
    parts = []
    for p in resp.parts:
        if isinstance(p, TextPart):
            parts.append(f"text:{p.content}")
        elif isinstance(p, ThinkingPart):
            parts.append(f"thinking:{p.content}")
        else:
            parts.append(f"tool-call:{p.tool_name}|{p.args}|{p.tool_call_id}")
    u = resp.usage
    details = ";".join(f"{k}={v}" for k, v in sorted(u.details.items()))
    return [
        "|".join(parts), str(u.input_tokens), str(u.output_tokens), str(u.cache_read_tokens), details,
        resp.model_name or "", resp.provider_response_id or "", resp.finish_reason or "",
    ]


def main():
    constants = {}
    docs = {}

    # Request bodies
    names, bodies = [], []
    for name, model_name, provider, profile, history, settings, params in SCENARIOS:
        transport = Transport([REPLIES[0]])
        model = chat_model(model_name, transport, provider, profile)
        asyncio.run(model.request(history, settings, params))
        names.append(name)
        bodies.append(canonical(transport.bodies[0]))
    constants["SCENARIO_NAMES"] = names
    constants["REQUEST_BODIES"] = bodies
    docs["SCENARIO_NAMES"] = "The request scenarios, in the suite's order."
    docs["REQUEST_BODIES"] = "The Chat Completions request body the real OpenAIChatModel sent per scenario, as sorted-key compact JSON."

    # Response parsing
    parsed, errors = [], []
    for text, provider in zip(REPLIES, REPLY_PROFILES):
        transport = Transport([text])
        model = chat_model("deepseek-chat" if provider == "deepseek" else "gpt-4o", transport, provider)
        try:
            resp = asyncio.run(model.request(PLAIN, None, ModelRequestParameters()))
            parsed.append(describe_response(resp))
            errors.append("")
        except Exception as exc:  # noqa: BLE001
            parsed.append([""] * 8)
            errors.append(type(exc).__name__)
    constants["REPLY_BODIES"] = REPLIES
    constants["REPLY_PROVIDERS"] = REPLY_PROFILES
    constants["PARSED_REPLIES"] = parsed
    constants["PARSED_ERRORS"] = errors
    docs["REPLY_BODIES"] = "Canned Chat Completions reply bodies."
    docs["REPLY_PROVIDERS"] = "The provider each reply is parsed under."
    docs["PARSED_REPLIES"] = "Per reply: parts (`kind:content` joined by `|`), input/output/cache-read tokens, details, model name, response id, finish reason."
    docs["PARSED_ERRORS"] = "Per reply: the exception the real library raised, empty when it parsed."

    # ModelResponse.text
    text_cases = [
        [["text", "a"]],
        [["text", "a"], ["text", "b"]],
        [["text", "a"], ["tool-call", "f"], ["text", "b"]],
        [["thinking", "t"], ["text", "a"], ["thinking", "u"], ["text", "b"], ["text", "c"]],
        [["tool-call", "f"]],
        [],
    ]
    texts = []
    for case in text_cases:
        parts = [TextPart(content=c) if k == "text" else ThinkingPart(content=c) if k == "thinking" else ToolCallPart(tool_name=c, args="{}") for k, c in case]
        texts.append(ModelResponse(parts=parts).text or "")
    constants["TEXT_PARTS"] = [["|".join(f"{k}:{c}" for k, c in case)] for case in text_cases]
    constants["TEXT_EXPECTED"] = texts
    docs["TEXT_PARTS"] = "Part sequences (`kind:content` joined by `|`) whose `ModelResponse.text` is pinned."
    docs["TEXT_EXPECTED"] = "`ModelResponse.text` per sequence, empty for CPython's None."

    # args helpers
    args_cases = ['{"a": 1, "b": "x"}', "", "{}", "[1, 2]", "not json", '{"nested": {"k": [1, 2.5, true, null]}}']
    constants["ARGS_INPUTS"] = args_cases
    constants["ARGS_AS_DICT"] = [canonical(ToolCallPart(tool_name="f", args=a).args_as_dict()) for a in args_cases]
    constants["ARGS_AS_JSON"] = [ToolCallPart(tool_name="f", args=a).args_as_json_str() for a in args_cases]
    docs["ARGS_INPUTS"] = "Raw `args` texts of a ToolCallPart."
    docs["ARGS_AS_DICT"] = "`args_as_dict()` per input as sorted-key JSON."
    docs["ARGS_AS_JSON"] = "`args_as_json_str()` per input."

    # Retry prompt text
    constants["RETRY_INPUTS"] = [["bad value", "add"], ["fix it", ""]]
    constants["RETRY_EXPECTED"] = [RetryPromptPart(content="bad value", tool_name="add").model_response(), RetryPromptPart(content="fix it").model_response()]
    docs["RETRY_INPUTS"] = "`[content, tool_name]` of a RetryPromptPart (empty tool name is CPython's None)."
    docs["RETRY_EXPECTED"] = "`RetryPromptPart.model_response()` per input."

    # Wire form
    dumped = ModelMessagesTypeAdapter.dump_python(HISTORY, mode="json")
    constants["DUMP_FULL"] = canonical(dumped)
    constants["DUMP_PROJECTED"] = [canonical(project(m)) for m in dumped]
    assert ModelMessagesTypeAdapter.validate_python([project(m) for m in dumped])
    docs["DUMP_FULL"] = "`ModelMessagesTypeAdapter.dump_python(HISTORY, mode='json')` in full, the form the port must read."
    docs["DUMP_PROJECTED"] = "Each dumped message reduced to the keys the port writes, sorted-key JSON; the real adapter validates this form."

    # TestModel schema data
    gen_schemas = [
        TEMPERATURE_SCHEMA,
        ADD_SCHEMA,
        LOOSE_SCHEMA,
        {"type": "object", "properties": {"flag": {"type": "boolean"}, "f": {"type": "number", "minimum": 2.5}, "i": {"type": "integer", "minimum": 3, "maximum": 3}, "j": {"type": "integer", "exclusiveMinimum": 4}, "k": {"type": "integer", "maximum": 9}, "e": {"enum": ["x", "y"]}, "c": {"const": 7}, "s": {"type": "string", "maxLength": 0}}, "required": ["flag", "f", "i", "j", "k", "e", "c", "s"], "additionalProperties": True},
        {"type": "object", "properties": {"items": {"type": "array", "items": {"type": "string"}, "minItems": 3, "uniqueItems": True}, "pair": {"type": "array", "prefixItems": [{"type": "integer"}, {"type": "boolean"}]}, "ref": {"$ref": "#/$defs/Inner"}, "any": {"anyOf": [{"type": "null"}, {"type": "integer"}]}, "untyped": {}}, "required": ["items", "pair", "ref", "any", "untyped"], "$defs": {"Inner": {"type": "object", "properties": {"v": {"type": "number", "exclusiveMaximum": 1.5}}, "required": ["v"]}}},
    ]
    gen_seeds = [0, 0, 5, 1, 2]
    constants["GEN_SCHEMAS"] = [json.dumps(s, separators=(",", ":")) for s in gen_schemas]
    constants["GEN_SEEDS"] = gen_seeds
    constants["GEN_EXPECTED"] = [canonical(TestModel(seed=seed).gen_tool_args(ToolDefinition(name="t", parameters_json_schema=s))) for s, seed in zip(gen_schemas, gen_seeds)]
    docs["GEN_SCHEMAS"] = "Tool parameter schemas handed to `TestModel.gen_tool_args`."
    docs["GEN_SEEDS"] = "The TestModel seed per schema."
    docs["GEN_EXPECTED"] = "The generated arguments per schema as sorted-key JSON."

    # Agent on TestModel
    def build(model, system_prompt="sys", retry=True):
        agent = Agent(model, system_prompt=system_prompt)

        @agent.tool_plain
        def get_temperature(city: str) -> str:
            """The current temperature of a city in degrees Celsius."""
            if city.lower() == "paris":
                return "21"
            if retry:
                raise ModelRetry(f"Unknown city {city}; known cities are Paris and London")
            return "unknown"

        @agent.tool_plain
        def add(a: int, b: int) -> str:
            """Adds two numbers."""
            return str(a + b)

        return agent

    agent = build(TestModel(), retry=False)
    run = agent.run_sync("go")
    constants["TEST_AGENT_OUTPUT"] = run.output
    constants["TEST_AGENT_MESSAGES"] = [canonical(project(m)) for m in ModelMessagesTypeAdapter.dump_python(run.all_messages(), mode="json")]
    constants["TEST_AGENT_USAGE"] = [run.usage.requests, run.usage.tool_calls]
    constants["TEST_AGENT_SCHEMAS"] = [canonical(t.tool_def.parameters_json_schema) for t in agent._function_toolset.tools.values()]
    docs["TEST_AGENT_OUTPUT"] = "`Agent(TestModel()).run_sync('go').output` with the two tools."
    docs["TEST_AGENT_MESSAGES"] = "`run.all_messages()` projected to the port's wire keys."
    docs["TEST_AGENT_USAGE"] = "`[requests, tool_calls]` of the TestModel run."
    docs["TEST_AGENT_SCHEMAS"] = "The parameter schemas pydantic derives for `get_temperature(city: str)` and `add(a: int, b: int)`."
    custom = build(TestModel(custom_output_text="done"), retry=False).run_sync("go")
    constants["TEST_AGENT_CUSTOM_OUTPUT"] = custom.output
    constants["TEST_AGENT_CUSTOM_REQUESTS"] = custom.usage.requests
    constants["TEST_AGENT_NO_TOOLS_OUTPUT"] = Agent(TestModel(), system_prompt="sys").run_sync("go").output
    docs["TEST_AGENT_CUSTOM_OUTPUT"] = "The output with `custom_output_text='done'`."
    docs["TEST_AGENT_CUSTOM_REQUESTS"] = "Requests made by the custom-output run."
    docs["TEST_AGENT_NO_TOOLS_OUTPUT"] = "The output of a TestModel agent without tools."

    # Scripted OpenAIChatModel agent run
    scripted = [
        reply(text=None, tool_calls=[("get_temperature", '{"city":"Paris"}', "call_1"), ("get_temperature", '{"city":"Rome"}', "call_2")],
              usage={"prompt_tokens": 40, "completion_tokens": 9, "total_tokens": 49}, finish="tool_calls", id_="s1"),
        reply(text=None, tool_calls=[("add", '{"a":21,"b":10}', "call_3")], usage={"prompt_tokens": 60, "completion_tokens": 7, "total_tokens": 67}, finish="tool_calls", id_="s2"),
        reply(text=None, tool_calls=[("add", '{"a":21,"b":10}', "call_3")], usage={"prompt_tokens": 60, "completion_tokens": 7, "total_tokens": 67}, finish="tool_calls", id_="s2"),
        reply(text="The answer is 31.", usage={"prompt_tokens": 80, "completion_tokens": 6, "total_tokens": 86}, id_="s3"),
    ]
    transport = Transport(scripted)
    agent = build(chat_model("gpt-4o", transport))
    run = agent.run_sync("What is the temperature in Paris plus 10?")
    constants["SCRIPTED_REPLIES"] = scripted
    constants["SCRIPTED_OUTPUT"] = run.output
    constants["SCRIPTED_BODIES"] = [canonical(b) for b in transport.bodies]
    constants["SCRIPTED_MESSAGES"] = [canonical(project(m)) for m in ModelMessagesTypeAdapter.dump_python(run.all_messages(), mode="json")]
    constants["SCRIPTED_USAGE"] = [run.usage.requests, run.usage.tool_calls, run.usage.input_tokens, run.usage.output_tokens]
    docs["SCRIPTED_REPLIES"] = "Reply bodies indexed by the number of tool messages in the request: two temperature calls (Rome makes the tool retry), the add call, the text answer."
    docs["SCRIPTED_OUTPUT"] = "The real agent's output over the scripted replies."
    docs["SCRIPTED_BODIES"] = "Every request body the real agent sent, in order, as sorted-key JSON."
    docs["SCRIPTED_MESSAGES"] = "`run.all_messages()` of the scripted run projected to the port's wire keys."
    docs["SCRIPTED_USAGE"] = "`[requests, tool_calls, input_tokens, output_tokens]` of the scripted run."

    # Errors
    error_transport = Transport(['{"error":{"message":"rate limited"}}'])
    try:
        build(chat_model("gpt-4o", error_transport)).run_sync("go")
        raise AssertionError("expected ModelHTTPError")
    except ModelHTTPError as exc:
        constants["HTTP_ERROR_STATUS"] = exc.status_code
        constants["HTTP_ERROR_MODEL"] = exc.model_name
    constants["HTTP_ERROR_MESSAGE"] = str(ModelHTTPError(status_code=429, model_name="gpt-4o", body='{"error":"x"}'))
    try:
        Agent(chat_model("gpt-4o", Transport([scripted[0]])), system_prompt="sys").run_sync("go", usage_limits=UsageLimits(request_limit=1))
    except UsageLimitExceeded as exc:
        constants["USAGE_LIMIT_MESSAGE"] = str(exc).replace("1", "50")
    docs["HTTP_ERROR_STATUS"] = "`ModelHTTPError.status_code` for a 429 reply."
    docs["HTTP_ERROR_MODEL"] = "`ModelHTTPError.model_name` for that reply."
    docs["HTTP_ERROR_MESSAGE"] = "`str(ModelHTTPError(429, 'gpt-4o', body))` with a text body."
    docs["USAGE_LIMIT_MESSAGE"] = "`str(UsageLimitExceeded)` for the default request limit of 50."

    # Profiles
    deepseek_profile = DeepSeekProvider.model_profile("deepseek-reasoner")
    constants["PROFILE_FLAGS"] = [
        deepseek_profile.get("supports_thinking"),
        deepseek_profile.get("thinking_always_enabled"),
        deepseek_profile.get("openai_supports_tool_choice_required"),
        OllamaProvider.model_profile("llama3").get("openai_supports_strict_tool_definition"),
        OpenAIModelProfile(openai_chat_supports_max_completion_tokens=False).get("openai_chat_supports_max_completion_tokens"),
    ]
    constants["PROFILE_TEXTS"] = [
        deepseek_profile.get("openai_chat_thinking_field"),
        deepseek_profile.get("openai_chat_send_back_thinking_parts"),
        ModelProfile().get("openai_system_prompt_role") or "",
    ]
    docs["PROFILE_FLAGS"] = "deepseek-reasoner supports_thinking, thinking_always_enabled, tool_choice_required; ollama strict tool definitions; an explicit max_completion_tokens override."
    docs["PROFILE_TEXTS"] = "deepseek-reasoner thinking field and send-back mode; the default system prompt role (empty for None)."

    expected_module.write(
        "pydantic_ai",
        "Expected values for the @osprey/pydantic_ai suite, computed by the real pydantic-ai over an httpx mock transport.",
        {"pydantic-ai": pydantic_ai.__version__, "openai": openai.__version__, "httpx": httpx.__version__},
        constants,
        docs,
    )


if __name__ == "__main__":
    main()
