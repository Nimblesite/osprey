# osprey: flavor=python
# osprey: namespace=fastapi_tests
# osprey: module=Fixture
"""Real HTTP service driven by e2e.py. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Responses as R
from fastapi import Routing
from fastapi import Params as P
from fastapi import Validation as V
from fastapi import Uuids
from fastapi import Models as M
from fastapi import Errors as E


def health(_request):
    return R.json_response(200, "{\"status\":\"ok\"}", "")


def ready(_request):
    return R.json_response(503, "{\"detail\":\"database unavailable\"}", "Cache-Control: no-store")


def echo(request):
    return R.response(201, request.body, "application/octet-stream", "X-Echo: yes")


def routes():
    """The ordered real HTTP routes used by both managed-backend suites."""
    return [Routing.route("GET", "/health", health), Routing.route("GET", "/readyz", ready), Routing.route("POST", "/echo", echo), Routing.route("GET", "/configs/{config_id}", config), Routing.route("GET", "/bundle/{path:path}", bundle), Routing.route("GET", "/ordered/me", me), Routing.route("GET", "/ordered/{value}", ordered), Routing.route("GET", "/usage", usage), Routing.route("GET", "/metrics", metrics)]



__all__ = ["routes"]


def config(request):
    identity = Uuids.uuid(["path", "config_id"], P.path(request, "config_id"))
    limit = V.integer(["query", "limit"], V.defaulted(P.query(request, "limit"), "20"), 1, 100)
    period = V.choice(["query", "period"], V.defaulted(P.query(request, "period"), "day"), ["day", "hour"])
    return config_response(request, identity, limit, period)


def usage(request):
    identity = Uuids.uuid(["query", "agent_config_id"], P.query(request, "agent_config_id"))
    limit = V.integer(["query", "limit"], P.query(request, "limit"), 1, 100)
    period = V.choice(["query", "period"], P.query(request, "period"), ["day", "hour"])
    return config_response(request, identity, limit, period)


def config_response(request, identity, limit, period):
    key = V.required(["header", "X-API-Key"], P.header(request, "X-API-Key"))
    match V.combine(V.combine(identity, limit), V.combine(period, key)):
        case M.Valid(value=bound):
            fields = [JsonMember("config_id", JsonString(bound.first.first)), JsonMember("limit", JsonInt(bound.first.second)), JsonMember("period", JsonString(bound.second.first)), JsonMember("key", JsonString(bound.second.second))]
            return E.json_response(200, JsonObject(fields), "")
        case M.Invalid(issues=errors):
            return E.validation(errors)


def bundle(request):
    match P.path(request, "path"):
        case Success(value=path):
            match P.query_all(request, "tags"):
                case Success(value=tags):
                    return E.json_response(200, JsonObject([JsonMember("path", JsonString(path)), JsonMember("tags", JsonArray(mapList(tags, lambda tag: JsonString(tag))))]), "")
                case Error(message=message):
                    return E.http_exception(E.exception(400, message, ""))
        case Error(message=message):
            return E.http_exception(E.exception(400, message, ""))


def me(_request):
    return E.json_response(200, JsonObject([JsonMember("value", JsonString("me"))]), "")


def ordered(request):
    match P.path(request, "value"):
        case Success(value=text):
            return E.json_response(200, JsonObject([JsonMember("value", JsonString(text))]), "")
        case Error(message=message):
            return E.http_exception(E.exception(400, message, ""))


def metrics(_request):
    return E.json_response(200, JsonObject([JsonMember("latency", JsonFloat(exp(1000.0)))]), "")
