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
from fastapi import Params

__all__ = ["application", "include_router", "with_exception_handler", "serve", "with_lifespan", "dispatch", "cors"]


def application(routes, middleware):
    """Create an immutable app; middleware is ordered outermost first."""
    return M.Application(routes=routes, middleware=middleware, exception_handlers={})


def include_router(app, prefix, routes):
    """Mount routes while retaining the app's middleware and exception handlers."""
    return M.Application(routes=Routing.include_router(app.routes, prefix, routes), middleware=app.middleware, exception_handlers=app.exception_handlers)


def with_exception_handler(app, status, handler: Callable[[M.Request, M.HTTPException], M.Response]):
    """Register a handler for typed HTTP exceptions with this status."""
    return M.Application(routes=app.routes, middleware=app.middleware, exception_handlers=mapSet(app.exception_handlers, toString(status), handler))


def resolve_exception(app, request, reply):
    match reply.failure:
        case M.Absent():
            return reply
        case M.Present(value=error):
            match mapGet(app.exception_handlers, toString(error.status_code)):
                case Success(value=handler):
                    return handler(request, error)
                case Error():
                    return reply


def dispatch(app, request):
    """Wrap routing from the innermost middleware outward, including error paths."""
    terminal = lambda current: resolve_exception(app, current, Routing.dispatch(app.routes, current))
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
                    return after_cleanup(Error(message=message), httpStopServer(server), "listener cleanup")


def with_lifespan(port, host, startup, build, run, shutdown):
    """Run an application with startup state and release it after the listener stops. [FASTAPI-HTTP-LIFESPAN]"""
    match startup():
        case Error(message=message):
            return Error(message=message)
        case Success(value=state):
            result = run_listener(port, host, build(state), run)
            return after_cleanup(result, shutdown(state), "lifespan shutdown")


def run_listener(port, host, app, run):
    match serve(port, host, app):
        case Error(message=message):
            return Error(message=message)
        case Success(value=server):
            result = run(server)
            return after_cleanup(result, httpStopServer(server), "listener cleanup")


def after_cleanup(result, cleanup, phase):
    match cleanup:
        case Success():
            return result
        case Error(message=error):
            match result:
                case Success():
                    return Error(message=error)
                case Error(message=message):
                    return Error(message=f"{message}; {phase} failed: {error}")


def latin1(text):
    """ASGI/FastAPI decodes each HTTP header byte as Latin-1, including obs-text."""
    pieces = fold(range(len(text)), Success(value=[]), lambda result, index: B.bind(result, lambda previous: B.mapped(B.bind(byteAt(text, index), lambda byte: fromCodePoint(byte)), lambda current: listConcat([current], previous))))
    return B.mapped(pieces, lambda parts: join(listReverse(parts), ""))


def native_request(app, method, url, headers, body):
    match latin1(headers):
        case Success(value=decoded):
            return R.native(dispatch(app, M.Request(method=method, url=url, headers=decoded, body=body, path_params={})))
        case Error(message=message):
            return R.native(E.exception_response(E.exception(500, message, "")))


# Implements [FASTAPI-HTTP-MIDDLEWARE]. Adapted Starlette policy: see NOTICE.md.
def cors(policy):
    """Build immutable browser-access middleware with an explicit typed policy."""
    return lambda request, next: cors_request(policy, request, next)


def cors_request(policy, request, next: Callable[[M.Request], M.Response]):
    match Params.header(request, "Origin"):
        case Error():
            return next(request)
        case Success(value=origin):
            match Params.header(request, "Access-Control-Request-Method"):
                case Success(value=method):
                    return cors_preflight(policy, request, origin, method) if request.method == "OPTIONS" else cors_simple(policy, origin, next(request))
                case Error():
                    return cors_simple(policy, origin, next(request))


def with_header(reply, name, value):
    retained = filterList(lines(reply.headers), lambda line: line != "" and not startsWith(toLowerCase(line), toLowerCase(name) + ":"))
    return R.response(reply.status_code, reply.content, reply.media_type, join(listConcat(mapList(retained, lambda line: trimEnd(line)), [f"{name}: {value}"]), "\r\n"))


def cors_origin(reply, origin):
    match Params.header(M.Request(method="", url="", headers=reply.headers, body="", path_params={}), "Vary"):
        case Success(value=previous):
            return with_header(with_header(reply, "Vary", previous + ", Origin"), "Access-Control-Allow-Origin", origin)
        case Error():
            return with_header(with_header(reply, "Vary", "Origin"), "Access-Control-Allow-Origin", origin)


def cors_credentials(policy, reply):
    return with_header(reply, "Access-Control-Allow-Credentials", "true") if policy.credentials else reply


def cors_simple(policy, origin, reply):
    exposed = with_header(reply, "Access-Control-Expose-Headers", join(policy.expose, ", ")) if len(policy.expose) > 0 else reply
    wildcard = listContains(policy.origins, "*")
    base = cors_credentials(policy, with_header(exposed, "Access-Control-Allow-Origin", "*") if wildcard else exposed)
    return cors_origin(base, origin) if (wildcard and policy.credentials) or (not wildcard and listContains(policy.origins, origin)) else base


def cors_methods(policy):
    return ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"] if listContains(policy.methods, "*") else policy.methods


def cors_headers(policy):
    names = listConcat(["Accept", "Accept-Language", "Content-Language", "Content-Type"], policy.headers)
    unique = foldList(names, [], lambda previous, name: previous if listContains(previous, name) else listConcat(previous, [name]))
    return listSortBy(unique, lambda left, right: -1 if left < right else (1 if left > right else 0))


def cors_preflight_base(policy, origin):
    explicit = not listContains(policy.origins, "*") or policy.credentials
    base = R.response(200, "OK", "text/plain; charset=utf-8", "")
    varied = with_header(base, "Vary", "Origin") if explicit else with_header(base, "Access-Control-Allow-Origin", "*")
    allowed = listContains(policy.origins, "*") or listContains(policy.origins, origin)
    reflected = with_header(varied, "Access-Control-Allow-Origin", origin) if explicit and allowed else varied
    methods = with_header(reflected, "Access-Control-Allow-Methods", join(cors_methods(policy), ", "))
    timed = with_header(methods, "Access-Control-Max-Age", toString(policy.max_age))
    named = timed if listContains(policy.headers, "*") else with_header(timed, "Access-Control-Allow-Headers", join(cors_headers(policy), ", "))
    return cors_credentials(policy, named)


def cors_requested_headers(policy, request, reply):
    match Params.header(request, "Access-Control-Request-Headers"):
        case Success(value=headers):
            return with_header(reply, "Access-Control-Allow-Headers", headers) if listContains(policy.headers, "*") else reply
        case Error():
            return reply


def cors_header_failures(policy, request):
    match Params.header(request, "Access-Control-Request-Headers"):
        case Error():
            return []
        case Success(value=headers):
            match split(headers, ","):
                case Success(value=names):
                    allowed = mapList(cors_headers(policy), lambda name: toLowerCase(name))
                    invalid = filterList(names, lambda name: not listContains(allowed, toLowerCase(trim(name))))
                    return ["headers"] if not listContains(policy.headers, "*") and len(invalid) > 0 else []
                case Error():
                    return ["headers"]


def cors_preflight(policy, request, origin, method):
    origins = [] if listContains(policy.origins, "*") or listContains(policy.origins, origin) else ["origin"]
    methods = [] if listContains(cors_methods(policy), method) else ["method"]
    network = [] if len(Params.header_all(request, "Access-Control-Request-Private-Network")) == 0 else ["private-network"]
    failures = listConcat(listConcat(origins, methods), listConcat(cors_header_failures(policy, request), network))
    reply = cors_requested_headers(policy, request, cors_preflight_base(policy, origin))
    return R.response(400, "Disallowed CORS " + join(failures, ", "), reply.media_type, reply.headers) if len(failures) > 0 else reply
