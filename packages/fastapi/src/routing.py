# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Routing
"""Ordered immutable routes and path capture. [FASTAPI-HTTP-ROUTING]"""
from typing import Callable
from fastapi import Models as M
from fastapi import Responses as R
from fastapi import Errors as E
from fastapi import Paths
from fastapi import Urls
from fastapi import Results as B
from fastapi import Params

__all__ = ["route", "path_of", "dispatch", "include_router"]


def route(method, path, handler: Callable[[M.Request], M.Response]):
    """Register a typed request handler for a method and route pattern."""
    return M.Route(method=method, path=path, handler=handler)


def path_of(url):
    """The raw URL path without its query string."""
    return Urls.path_of(url)


def include_router(routes, prefix: str, children):
    """Append child routes under a prefix while preserving their declaration order."""
    return listConcat(routes, mapList(children, lambda child: route(child.method, prefix + child.path, child.handler)))


def dispatch(routes, request):
    """Decode the URL, select an ordered route and invoke its typed handler."""
    match Urls.decode(path_of(request.url), False):
        case Success(value=path):
            return select(routes, request, path)
        case Error(message=message):
            return E.http_exception(E.exception(400, message, ""))


def select(routes, request, path):
    exact = filterList(routes, lambda candidate: Paths.matches(candidate.path, path))
    selected = filterList(exact, lambda candidate: candidate.method == request.method)
    match listGet(selected, 0):
        case Success(value=selected_route):
            return invoke(selected_route, request, path)
        case Error():
            return unmatched(routes, exact, path, request)


def invoke(selected, request, path):
    match Paths.bind(selected.path, path):
        case Success(value=params):
            return selected.handler(M.Request(method=request.method, url=request.url, headers=request.headers, body=request.body, path_params=params))
        case Error(message=message):
            return E.http_exception(E.exception(500, message, ""))


def unmatched(routes, exact, path, request):
    match listGet(exact, 0):
        case Success(value=candidate):
            return R.method_not_allowed(candidate.method)
        case Error():
            alternates = filterList(routes, lambda candidate: Paths.matches(candidate.path + "/", path) or Paths.matches(candidate.path, path + "/"))
            match listGet(alternates, 0):
                case Success(value=_candidate):
                    return redirect_request(request)
                case Error():
                    return R.not_found()


def redirect_path(url):
    path = Urls.path_of(url)
    query = drop(url, len(path))
    if endsWith(path, "/"):
        return B.mapped(checkedSub(len(path), 1), lambda size: take(path, size) + query)
    else:
        return Success(value=path + "/" + query)


def redirect_request(request):
    match redirect_path(request.url):
        case Success(value=path):
            match Params.header(request, "Host"):
                case Success(value=host):
                    return R.redirect(f"http://{host}{path}")
                case Error():
                    return R.redirect(path)
        case Error(message=message):
            return E.http_exception(E.exception(400, message, ""))
