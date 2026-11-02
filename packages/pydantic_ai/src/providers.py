# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Providers
"""pydantic_ai.providers: where a model's HTTP API lives and how it is called.

Port of pydantic_ai/providers/openai.py, providers/ollama.py and
providers/deepseek.py (2.46.0). CPython wraps an `AsyncOpenAI` client; here
a `Provider` carries the base URL, the API key and a `post` function that
performs one HTTP POST through the runtime's HTTPS client and answers the
status and body. `scripted` is the test double: its `post` answers from a
list of canned replies, chosen by how many tool results the request carries,
so a multi-step tool conversation can be scripted without a network.
"""
from typing import Callable

__all__ = ["HttpReply", "Provider", "OpenAIProvider", "OllamaProvider", "DeepSeekProvider", "provider", "scripted", "http_post", "reply"]

REQUEST_TIMEOUT_MS = 600000
"""The 600 s timeout `openai.AsyncOpenAI` defaults to."""


class HttpReply:
    """One HTTP response: its status code and body text."""
    status: int
    body: str


class Provider:
    """A model API: `post(path, body, headers)` performs a JSON POST against `base_url`."""
    name: str
    base_url: str
    api_key: str
    post: Callable[[str, str, str], Result[HttpReply, str]]


def reply(status, body):
    return HttpReply(status=status, body=body)


def http_post(base_url, path, body, headers) -> Result[HttpReply, str]:
    """One POST through the runtime HTTP client; `Error` names a transport failure."""
    client = httpCreateClient(base_url, REQUEST_TIMEOUT_MS)
    result = post_with(client, path, body, headers)
    closed = httpCloseClient(client)
    return result if closed == 0 else result


def post_with(client, path, body, headers) -> Result[HttpReply, str]:
    if client <= 0:
        return Error(message=f"httpCreateClient failed: {client}")
    else:
        match httpPostResponse(client, path, body, headers):
            case Success(value=handle):
                status = httpResponseStatus(handle)
                text = unwrap_or(httpResponseBody(handle), "")
                freed = unwrap_or(httpResponseFree(handle), 0)
                return Success(value=HttpReply(status=status, body=text)) if freed == 0 else Success(value=HttpReply(status=status, body=text))
            case Error(message=message):
                return Error(message=message)


def provider(name, base_url, api_key):
    """A provider whose `post` sends `Authorization: Bearer <api_key>` and JSON to `base_url`."""
    return Provider(name=name, base_url=base_url, api_key=api_key, post=lambda path, body, headers: http_post(base_url, path, body, "Content-Type: application/json\r\nAuthorization: Bearer " + api_key + headers))


def OpenAIProvider(base_url, api_key):
    """`OpenAIProvider(base_url=, api_key=)`; an empty key becomes `api-key-not-set` as CPython does when a base URL is given."""
    return provider("openai", "https://api.openai.com/v1" if base_url == "" else base_url, "api-key-not-set" if api_key == "" else api_key)


def OllamaProvider(base_url):
    """`OllamaProvider(base_url=)`: Ollama's OpenAI-compatible endpoint, no key."""
    return provider("ollama", base_url, "ollama")


def DeepSeekProvider(api_key):
    """`DeepSeekProvider(api_key=)`: https://api.deepseek.com."""
    return provider("deepseek", "https://api.deepseek.com", api_key)


def tool_message_count(body) -> int:
    """How many `"role":"tool"` messages a request body carries."""
    return len(unwrap_or(split(body, "\"role\":\"tool\""), [body])) - 1


def scripted(replies):
    """A provider answering `replies[n]` to the request carrying `n` tool results (the last reply repeats)."""
    return Provider(name="scripted", base_url="scripted://", api_key="", post=lambda path, body, headers: scripted_post(replies, body))


def scripted_post(replies, body) -> Result[HttpReply, str]:
    n = tool_message_count(body)
    index = n if n < len(replies) else len(replies) - 1
    match listGet(replies, index):
        case Success(value=r):
            return Success(value=r)
        case Error(message=_m):
            return Error(message="scripted provider has no replies")
