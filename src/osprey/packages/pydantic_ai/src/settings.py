# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Settings
"""pydantic_ai.settings: the per-request model settings.

Port of pydantic_ai/settings.py (2.46.0). `ModelSettings` is a TypedDict in
CPython; here it is a JSON-valued map with the same keys (`max_tokens`,
`temperature`, `top_p`, `seed`, `stop_sequences`, `presence_penalty`,
`frequency_penalty`, `timeout`, `thinking`, `extra_body`...). A missing key is
CPython's `None`.
"""

from pydantic_ai import JsonUtil as J

__all__ = ["ModelSettings", "model_settings", "merge_model_settings", "setting_int", "setting_float", "setting_text", "has_setting"]

type ModelSettings = dict[str, Json]
"""Settings a model request honours, keyed as CPython's TypedDict."""


def model_settings():
    """`ModelSettings()`: no setting given."""
    empty: dict[str, Json] = {}
    return empty


def merge_model_settings(base, overrides):
    """`merge_model_settings`: `overrides` win over `base`, key by key."""
    return mapMerge(base, overrides)


def has_setting(settings, key) -> bool:
    """Whether `key` was given."""
    return mapContains(settings, key)


def setting(settings, key):
    """The value under `key`, JSON null when absent (CPython's `None`)."""
    return unwrap_or(mapGet(settings, key), JsonNull())


def setting_int(settings, key, fallback) -> int:
    """The integer setting under `key`, `fallback` when absent or not an integer."""
    return unwrap_or(jsonInt(setting(settings, key)), fallback)


def setting_float(settings, key, fallback) -> float:
    """The number setting under `key`, `fallback` when absent or not a number."""
    return J.number(setting(settings, key), fallback)


def setting_text(settings, key, fallback) -> str:
    """The string setting under `key`, `fallback` when absent or not a string."""
    return unwrap_or(jsonText(setting(settings, key)), fallback)
