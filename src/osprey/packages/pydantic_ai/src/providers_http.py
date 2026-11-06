# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=ProvidersHttp
"""The providers that reach a real API over the runtime's HTTPS client.

Port of pydantic_ai/providers/openai.py, providers/ollama.py, providers/deepseek.py,
providers/moonshotai.py and providers/zai.py (2.46.0): `OpenAIProvider`, `OllamaProvider`,
`DeepSeekProvider`, `MoonshotAIProvider` and `ZaiProvider`. Separate from `Providers`, because
a target without runtime HTTP, such as a native application whose host performs requests,
cannot link it.
"""
from pydantic_ai import Providers as P

__all__ = ["OpenAIProvider", "OllamaProvider", "DeepSeekProvider", "MoonshotAIProvider", "ZaiProvider", "provider", "post_json", "reply_of", "after_close"]

REQUEST_TIMEOUT_MS = 600000
"""The 600 s timeout `openai.AsyncOpenAI` defaults to."""


def post_json(base_url, path, body, headers) -> Result[P.HttpReply, str]:
    """One POST through the runtime HTTP client; `Error` names a transport failure."""
    match httpCreateClient(base_url, REQUEST_TIMEOUT_MS):
        case Success(value=client):
            return after_close(post_with(client, path, body, headers), httpCloseClient(client))
        case Error(message=message):
            return Error(message=f"httpCreateClient failed: {message}")


def post_with(client, path, body, headers) -> Result[P.HttpReply, str]:
    match httpPostResponse(client, path, body, headers):
        case Success(value=handle):
            status = httpResponseStatus(handle)
            text = httpResponseBody(handle)
            return reply_of(status, text, httpResponseFree(handle))
        case Error(message=message):
            return Error(message=message)


def reply_of(status, text, freed) -> Result[P.HttpReply, str]:
    """The reply once the body is read and the response freed; a failed read or free is an `Error`, as httpx raises."""
    match text:
        case Success(value=body):
            return released(Success(value=P.HttpReply(status=status, body=body)), "httpResponseFree", freed)
        case Error(message=message):
            return Error(message=f"httpResponseBody failed: {message}")


def after_close(result, closed) -> Result[P.HttpReply, str]:
    """The request's result once the client is closed; a failed close fails a good request."""
    match result:
        case Success(value=_reply):
            return released(result, "httpCloseClient", closed)
        case Error(message=message):
            return Error(message=message)


def released(result, call, code) -> Result[P.HttpReply, str]:
    """`result` when the runtime `call` that releases it returned 0, otherwise the release failure."""
    match code:
        case Success(value=value):
            return result if value == 0 else Error(message=f"{call} returned {value}")
        case Error(message=message):
            return Error(message=f"{call} failed: {message}")


def provider(name, base_url, api_key):
    """A provider whose `post` sends `Authorization: Bearer <api_key>` and JSON to `base_url`."""
    return P.Provider(name=name, base_url=base_url, api_key=api_key, post=lambda path, body, headers: post_json(base_url, path, body, "Content-Type: application/json\r\nAuthorization: Bearer " + api_key + headers))


def OpenAIProvider(base_url, api_key):
    """`OpenAIProvider(base_url=, api_key=)`; an empty key becomes `api-key-not-set` as CPython does when a base URL is given."""
    return provider("openai", "https://api.openai.com/v1" if base_url == "" else base_url, "api-key-not-set" if api_key == "" else api_key)


def OllamaProvider(base_url):
    """`OllamaProvider(base_url=)`: Ollama's OpenAI-compatible endpoint, no key."""
    return provider("ollama", base_url, "ollama")


def DeepSeekProvider(api_key):
    """`DeepSeekProvider(api_key=)`: https://api.deepseek.com."""
    return provider("deepseek", "https://api.deepseek.com", api_key)


def MoonshotAIProvider(api_key):
    """`MoonshotAIProvider(api_key=)`: MoonshotAI's OpenAI-compatible endpoint."""
    return provider("moonshotai", "https://api.moonshot.ai/v1", api_key)


def ZaiProvider(api_key):
    """`ZaiProvider(api_key=)`: Z.AI's OpenAI-compatible endpoint."""
    return provider("zai", "https://api.z.ai/api/paas/v4", api_key)
