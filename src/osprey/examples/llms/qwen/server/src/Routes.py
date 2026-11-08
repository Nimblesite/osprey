# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Routes
"""Method and path routing for both protocols ([LLM-API-OLLAMA], [LLM-API-OPENAI]).

A route exists under exactly one method, as in Ollama and OpenAI: the other
method is a 405, a path the server does not serve is a 404, and a body that is
not JSON or lacks a model name is a 400. No route answers a request by
truncating it.
"""

from qwen_server import Ollama
from qwen_server import OpenAi
from qwen_server import Requests as Q
from qwen_server import Wire

__all__ = ["dispatch"]


def dispatch(method, url, body, model):
    """The reply for one request under the installed `model`; the 404 for a path neither protocol serves."""
    path = Wire.path_of(url)
    if path == "/":
        return heartbeat(method)
    elif startsWith(path, "/api/"):
        return ollama(method, path, body, model)
    elif startsWith(path, "/v1/"):
        return openai(method, path, body, model)
    else:
        return absent(path)


def openai(method, path, body, model):
    """The OpenAI protocol's routes."""
    if path == "/v1/models":
        return read_only(method, OpenAi.models())
    elif path == "/v1/chat/completions":
        return posted(method, body, lambda request: OpenAi.completions(model, request), Q.openai_chat_request, OpenAi.failed)
    else:
        return OpenAi.failed(404, f"no route for {path}")


def ollama(method, path, body, model):
    """The Ollama protocol's routes."""
    if path == "/api/version":
        return read_only(method, Ollama.version())
    elif path == "/api/tags":
        return read_only(method, Ollama.tags())
    elif path == "/api/ps":
        return read_only(method, Ollama.ps())
    elif path == "/api/show":
        return posted(method, body, Ollama.show, Q.show_request, Wire.failed)
    elif path == "/api/chat":
        return posted(method, body, lambda request: Ollama.chat(model, request), Q.chat_request, Wire.failed)
    elif path == "/api/generate":
        return posted(method, body, lambda request: Ollama.generate(model, request), Q.generate_request, Wire.failed)
    else:
        return absent(path)


def heartbeat(method):
    """`/`: the liveness probe the `ollama` command sends (as HEAD) before every command."""
    return Wire.text_reply(200, "Ollama is running") if method == "GET" or method == "HEAD" else Wire.failed(405, "method not allowed; GET")


def read_only(method, reply):
    """A GET route's reply, or the 405 for any other method."""
    return reply if method == "GET" else Wire.failed(405, "method not allowed; GET")


def posted(method, body, handler, decode, fail):
    """A POST route's reply, after the body decodes into the route's record."""
    if method == "POST":
        return requested(decode(body), handler, fail)
    else:
        return fail(405, "method not allowed; POST")


def requested(decoded, handler, fail):
    """Run `handler` over the decoded request, or answer the decode's fault."""
    match decoded:
        case Success(value=request):
            return handler(request)
        case Error(message=message):
            return fail(400, message)


def absent(path):
    """The 404 for a path this server does not serve."""
    return Wire.failed(404, f"no route for {path}")
