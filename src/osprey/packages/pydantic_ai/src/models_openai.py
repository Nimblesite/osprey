# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=ModelsOpenai
"""pydantic_ai.models.openai: `OpenAIChatModel` over the Chat Completions API.

Port of pydantic_ai/models/openai.py (2.46.0), the chat half: `_map_messages`
is `map_messages`, `_map_user_message` is `map_request`, the
`_MapModelResponseContext` is `map_response`, `_map_tool_definition` is
`map_tool_definition`, `_completions_create`'s request is `completions_body`,
`_process_response` is `process_response` with `_map_usage` as `map_usage`
and `_CHAT_FINISH_REASON_MAP` as `map_finish_reason`. The Responses API,
streaming, files, structured output, logprobs and web search are not ported.
"""
from pydantic_ai import Exceptions as E
from pydantic_ai import JsonSchema
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from pydantic_ai import Models
from pydantic_ai import Profiles
from pydantic_ai import Providers
from pydantic_ai import Settings
from pydantic_ai import Usage

__all__ = [
    "OpenAIChatModel", "map_messages", "map_request", "map_response", "map_tool_definition",
    "completions_body", "process_response", "map_usage", "map_finish_reason", "chat_request",
]


def provider_profile(provider, model_name):
    """`Provider.model_profile(model_name)` for the ported providers. A provider serving one of these
    APIs under another name (a coding plan endpoint) passes the provider profile function's result as the
    model's `profile` argument instead, so no further names are needed here."""
    if provider.name == "deepseek":
        return Profiles.deepseek_model_profile(model_name)
    elif provider.name == "ollama":
        return Profiles.ollama_model_profile(model_name)
    elif provider.name == "moonshotai":
        return Profiles.moonshotai_provider_profile(model_name)
    elif provider.name == "zai":
        return Profiles.zai_provider_profile(model_name)
    else:
        return Profiles.openai_model_profile(model_name)


def OpenAIChatModel(model_name, provider, profile):
    """`OpenAIChatModel(model_name, provider=provider, profile=profile)`: `profile` overrides the provider's profile for the model; pass `{}` for none."""
    merged = Profiles.merge_profile(provider_profile(provider, model_name), profile)
    return Models.model(model_name, provider.name, merged, lambda messages, settings, parameters: chat_request(model_name, provider, merged, messages, settings, parameters))


def chat_request(model_name, provider, profile, messages, settings, parameters) -> E.Outcome[M.ModelResponse]:
    """`OpenAIChatModel.request`: POST /chat/completions and map the reply."""
    body = J.encode(completions_body(model_name, profile, messages, settings, parameters))
    match provider.post("/chat/completions", body, ""):
        case Success(value=reply):
            if reply.status >= 400:
                return E.Failed(error=E.status_error(reply.status, model_name, reply.body))
            else:
                return process_response(model_name, provider.name, profile, reply.body)
        case Error(message=message):
            return E.Failed(error=E.ModelAPIError(model_name=model_name, message=message))


def system_prompt_role(profile):
    return Profiles.profile_text(profile, "openai_system_prompt_role", "system")


def role_message(role, content):
    return J.obj([J.member("role", JsonString(role)), J.member("content", JsonString(content))])


def tool_message(tool_call_id, content):
    return J.obj([J.member("role", JsonString("tool")), J.member("tool_call_id", JsonString(tool_call_id)), J.member("content", JsonString(content))])


def map_request_part(profile, part):
    match part:
        case M.SystemPromptPart(content=content):
            return role_message(system_prompt_role(profile), content)
        case M.UserPromptPart(content=content):
            return role_message("user", content)
        case M.ToolReturnPart(content=content, tool_call_id=tool_call_id):
            return tool_message(tool_call_id, content)
        case M.RetryPromptPart(tool_name=tool_name, tool_call_id=tool_call_id):
            return role_message("user", M.retry_model_response(part)) if tool_name == "" else tool_message(tool_call_id, M.retry_model_response(part))
        case M.InstructionPart(content=content):
            return role_message(system_prompt_role(profile), content)


def is_instruction(part):
    match part:
        case M.InstructionPart():
            return True
        case _:
            return False


def map_request(profile, request) -> list[Json]:
    """`_map_user_message`: one chat message per part (instruction parts travel through the parameters instead)."""
    return mapList(filterList(request.parts, lambda p: not is_instruction(p)), lambda p: map_request_part(profile, p))


def map_tool_call(part):
    return J.obj([
        J.member("id", JsonString(M.tool_call_id_of(part))),
        J.member("type", JsonString("function")),
        J.member("function", J.obj([J.member("name", JsonString(M.tool_name_of(part))), J.member("arguments", JsonString(M.args_as_json_str(part)))])),
    ])


def thinking_text(part):
    match part:
        case M.ThinkingPart(content=content):
            return content
        case _:
            return ""


THINK_START = "<think>"
THINK_END = "</think>"


def sent_text(profile, part):
    """`_map_response_text_part`/`_map_response_thinking_part`: a text part's content; a thinking part wrapped in the thinking tags unless the profile sends it back as a field."""
    send_back = Profiles.profile_text(profile, "openai_chat_send_back_thinking_parts", "auto")
    match part:
        case M.TextPart(content=content):
            return content
        case M.ThinkingPart(content=content):
            return "" if send_back == "field" else join([THINK_START, content, THINK_END], "\n")
        case M.ToolCallPart():
            return ""


def map_response(profile, response) -> list[Json]:
    """`_MapModelResponseContext.map_assistant_message`: one assistant message, or none when the response is empty."""
    texts = filterList(mapList(response.parts, lambda p: sent_text(profile, p)), lambda s: s != "")
    thinkings = filterList(mapList(response.parts, thinking_text), lambda s: s != "")
    calls = mapList(M.tool_calls(response), map_tool_call)
    thinking_field = Profiles.profile_text(profile, "openai_chat_thinking_field", "")
    send_back = Profiles.profile_text(profile, "openai_chat_send_back_thinking_parts", "auto")
    if len(texts) == 0 and len(calls) == 0:
        return []
    else:
        base = [J.member("role", JsonString("assistant"))]
        with_thinking = listAppend(base, J.member(thinking_field, JsonString(join(thinkings, "\n\n")))) if len(thinkings) > 0 and thinking_field != "" and send_back == "field" else base
        with_content = listAppend(with_thinking, J.member("content", JsonString(join(texts, "\n\n")) if len(texts) > 0 else JsonNull()))
        with_calls = listAppend(with_content, J.member("tool_calls", JsonArray(calls))) if len(calls) > 0 else with_content
        return [J.obj(with_calls)]


def split_thinking(content):
    """`split_content_into_text_and_thinking`: `<think>` sections become thinking parts, the rest text parts."""
    if content == "":
        return []
    else:
        match indexOf(content, THINK_START):
            case Success(value=start):
                before = take(content, start)
                rest = drop(content, start + length(THINK_START))
                head = [] if before == "" else [M.TextPart(content=before)]
                match indexOf(rest, THINK_END):
                    case Success(value=end):
                        return listConcat(listAppend(head, M.ThinkingPart(content=take(rest, end))), split_thinking(drop(rest, end + length(THINK_END))))
                    case Error(message=_m):
                        return listAppend(head, M.TextPart(content=rest))
            case Error(message=_m):
                return [M.TextPart(content=content)]


def has_thinking(response):
    return len(filterList(mapList(response.parts, thinking_text), lambda s: s != "")) > 0


def backfill(profile, mapped):
    """`_map_messages`' backfill: an assistant message with tool calls and no thinking field gets an empty one, so a provider that must see reasoning on every turn is satisfied."""
    field = Profiles.profile_text(profile, "openai_chat_thinking_field", "")
    needs = J.text_at(mapped, "role", "") == "assistant" and len(J.list_at(mapped, "tool_calls")) > 0 and not J.has(mapped, field)
    return J.obj(listAppend(J.members_of(mapped), J.member(field, JsonString("")))) if needs else mapped


def map_message(profile, backfill_field, message):
    match message:
        case M.Request(request=request):
            return map_request(profile, request)
        case M.Response(response=response):
            return mapList(map_response(profile, response), lambda m: backfill(profile, m) if backfill_field else m)


def count_leading(mapped, role, i):
    if i >= len(mapped):
        return i
    elif J.text_at(unwrap_or(listGet(mapped, i), JsonNull()), "role", "") == role:
        return count_leading(mapped, role, i + 1)
    else:
        return i


def map_messages(profile, messages, parameters) -> list[Json]:
    """`_map_messages`: the chat messages, with the instructions inserted after the leading system messages."""
    thinking_active = len(filterList(M.responses(messages), has_thinking)) > 0
    backfill_field = thinking_active and Profiles.profile_text(profile, "openai_chat_send_back_thinking_parts", "auto") == "field" and Profiles.profile_text(profile, "openai_chat_thinking_field", "") != ""
    mapped = foldList(messages, [], lambda acc, m: listConcat(acc, map_message(profile, backfill_field, m)))
    instructions = Models.instructions_text(messages, parameters)
    if instructions == "":
        return mapped
    else:
        role = system_prompt_role(profile)
        n = count_leading(mapped, role, 0)
        head = filterList(M.map_indexed(mapped, lambda i: unwrap_or(listGet(mapped, i), JsonNull()) if i < n else JsonNull()), lambda m: not J.is_null(m))
        tail = filterList(M.map_indexed(mapped, lambda i: unwrap_or(listGet(mapped, i), JsonNull()) if i >= n else JsonNull()), lambda m: not J.is_null(m))
        return listConcat(listAppend(head, role_message(role, instructions)), tail)


def map_tool_definition(profile, tool) -> Json:
    """`_customize_tool_def` then `_map_tool_definition`: the walked schema as the `function` tool parameter, `strict` when the tool asks for it or the schema allows it."""
    has_transformer = Profiles.profile_text(profile, "json_schema_transformer", "") == "openai"
    transformed = JsonSchema.transform(tool.parameters_json_schema) if has_transformer else JsonSchema.Transformed(schema=tool.parameters_json_schema, is_strict_compatible=False)
    function = [J.member("name", JsonString(tool.name)), J.member("description", JsonString(tool.description)), J.member("parameters", transformed.schema)]
    strict = (tool.strict or transformed.is_strict_compatible) and Profiles.profile_bool(profile, "openai_supports_strict_tool_definition", True)
    return J.obj([J.member("type", JsonString("function")), J.member("function", J.obj(listAppend(function, J.member("strict", JsonBool(True))) if strict else function))])


def setting_member(settings, key, name):
    """The request member `name` from setting `key`, nothing when the setting is absent."""
    match mapGet(settings, key):
        case Success(value=value):
            return [J.member(name, value)]
        case Error(message=_m):
            return []


def completions_body(model_name, profile, messages, settings, parameters) -> Json:
    """`_completions_create`'s request body: model, messages, tools, tool_choice and the given settings;
    the unified `thinking` setting becomes `reasoning_effort` (Z.AI's profile sends it as `extra_body`
    instead, where `ZaiModel.prepare_request` builds the `thinking` object)."""
    tools = mapList(parameters.function_tools, lambda t: map_tool_definition(profile, t))
    supports_max_completion = Profiles.profile_bool(profile, "openai_chat_supports_max_completion_tokens", True)
    sampling = Profiles.profile_bool(profile, "openai_supports_sampling_settings", True)
    base = [J.member("model", JsonString(model_name)), J.member("messages", JsonArray(map_messages(profile, messages, parameters)))]
    with_tools = listConcat(base, [J.member("tools", JsonArray(tools)), J.member("tool_choice", JsonString(tool_choice(profile, settings, parameters)))]) if len(tools) > 0 else base
    with_stream = listAppend(with_tools, J.member("stream", JsonBool(False)))
    members = listConcat(with_stream, listConcat(
        listConcat(setting_member(settings, "stop_sequences", "stop"), setting_member(settings, "max_tokens", "max_completion_tokens" if supports_max_completion else "max_tokens")),
        listConcat(setting_member(settings, "seed", "seed"), sampling_members(settings) if sampling else []),
    ))
    zai = Profiles.profile_bool(profile, "zai_thinking_body", False)
    with_effort = members if zai else listConcat(members, reasoning_members(profile, settings))
    extra = zai_extra(profile, settings) if zai else extra_body_members(settings)
    names = mapList(extra, lambda member: J.member_name(member))
    return J.obj(listConcat(filterList(with_effort, lambda member: not listContains(names, J.member_name(member))), extra))


def sampling_members(settings):
    return listConcat(
        listConcat(setting_member(settings, "temperature", "temperature"), setting_member(settings, "top_p", "top_p")),
        listConcat(setting_member(settings, "presence_penalty", "presence_penalty"), setting_member(settings, "frequency_penalty", "frequency_penalty")),
    )


def tool_choice(profile, settings, parameters):
    """`_get_tool_choice`'s `required` branch: `required` unless the provider rejects it (Kimi), or the
    model rejects forcing while reasoning is active (`_support_tool_forcing`'s DeepSeek V4 rule)."""
    if parameters.allow_text_output or not Profiles.profile_bool(profile, "openai_supports_tool_choice_required", True):
        return "auto"
    elif Profiles.profile_bool(profile, "openai_supports_forced_tool_choice_with_thinking", True) or not reasoning_active(profile, settings):
        return "required"
    else:
        return "auto"


def resolved_thinking(profile, settings):
    """`Model.prepare_request`'s `thinking` resolution: the setting's value when the model takes one,
    `JsonNull()` when it does not — a model that cannot think ignores the setting, and `false` on a model
    that always thinks is dropped rather than sent."""
    match mapGet(settings, "thinking"):
        case Error(message=_m):
            return JsonNull()
        case Success(value=value):
            always = Profiles.profile_bool(profile, "thinking_always_enabled", False)
            takes = Profiles.profile_bool(profile, "supports_thinking", False) or always
            return value if takes and not (always and thinking_off(value)) else JsonNull()


def thinking_off(value):
    """Whether a resolved thinking value is `false`."""
    return not unwrap_or(jsonBool(value), True)


def reasoning_active(profile, settings):
    """`_reasoning_active`: an explicit `openai_reasoning_effort` wins, then the resolved `thinking`
    setting, then the profile's `openai_reasoning_enabled_by_default`."""
    effort = Settings.setting_text(settings, "openai_reasoning_effort", "")
    if effort != "":
        return effort != "none"
    else:
        value = resolved_thinking(profile, settings)
        return not thinking_off(value) if not J.is_null(value) else Profiles.profile_bool(profile, "openai_reasoning_enabled_by_default", False)


def reasoning_members(profile, settings):
    """`_translate_thinking`: an explicit `openai_reasoning_effort` wins; otherwise the unified `thinking`
    setting becomes `reasoning_effort` through `OPENAI_REASONING_EFFORT_MAP` (`true` is `medium`, `false` is
    `none`)."""
    effort = Settings.setting_text(settings, "openai_reasoning_effort", "")
    if effort != "":
        return [J.member("reasoning_effort", JsonString(effort))]
    else:
        value = resolved_thinking(profile, settings)
        return [] if J.is_null(value) else [J.member("reasoning_effort", JsonString(effort_of(value)))]


def effort_of(value):
    """`OPENAI_REASONING_EFFORT_MAP`: a thinking value as the effort the API takes — `true` is `medium`,
    `false` is `none`, a level is itself."""
    match value:
        case JsonString(level):
            return level
        case _:
            return "medium" if unwrap_or(jsonBool(value), False) else "none"


def zai_extra(profile, settings):
    """Z.AI's request `extra_body`: the computed `thinking` and `reasoning_effort` replace the same-named
    members of the user's `extra_body`, as `_zai_settings_to_openai_settings` overwrites those keys in the
    dict it hands the SDK (and the SDK's `extra_body` merge then replaces top-level members of the same name)."""
    computed = zai_extra_members(profile, settings)
    computed_names = mapList(computed, J.member_name)
    kept = filterList(extra_body_members(settings), lambda member: not listContains(computed_names, J.member_name(member)))
    return listConcat(kept, computed)


def zai_extra_members(profile, settings):
    """`_zai_settings_to_openai_settings`: the `extra_body` members Z.AI's endpoint takes — `thinking`
    with its `type` and `clear_thinking`, and `reasoning_effort` mapped from the level on the models that
    accept one. These merge over (and replace same-named members of) the request body and the given
    `extra_body`, as the OpenAI SDK's `extra_body` merge does."""
    return listConcat(zai_thinking_members(profile, settings), zai_effort_member(profile, settings))


def zai_thinking_members(profile, settings):
    payload = listConcat(zai_type_members(profile, settings), zai_clear_members(profile, settings))
    return [J.member("thinking", J.obj(payload))] if len(payload) > 0 else []


def zai_type_members(profile, settings):
    """The payload's `type`: `enabled` or `disabled` for a thinking setting, absent when none was given."""
    value = resolved_thinking(profile, settings)
    if J.is_null(value):
        return []
    else:
        return [J.member("type", JsonString("disabled" if thinking_off(value) else "enabled"))]


def zai_clear_members(profile, settings):
    """`zai_clear_thinking`: the setting when given, else `false` on a thinking-capable model (preserved
    thinking), else absent."""
    match mapGet(settings, "zai_clear_thinking"):
        case Success(value=value):
            match value:
                case JsonBool(clear):
                    return [J.member("clear_thinking", JsonBool(clear))]
                case _:
                    return []
        case Error(message=_m):
            return [J.member("clear_thinking", JsonBool(False))] if Profiles.profile_bool(profile, "supports_thinking", False) else []


def zai_effort_member(profile, settings):
    """The payload's `reasoning_effort`: the level through the profile's mapping, on the models that take
    one (`_zai_settings_to_openai_settings` forwards an explicit level, never a bare `true`/`false`)."""
    if not Profiles.profile_bool(profile, "zai_supports_reasoning_effort", False):
        return []
    else:
        value = resolved_thinking(profile, settings)
        match value:
            case JsonString(level):
                mapping = J.to_map(unwrap_or(mapGet(profile, "zai_reasoning_effort_mapping"), JsonNull()))
                return [J.member("reasoning_effort", JsonString(unwrap_or(jsonText(unwrap_or(mapGet(mapping, level), JsonNull())), level)))]
            case _:
                return []


def extra_body_members(settings):
    """`extra_body`: its members are added to the request body, and replace any of the same name."""
    return J.members_of(unwrap_or(mapGet(settings, "extra_body"), JsonNull()))


def map_finish_reason(key) -> str:
    """`_CHAT_FINISH_REASON_MAP`: the API's finish reason as pydantic-ai's; unknown keys map to empty (CPython's None)."""
    if key == "stop":
        return "stop"
    elif key == "length":
        return "length"
    elif key == "tool_calls":
        return "tool_call"
    elif key == "content_filter":
        return "content_filter"
    elif key == "function_call":
        return "tool_call"
    else:
        return ""


def int_members(value):
    """The integer-valued members of a JSON object."""
    empty: dict[str, int] = {}
    return foldList(J.members_of(value), empty, int_member_step)


def int_member_step(acc, m):
    match m:
        case JsonMember(name, value):
            match jsonInt(value):
                case Success(value=i):
                    return mapSet(acc, name, i)
                case Error(message=_e):
                    return acc


def map_usage(response) -> Usage.RequestUsage:
    """`_map_usage` for a chat completion: prompt/completion tokens, cached prompt tokens, every other integer as a detail."""
    usage = J.object_at(response, "usage")
    if J.is_null(usage):
        return Usage.request_usage()
    else:
        top = int_members(usage)
        details = foldList(["prompt_tokens", "completion_tokens", "total_tokens"], top, lambda acc, key: mapRemove(acc, key))
        with_completion = Usage.details_merge(details, int_members(J.object_at(usage, "completion_tokens_details")))
        return Usage.RequestUsage(
            input_tokens=J.int_at(usage, "prompt_tokens", 0),
            cache_write_tokens=0,
            cache_read_tokens=J.int_at(J.object_at(usage, "prompt_tokens_details"), "cached_tokens", 0),
            output_tokens=J.int_at(usage, "completion_tokens", 0),
            input_audio_tokens=J.int_at(J.object_at(usage, "prompt_tokens_details"), "audio_tokens", 0),
            cache_audio_read_tokens=0,
            output_audio_tokens=J.int_at(J.object_at(usage, "completion_tokens_details"), "audio_tokens", 0),
            details=with_completion,
        )


def map_tool_call_part(call):
    function = J.object_at(call, "function")
    return M.ToolCallPart(tool_name=J.text_at(function, "name", ""), args=J.text_at(function, "arguments", ""), tool_call_id=J.text_at(call, "id", ""))


def thinking_parts(profile, message):
    """`_process_thinking`: the first non-empty of the profile's thinking field, `reasoning`, `reasoning_content`."""
    custom = Profiles.profile_text(profile, "openai_chat_thinking_field", "")
    fields = filterList([custom, "reasoning", "reasoning_content"], lambda f: f != "")
    found = filterList(mapList(fields, lambda f: J.text_at(message, f, "")), lambda t: t != "")
    return [] if len(found) == 0 else [M.ThinkingPart(content=unwrap_or(listGet(found, 0), ""))]


def process_response(model_name, provider_name, profile, body) -> E.Outcome[M.ModelResponse]:
    """`_process_response`: the first choice's thinking, content and tool calls, usage, model and id."""
    match J.decode(body):
        case Success(value=data):
            choice = unwrap_or(listGet(J.list_at(data, "choices"), 0), JsonNull())
            message = J.object_at(choice, "message")
            if J.is_null(message):
                return E.Failed(error=E.UnexpectedModelBehavior(message=f"Invalid response from {provider_name} chat completions endpoint, expected JSON data", body=body))
            else:
                content = J.text_at(message, "content", "")
                texts = split_thinking(content)
                calls = mapList(J.list_at(message, "tool_calls"), map_tool_call_part)
                finish = J.text_at(choice, "finish_reason", "stop")
                return E.Ok(value=M.ModelResponse(
                    parts=listConcat(listConcat(thinking_parts(profile, message), texts), calls),
                    usage=map_usage(data),
                    model_name=J.text_at(data, "model", model_name),
                    provider_name=provider_name,
                    provider_response_id=J.text_at(data, "id", ""),
                    finish_reason=map_finish_reason(finish),
                ))
        case Error(message=message):
            return E.Failed(error=E.UnexpectedModelBehavior(message=f"Invalid response from {provider_name} chat completions endpoint, expected JSON data: {message}", body=body))
