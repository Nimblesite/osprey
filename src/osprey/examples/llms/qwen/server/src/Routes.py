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
from qwen_server import Requests as Q
from qwen_server import Wire

__all__ = ["dispatch"]


def dispatch(method, url, body):
    """The reply for one request; the 404 for a path neither protocol serves."""
    path = Wire.path_of(url)
    return ollama(method, path, body) if startsWith(path, "/api/") else absent(path)


def ollama(method, path, body):
    """The Ollama protocol's routes."""
    if path == "/api/version":
        return read_only(method, Ollama.version())
    elif path == "/api/tags":
        return read_only(method, Ollama.tags())
    elif path == "/api/ps":
        return read_only(method, Ollama.ps())
    elif path == "/api/show":
        return posted(method, body, Ollama.show, Q.show_request)
    else:
        return absent(path)


def read_only(method, reply):
    """A GET route's reply, or the 405 for any other method."""
    return reply if method == "GET" else Wire.failed(405, "method not allowed; GET")


def posted(method, body, handler, decode):
    """A POST route's reply, after the body decodes into the route's record."""
    if method == "POST":
        return requested(decode(body), handler)
    else:
        return Wire.failed(405, "method not allowed; POST")


def requested(decoded, handler):
    """Run `handler` over the decoded request, or answer the decode's fault."""
    match decoded:
        case Success(value=request):
            return handler(request)
        case Error(message=message):
            return Wire.failed(400, message)


def absent(path):
    """The 404 for a path this server does not serve."""
    return Wire.failed(404, f"no route for {path}")
