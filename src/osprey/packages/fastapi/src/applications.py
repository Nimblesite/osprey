# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Applications
"""Serve typed Python flavor handlers through Osprey HTTP. [FASTAPI-HTTP-SERVE]"""
from typing import Callable
from fastapi import Models as M
from fastapi import Responses as R
from fastapi import Routing
from fastapi import Results as B
from fastapi import Errors as E

__all__ = ["application", "serve", "dispatch"]


def application(routes, middleware):
    """Create an immutable app; middleware is ordered outermost first."""
    return M.Application(routes=routes, middleware=middleware)


def dispatch(app, request):
    """Wrap routing from the innermost middleware outward, including error paths."""
    terminal = lambda current: Routing.dispatch(app.routes, current)
    handler = foldList(listReverse(app.middleware), terminal, wrap)
    return handler(request)


def wrap(next, middleware: Callable[[M.Request, Callable[[M.Request], M.Response]], M.Response]):
    return lambda request: middleware(request, next)


def serve(port, host, app):
    """Start the native listener; return its handle or the creation/listen/cleanup error."""
    match httpCreateServer(port, host):
        case Error(message=message):
            return Error(message=message)
        case Success(value=server):
            listening = httpListen(server, lambda method, url, headers, body: native_request(app, method, url, headers, body))
            match listening:
                case Success():
                    return Success(value=server)
                case Error(message=message):
                    match httpStopServer(server):
                        case Success():
                            return Error(message=message)
                        case Error(message=cleanup):
                            return Error(message=f"{message}; listener cleanup failed: {cleanup}")


def latin1(text):
    """ASGI/FastAPI decodes each HTTP header byte as Latin-1, including obs-text."""
    pieces = fold(range(len(text)), Success(value=[]), lambda result, index: B.bind(result, lambda previous: B.mapped(B.bind(byteAt(text, index), lambda byte: fromCodePoint(byte)), lambda current: listConcat([current], previous))))
    return B.mapped(pieces, lambda parts: join(listReverse(parts), ""))


def native_request(app, method, url, headers, body):
    match latin1(headers):
        case Success(value=decoded):
            return R.native(dispatch(app, M.Request(method=method, url=url, headers=decoded, body=body, path_params={})))
        case Error(message=message):
            return R.native(E.http_exception(E.exception(500, message, "")))
