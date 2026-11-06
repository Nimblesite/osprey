# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Dependencies
"""Typed request-local dependency graphs and function-scope cleanup. [FASTAPI-HTTP-DEPENDENCIES]"""
from typing import Callable
from fastapi import Models as M
from fastapi import Errors as E

__all__ = ["need", "provider", "override", "execute", "pure_close"]


def need(name, use_cache):
    """Declare an injection edge with an explicit cache policy."""
    return M.Need(name=name, use_cache=use_cache)


def provider[T](name, inputs, provide: Callable[[M.Request, list[T]], M.DependencyResult[T]], close: Callable[[T], M.DependencyResult[None]]):
    """Register a typed provider and its explicit function-scope cleanup."""
    return M.Dependency(name=name, inputs=inputs, provide=provide, close=close)


def pure_close(_value):
    """A pure provider has no resource to release; cleanup succeeds with unit."""
    return M.Provided(value=None)


def override(graph, name, replacement):
    """Replace the provider and its children while retaining its original cache key."""
    return mapList(graph, lambda item: M.Dependency(name=item.name, inputs=replacement.inputs, provide=replacement.provide, close=replacement.close) if item.name == name else item)


def rejected(message, state):
    return DependencyStep(outcome=M.Rejected(error=E.exception(500, message, "")), state=state)


def resolve_one[T](injection, graph, request, state, visiting) -> DependencyStep[T, DependencyState[T]]:
    if listContains(visiting, injection.name):
        return rejected(f"Dependency cycle: {injection.name}", state)
    else:
        candidates = filterList(graph, lambda item: item.name == injection.name)
        match candidates:
            case [selected]:
                children = resolve_many(selected.inputs, graph, request, state, listConcat(visiting, [injection.name]))
                return resolve_children(injection, selected, request, children)
            case _:
                return rejected(f"Expected exactly one dependency provider: {injection.name}", state)


def resolve_children(injection, selected, request, children):
    """Resolve children even on a parent cache hit, as the reference resolver does."""
    match children.outcome:
        case M.Provided(value=values):
            match mapGet(children.state.cache, injection.name):
                case Success(value=cached):
                    return DependencyStep(outcome=M.Provided(value=cached), state=children.state) if injection.use_cache else acquire(selected, request, values, children.state)
                case Error():
                    return acquire(selected, request, values, children.state)
        case M.Rejected(error=error):
            return DependencyStep(outcome=M.Rejected(error=error), state=children.state)
        case M.DependencyInvalid(issues=issues):
            return DependencyStep(outcome=M.DependencyInvalid(issues=issues), state=children.state)


def first_cached(cache, name, item):
    match mapGet(cache, name):
        case Success():
            return cache
        case Error():
            return mapSet(cache, name, item)


def acquire(selected, request, values, state):
    outcome = selected.provide(request, values)
    match outcome:
        case M.Provided(value=item):
            cache = first_cached(state.cache, selected.name, item)
            acquired = listConcat([Acquired(name=selected.name, value=item)], state.acquired)
            return DependencyStep(outcome=outcome, state=DependencyState(cache=cache, acquired=acquired))
        case _:
            return DependencyStep(outcome=outcome, state=state)


def resolve_many[T](injections, graph, request, state, visiting) -> DependencyStep[list[T], DependencyState[T]]:
    match injections:
        case []:
            return DependencyStep(outcome=M.Provided(value=[]), state=state)
        case [head, *tail]:
            first = resolve_one(head, graph, request, state, visiting)
            match first.outcome:
                case M.Rejected(error=error):
                    return DependencyStep(outcome=M.Rejected(error=error), state=first.state)
                case M.Provided(value=item):
                    return resolve_tail(M.Valid(value=item), tail, graph, request, first.state, visiting)
                case M.DependencyInvalid(issues=issues):
                    return resolve_tail(M.Invalid(issues=issues), tail, graph, request, first.state, visiting)


def resolve_tail(first, tail, graph, request, state, visiting):
    rest = resolve_many(tail, graph, request, state, visiting)
    return DependencyStep(outcome=combine(first, rest.outcome), state=rest.state)


def combine(first, rest):
    match rest:
        case M.Rejected(error=error):
            return M.Rejected(error=error)
        case M.DependencyInvalid(issues=issues):
            match first:
                case M.Invalid(issues=earlier):
                    return M.DependencyInvalid(issues=listConcat(earlier, issues))
                case _:
                    return M.DependencyInvalid(issues=issues)
        case M.Provided(value=tail):
            match first:
                case M.Valid(value=head):
                    return M.Provided(value=listConcat([head], tail))
                case M.Invalid(issues=issues):
                    return M.DependencyInvalid(issues=issues)


def rendered(outcome, request, endpoint):
    match outcome:
        case M.Provided(value=values):
            return endpoint(request, values)
        case M.Rejected(error=error):
            return E.exception_response(error)
        case M.DependencyInvalid(issues=issues):
            return E.validation(issues)


def close_one(graph, acquired):
    match filterList(graph, lambda item: item.name == acquired.name):
        case [selected]:
            return selected.close(acquired.value)
        case _:
            return M.Rejected(error=E.exception(500, f"Expected cleanup provider: {acquired.name}", ""))


def cleanup(graph, acquired, response):
    """Unwind every acquisition in reverse order, including after a cleanup failure."""
    match acquired:
        case []:
            return response
        case [head, *tail]:
            match close_one(graph, head):
                case M.Provided():
                    return cleanup(graph, tail, response)
                case M.Rejected(error=error):
                    return cleanup(graph, tail, E.exception_response(error))
                case M.DependencyInvalid(issues=issues):
                    return cleanup(graph, tail, E.validation(issues))


def execute[T](graph, injections, request, endpoint: Callable[[M.Request, list[T]], M.Response]):
    """Resolve one request, invoke its endpoint and always unwind acquired resources."""
    solved = resolve_many(injections, graph, request, DependencyState(cache={}, acquired=[]), [])
    return cleanup(graph, solved.state.acquired, rendered(solved.outcome, request, endpoint))

# Generated private models from models.td


class Acquired[T]:
    """One successfully acquired value and its immutable provider key."""
    name: str
    value: T


class DependencyState[T]:
    """Request-local cache and reverse-acquisition cleanup stack."""
    cache: dict[str, T]
    acquired: list[Acquired[T]]


class DependencyStep[T, S]:
    """A dependency outcome and the resulting request-local state."""
    outcome: M.DependencyResult[T]
    state: S
