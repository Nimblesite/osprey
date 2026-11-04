# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Routing
"""Ordered immutable routes over the native HTTP boundary. [FASTAPI-HTTP-ROUTING]"""
from typing import Callable
from fastapi import Models as M
from fastapi import Responses as R

__all__ = ["route", "path_of", "dispatch", "include_router"]


def route(method, path, handler: Callable[[M.Request], M.Response]):
    return M.Route(method=method, path=path, handler=handler)


def path_of(url):
    match indexOf(url, "?"):
        case Success(value=index):
            return take(url, index)
        case Error():
            return url


def include_router(routes, prefix: str, children):
    return listConcat(routes, mapList(children, lambda child: route(child.method, prefix + child.path, child.handler)))


def dispatch(routes, request):
    path = path_of(request.url)
    exact = filterList(routes, lambda candidate: candidate.path == path)
    selected = filterList(exact, lambda candidate: candidate.method == request.method)
    match listGet(selected, 0):
        case Success(value=selected_route):
            return selected_route.handler(request)
        case Error():
            return unmatched(routes, exact, path)


def unmatched(routes, exact, path):
    match listGet(exact, 0):
        case Success(value=candidate):
            return R.method_not_allowed(candidate.method)
        case Error():
            alternates = filterList(routes, lambda candidate: candidate.path + "/" == path or candidate.path == path + "/")
            match listGet(alternates, 0):
                case Success(value=candidate):
                    return R.redirect(candidate.path)
                case Error():
                    return R.not_found()
