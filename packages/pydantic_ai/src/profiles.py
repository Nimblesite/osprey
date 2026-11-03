# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Profiles
"""pydantic_ai.profiles: what a model supports and how its API is spoken.

Port of pydantic_ai/profiles/__init__.py, profiles/openai.py, profiles/deepseek.py,
profiles/moonshotai.py and profiles/zai.py (2.46.0). `ModelProfile` is a dataclass in
CPython; here it is a JSON-valued map keyed by the dataclass field names, so
`OpenAIModelProfile(openai_chat_supports_max_completion_tokens=False)` is
spelled `openai_model_profile_with({"openai_chat_supports_max_completion_tokens": JsonBool(False)})`
and `merge_profile` is the dict spread CPython performs.
"""

from pydantic_ai import JsonUtil as J

__all__ = [
    "ModelProfile", "model_profile", "merge_profile", "profile_bool", "profile_text",
    "openai_model_profile", "openai_model_profile_with", "deepseek_model_profile", "ollama_model_profile",
    "moonshotai_model_profile", "moonshotai_provider_profile", "zai_model_profile", "zai_provider_profile",
    "thinking_levels", "thinking_setting",
]

type ModelProfile = dict[str, Json]
"""A model profile keyed as CPython's `ModelProfile` dataclass fields."""


def model_profile():
    """`ModelProfile()`: the dataclass defaults every model shares."""
    return {
        "supports_tools": JsonBool(True),
        "supports_json_schema_output": JsonBool(False),
        "supports_json_object_output": JsonBool(False),
        "supports_image_output": JsonBool(False),
        "supports_thinking": JsonBool(False),
        "thinking_always_enabled": JsonBool(False),
        "ignore_streamed_leading_whitespace": JsonBool(False),
        "openai_supports_strict_tool_definition": JsonBool(True),
        "openai_supports_sampling_settings": JsonBool(True),
        "openai_supports_tool_choice_required": JsonBool(True),
        "openai_chat_supports_max_completion_tokens": JsonBool(True),
        "json_schema_transformer": JsonString(""),
        "openai_system_prompt_role": JsonString(""),
        "openai_chat_thinking_field": JsonString(""),
        "openai_chat_send_back_thinking_parts": JsonString(""),
        "openai_supports_forced_tool_choice_with_thinking": JsonBool(True),
        "openai_reasoning_enabled_by_default": JsonBool(False),
        "zai_supports_reasoning_effort": JsonBool(False),
        "zai_reasoning_effort_mapping": JsonObject([]),
        "zai_thinking_body": JsonBool(False),
    }


def merge_profile(base, overrides):
    """`merge_profile(base, overrides)`: the overrides' keys replace the base's."""
    return mapMerge(base, overrides)


def profile_bool(profile, key, fallback) -> bool:
    """The boolean under `key`, `fallback` when absent."""
    return unwrap_or(jsonBool(unwrap_or(mapGet(profile, key), JsonNull())), fallback)


def profile_text(profile, key, fallback) -> str:
    """The string under `key`, `fallback` when absent or empty."""
    text = unwrap_or(jsonText(unwrap_or(mapGet(profile, key), JsonNull())), fallback)
    return fallback if text == "" else text


def openai_model_profile(model_name):
    """`openai_model_profile(model_name)`: the OpenAI profile of a model name."""
    reasoning = startsWith(model_name, "o1") or startsWith(model_name, "o3") or startsWith(model_name, "o4") or startsWith(model_name, "gpt-5")
    return merge_profile(model_profile(), {
        "json_schema_transformer": JsonString("openai"),
        "supports_json_schema_output": JsonBool(True),
        "supports_json_object_output": JsonBool(True),
        "supports_thinking": JsonBool(reasoning),
        "openai_supports_sampling_settings": JsonBool(not reasoning),
        "openai_system_prompt_role": JsonString("user" if startsWith(model_name, "o1-mini") else ""),
    })


def openai_model_profile_with(overrides):
    """`OpenAIModelProfile(**overrides)` merged over the shared defaults."""
    return merge_profile(model_profile(), overrides)


def deepseek_model_profile(model_name):
    """`DeepSeekProvider.model_profile(model_name)`: DeepSeek's chat API quirks."""
    is_r1 = model_name == "r1" or startsWith(model_name, "deepseek-r1") or model_name == "deepseek-reasoner"
    is_v4 = startsWith(model_name, "deepseek-v4-")
    return merge_profile(model_profile(), {
        "json_schema_transformer": JsonString("openai"),
        "ignore_streamed_leading_whitespace": JsonBool(is_r1),
        "supports_thinking": JsonBool(is_r1 or is_v4),
        "thinking_always_enabled": JsonBool(is_r1),
        "supports_json_object_output": JsonBool(True),
        "openai_chat_thinking_field": JsonString("reasoning_content"),
        "openai_chat_send_back_thinking_parts": JsonString("field"),
        "openai_supports_tool_choice_required": JsonBool(model_name != "deepseek-reasoner"),
        "openai_supports_forced_tool_choice_with_thinking": JsonBool(not is_v4),
        "openai_reasoning_enabled_by_default": JsonBool(is_v4 or model_name == "deepseek-reasoner"),
    })


def ollama_model_profile(_model_name):
    """`OllamaProvider.model_profile(model_name)`: the overrides Ollama needs."""
    return merge_profile(model_profile(), {
        "supports_json_schema_output": JsonBool(True),
        "supports_json_object_output": JsonBool(True),
        "openai_supports_strict_tool_definition": JsonBool(False),
        "openai_chat_thinking_field": JsonString("reasoning"),
    })


KIMI_REASONING_PREFIXES = ["kimi-k2.5", "kimi-k2.6", "kimi-k2.7", "kimi-k2-thinking", "kimi-k3", "kimi-thinking"]
"""The Kimi model-name prefixes that reason and emit `reasoning_content`."""


def kimi_reasoning(model_name):
    """Whether a Kimi model name is one of the reasoning prefixes."""
    lowered = toLowerCase(model_name)
    return len(filterList(KIMI_REASONING_PREFIXES, lambda prefix: startsWith(lowered, prefix))) > 0


def moonshotai_model_profile(model_name):
    """`moonshotai_model_profile(model_name)`: the Kimi model-name profile, before the provider's overrides."""
    return merge_profile(model_profile(), {
        "ignore_streamed_leading_whitespace": JsonBool(True),
        "supports_thinking": JsonBool(kimi_reasoning(model_name)),
    })


def moonshotai_provider_profile(model_name):
    """`MoonshotAIProvider.model_profile(model_name)`: the OpenAI-compatible base, the Kimi profile, then
    the endpoint's overrides — no `tool_choice='required'`, `reasoning_content` sent back as a field, and
    reasoning always on, so a `thinking` of `false` omits `reasoning_effort` instead of sending `'none'`."""
    return merge_profile(moonshotai_model_profile(model_name), {
        "json_schema_transformer": JsonString("openai"),
        "supports_json_object_output": JsonBool(True),
        "openai_supports_tool_choice_required": JsonBool(False),
        "openai_chat_thinking_field": JsonString("reasoning_content"),
        "openai_chat_send_back_thinking_parts": JsonString("field"),
        "thinking_always_enabled": JsonBool(kimi_reasoning(model_name)),
    })


GLM_THINKING_PREFIXES = ["glm-5", "glm-4.7", "glm-4.6", "glm-4.5"]
"""The GLM model-name prefixes that think; GLM-5.2 and GLM-5.3 additionally take an effort level."""


def glm_thinking(model_name):
    lowered = toLowerCase(model_name)
    return len(filterList(GLM_THINKING_PREFIXES, lambda prefix: startsWith(lowered, prefix))) > 0


def glm_53(model_name):
    return startsWith(toLowerCase(model_name), "glm-5.3")


def glm_effort(model_name):
    lowered = toLowerCase(model_name)
    return glm_53(model_name) or startsWith(lowered, "glm-5.2")


def zai_reasoning_effort_mapping():
    """`_GLM_5_3_REASONING_EFFORT_MAPPING` made total: GLM-5.3 takes only `low`, `high` and `max`, so every
    unified level names the wire level it becomes."""
    return JsonObject([
        JsonMember("minimal", JsonString("low")),
        JsonMember("low", JsonString("low")),
        JsonMember("medium", JsonString("high")),
        JsonMember("high", JsonString("high")),
        JsonMember("xhigh", JsonString("max")),
    ])


def zai_model_profile(model_name):
    """`zai_model_profile(model_name)`: the GLM model-name profile (the shared defaults for a name that is not a thinking GLM)."""
    if not glm_thinking(model_name):
        return model_profile()
    else:
        overrides = {
            "supports_thinking": JsonBool(True),
            "thinking_always_enabled": JsonBool(glm_53(model_name)),
            "zai_supports_reasoning_effort": JsonBool(glm_effort(model_name)),
        }
        with_mapping = mapSet(overrides, "zai_reasoning_effort_mapping", zai_reasoning_effort_mapping()) if glm_53(model_name) else overrides
        return merge_profile(model_profile(), with_mapping)


def zai_provider_profile(model_name):
    """`ZaiProvider.model_profile(model_name)`: the OpenAI-compatible base, the GLM profile, then the
    endpoint's overrides. `zai_thinking_body` (not upstream: the port has one model class where upstream
    has `ZaiModel`) marks Z.AI's request shape, where thinking travels as `extra_body`'s `thinking` object
    rather than the OpenAI `reasoning_effort` parameter."""
    return merge_profile(zai_model_profile(model_name), {
        "json_schema_transformer": JsonString("openai"),
        "supports_json_object_output": JsonBool(True),
        "openai_chat_thinking_field": JsonString("reasoning_content"),
        "openai_chat_send_back_thinking_parts": JsonString("field"),
        "zai_thinking_body": JsonBool(True),
    })


EFFORT_LEVELS = ["minimal", "low", "medium", "high", "xhigh"]
"""CPython's `ThinkingLevel` strings, in order."""


def thinking_levels(profile, provider_kind) -> list[str]:
    """Not upstream (an Osprey addition): the effort choices a person may pick for this model, as text, in
    order — `[]` when the model does not think or always thinks with no choice of level. Kimi reasoning
    models are never off; GLM-5.3 takes its mapped `low`, `high`, `max`; GLM-5.2 every level with `off`;
    older thinking GLM only `off` and `on`; DeepSeek v4 `off`, `on` and every level upstream forwards."""
    supports = profile_bool(profile, "supports_thinking", False)
    always = profile_bool(profile, "thinking_always_enabled", False)
    if not supports and not always:
        return []
    elif provider_kind == "moonshotai":
        return ["minimal", "low", "medium", "high"]
    elif provider_kind == "zai":
        return zai_thinking_levels(profile)
    elif provider_kind == "deepseek":
        if always:
            return []
        else:
            return listConcat(["off", "on"], EFFORT_LEVELS)
    else:
        if always:
            return []
        else:
            return listPrepend(EFFORT_LEVELS, "off")


def zai_thinking_levels(profile):
    if not profile_bool(profile, "zai_supports_reasoning_effort", False):
        return ["off", "on"]
    else:
        mapping = effort_map(profile)
        if len(mapKeys(mapping)) == 0:
            return listPrepend(EFFORT_LEVELS, "off")
        else:
            return distinct(mapList(EFFORT_LEVELS, lambda level: mapped_effort(mapping, level)))


def effort_map(profile) -> dict[str, str]:
    """`zai_reasoning_effort_mapping` as a text map."""
    raw = unwrap_or(mapGet(profile, "zai_reasoning_effort_mapping"), JsonNull())
    table = J.to_map(raw)
    return foldList(mapKeys(table), {}, lambda acc, key: mapSet(acc, key, J.text_at(raw, key, "")))


def mapped_effort(mapping, level):
    """The wire effort a unified level maps to; the level itself when the map has none (upstream's forward)."""
    return unwrap_or(mapGet(mapping, level), level)


def distinct(levels) -> list[str]:
    """A list's first occurrences, in order."""
    return foldList(levels, [], lambda acc, level: distinct_step(acc, level))


def distinct_step(acc, level):
    if listContains(acc, level):
        return acc
    else:
        return listAppend(acc, level)


def thinking_setting(level):
    """Not upstream (an Osprey addition): the JSON `thinking` value a picked level text spells — `off` and
    `on` are the booleans, `max` is GLM-5.3's mapped `xhigh`, a level is itself, anything else null."""
    if level == "off":
        return JsonBool(False)
    elif level == "on":
        return JsonBool(True)
    elif level == "max":
        return JsonString("xhigh")
    elif listContains(EFFORT_LEVELS, level):
        return JsonString(level)
    else:
        return JsonNull()
