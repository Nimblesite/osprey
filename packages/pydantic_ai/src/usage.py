# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Usage
"""pydantic_ai.usage: token counts of one request and of a whole run.

Port of pydantic_ai/usage.py (2.46.0). `RequestUsage` and `RunUsage` are
closed records, so the CPython defaults are spelled by `request_usage()` and
`run_usage()`; `incr` is `RunUsage.incr`, `total_tokens` the property of both.
Cost (genai-prices) is not ported.
"""

__all__ = [
    "RequestUsage", "RunUsage", "request_usage", "run_usage", "total_tokens",
    "run_total_tokens", "incr", "details_merge",
]


class RequestUsage:
    """Token usage of one model request."""
    input_tokens: int
    cache_write_tokens: int
    cache_read_tokens: int
    output_tokens: int
    input_audio_tokens: int
    cache_audio_read_tokens: int
    output_audio_tokens: int
    details: dict[str, int]


class RunUsage:
    """Token usage and request/tool-call counts of a whole agent run."""
    requests: int
    tool_calls: int
    input_tokens: int
    cache_write_tokens: int
    cache_read_tokens: int
    output_tokens: int
    input_audio_tokens: int
    cache_audio_read_tokens: int
    output_audio_tokens: int
    details: dict[str, int]


def request_usage():
    """`RequestUsage()`: every count zero."""
    return RequestUsage(input_tokens=0, cache_write_tokens=0, cache_read_tokens=0, output_tokens=0, input_audio_tokens=0, cache_audio_read_tokens=0, output_audio_tokens=0, details={})


def run_usage():
    """`RunUsage()`: every count zero."""
    return RunUsage(requests=0, tool_calls=0, input_tokens=0, cache_write_tokens=0, cache_read_tokens=0, output_tokens=0, input_audio_tokens=0, cache_audio_read_tokens=0, output_audio_tokens=0, details={})


def total_tokens(usage) -> int:
    """`RequestUsage.total_tokens`: input plus output tokens."""
    return usage.input_tokens + usage.output_tokens


def run_total_tokens(usage) -> int:
    """`RunUsage.total_tokens`: input plus output tokens."""
    return usage.input_tokens + usage.output_tokens


def details_merge(base, incoming):
    """Adds every count of `incoming` into `base`, key by key."""
    return fold(range(len(mapKeys(incoming))), base, lambda acc, i: details_add(acc, unwrap_or(listGet(mapKeys(incoming), i), ""), incoming))


def details_add(acc, key, incoming):
    return mapSet(acc, key, unwrap_or(mapGet(acc, key), 0) + unwrap_or(mapGet(incoming, key), 0))


def incr(run, request):
    """`RunUsage.incr(request_usage)`: one more request, the counts summed."""
    return RunUsage(
        requests=run.requests + 1,
        tool_calls=run.tool_calls,
        input_tokens=run.input_tokens + request.input_tokens,
        cache_write_tokens=run.cache_write_tokens + request.cache_write_tokens,
        cache_read_tokens=run.cache_read_tokens + request.cache_read_tokens,
        output_tokens=run.output_tokens + request.output_tokens,
        input_audio_tokens=run.input_audio_tokens + request.input_audio_tokens,
        cache_audio_read_tokens=run.cache_audio_read_tokens + request.cache_audio_read_tokens,
        output_audio_tokens=run.output_audio_tokens + request.output_audio_tokens,
        details=details_merge(run.details, request.details),
    )
