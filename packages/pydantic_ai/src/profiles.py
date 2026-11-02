# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Profiles
"""pydantic_ai.profiles: what a model supports and how its API is spoken.

Port of pydantic_ai/profiles/__init__.py, profiles/openai.py and
profiles/deepseek.py (2.46.0). `ModelProfile` is a dataclass in CPython; here
it is a JSON-valued map keyed by the dataclass field names, so
`OpenAIModelProfile(openai_chat_supports_max_completion_tokens=False)` is
spelled `openai_model_profile_with({"openai_chat_supports_max_completion_tokens": JsonBool(False)})`
and `merge_profile` is the dict spread CPython performs.
"""

__all__ = [
    "ModelProfile", "model_profile", "merge_profile", "profile_bool", "profile_text",
    "openai_model_profile", "openai_model_profile_with", "deepseek_model_profile", "ollama_model_profile",
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
    })


def ollama_model_profile(_model_name):
    """`OllamaProvider.model_profile(model_name)`: the overrides Ollama needs."""
    return merge_profile(model_profile(), {
        "supports_json_schema_output": JsonBool(True),
        "supports_json_object_output": JsonBool(True),
        "openai_supports_strict_tool_definition": JsonBool(False),
        "openai_chat_thinking_field": JsonString("reasoning"),
    })
