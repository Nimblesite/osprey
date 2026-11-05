# osprey: flavor=python
# osprey: namespace=fastapi_tests
# osprey: module=Fixture
"""Real HTTP service driven by e2e.cpython. Implements [FASTAPI-HTTP-ROUTING]."""
from typing import Callable
from fastapi import Responses as R
from fastapi import Routing
from fastapi import Params as P
from fastapi import Validation as V
from fastapi import Uuids
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Dependencies as D
from fastapi import Applications as A
from fastapi import Results as B
from fastapi import Body

def health(_request):
    return R.json_response(200, "{\"status\":\"ok\"}", "")

def varied(_request):
    return R.json_response(200, '{"status":"ok"}', "Vary: Accept-Encoding")

def ready(_request):
    return R.json_response(503, "{\"detail\":\"database unavailable\"}", "Cache-Control: no-store")

def echo(request):
    return R.response(201, request.body, "application/octet-stream", "X-Echo: yes")

def routes():
    """The ordered real HTTP routes used by both managed-backend suites."""
    return listConcat(body_routes(), [Routing.route("GET", "/health", health), Routing.route("GET", "/vary", varied), Routing.route("GET", "/readyz", ready), Routing.route("POST", "/echo", echo), Routing.route("GET", "/configs/{config_id}", config), Routing.route("GET", "/bundle/{path:path}", bundle), Routing.route("GET", "/ordered/me", me), Routing.route("GET", "/ordered/{value}", ordered), Routing.route("GET", "/usage", usage), Routing.route("GET", "/metrics", metrics)])


__all__ = ["routes", "application", "cors_application"]

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
                    return E.exception_response(E.exception(400, message, ""))
        case Error(message=message):
            return E.exception_response(E.exception(400, message, ""))

def me(_request):
    return E.json_response(200, JsonObject([JsonMember("value", JsonString("me"))]), "")

def ordered(request):
    match P.path(request, "value"):
        case Success(value=text):
            return E.json_response(200, JsonObject([JsonMember("value", JsonString(text))]), "")
        case Error(message=message):
            return E.exception_response(E.exception(400, message, ""))

def metrics(_request):
    return E.json_response(200, JsonObject([JsonMember("latency", JsonFloat(exp(1000.0)))]), "")

def trace_path(port):
    return f"target/fastapi-dependency-{port}.trace"

def application(port):
    """Build the real HTTP acceptance app with request-local trace capture."""
    path = trace_path(port)
    return A.application(listConcat(listConcat(routes(), dependency_routes(path)), extra_routes(path, providers(path))), [A.cors(cors_policy("cors-nap")), lambda request, next: reset_trace(path, request, next)])

def reset_trace(path, request, next: Callable[[M.Request], M.Response]):
    if Routing.path_of(request.url) == "/dependency-trace":
        return next(request)
    else:
        match writeFile(path, ""):
            case Success():
                return next(request)
            case Error(message=message):
                return E.exception_response(E.exception(500, message, ""))

def logged(path, message, result):
    match readFile(path):
        case Success(value=previous):
            match writeFile(path, previous + message + "\n"):
                case Success():
                    return M.Provided(value=result)
                case Error(message=error):
                    return M.Rejected(error=E.exception(500, error, ""))
        case Error(message=error):
            return M.Rejected(error=E.exception(500, error, ""))

def trace_lines(path):
    return B.bind(readFile(path), lambda text: split(text, "\n"))

def open_session(path, _request, _children):
    match trace_lines(path):
        case Success(value=previous):
            identity = satAdd(len(filterList(previous, lambda line: startsWith(line, "open:"))), 1)
            return logged(path, f"open:{identity}", SessionValue(id=identity))
        case Error(message=error):
            return M.Rejected(error=E.exception(500, error, ""))

def close_session(path, resource):
    match resource:
        case SessionValue(id=identity):
            return logged(path, f"close:{identity}", None)
        case _:
            return M.Rejected(error=E.exception(500, "Expected session resource", ""))

def identity_provider(path, request, children):
    match listGet(children, 0):
        case Success(value=resource):
            match resource:
                case SessionValue(id=identity):
                    return authenticate(path, request, identity)
                case _:
                    return M.Rejected(error=E.exception(500, "Expected session dependency", ""))
        case Error(message=error):
            return M.Rejected(error=E.exception(500, error, ""))

def authenticate(path, request, identity):
    match P.header(request, "Authorization"):
        case Success(value=authorization):
            if startsWith(authorization, "Bearer ") and trim(drop(authorization, 7)) != "":
                match V.defaulted(P.header(request, "X-Role"), "member"):
                    case Success(value=role):
                        return logged(path, f"auth:{identity}", IdentityValue(id=identity, role=role))
                    case Error(message=error):
                        return M.Rejected(error=E.exception(500, error, ""))
            else:
                return M.Rejected(error=E.exception(401, "not_authenticated", "WWW-Authenticate: Bearer"))
        case Error():
            return M.Rejected(error=E.exception(401, "not_authenticated", "WWW-Authenticate: Bearer"))

def credit_provider(path, request, children):
    parsed = V.integer(["header", "x-credit"], V.defaulted(P.header(request, "X-Credit"), "1"), -9223372036854775807, 9223372036854775807)
    match parsed:
        case M.Invalid(issues=issues):
            return M.DependencyInvalid(issues=issues)
        case M.Valid(value=credit):
            match listGet(children, 0):
                case Success(value=person):
                    return authorize_credit(path, person, credit)
                case Error(message=error):
                    return M.Rejected(error=E.exception(500, error, ""))

def authorize_credit(path, person, credit):
    match person:
        case IdentityValue(id=identity, role=role):
            if role != "writer":
                return M.Rejected(error=E.exception(403, "writer_required", ""))
            elif credit <= 0:
                return M.Rejected(error=E.exception(402, "insufficient_credit", ""))
            else:
                return logged(path, f"credit:{identity}", CreditValue(id=identity, role=role, credit=credit))
        case _:
            return M.Rejected(error=E.exception(500, "Expected identity dependency", ""))

def fake_identity(_request, _children):
    return M.Provided(value=IdentityValue(id=0, role="writer"))

def providers(path):
    return [D.provider("session", [], lambda request, children: open_session(path, request, children), lambda resource: close_session(path, resource)), D.provider("identity", [D.need("session", True)], lambda request, children: identity_provider(path, request, children), D.pure_close), D.provider("credit", [D.need("identity", True)], lambda request, children: credit_provider(path, request, children), D.pure_close)]

def dependency_routes(path):
    graph = providers(path)
    changed = D.override(graph, "identity", D.provider("fake", [], fake_identity, D.pure_close))
    shared = [D.need("session", True), D.need("identity", True), D.need("credit", True)]
    fresh = [D.need("session", False), D.need("session", False)]
    return [Routing.route("GET", "/deps", lambda request: D.execute(graph, shared, request, dependency_response)), Routing.route("GET", "/deps/uncached", lambda request: D.execute(graph, fresh, request, sessions_response)), Routing.route("GET", "/deps/override", lambda request: D.execute(changed, shared, request, dependency_response)), Routing.route("GET", "/dependency-trace", lambda request: trace_response(path, request))]

def dependency_response(_request, values):
    match values:
        case [first, second, third]:
            match first:
                case SessionValue(id=identity):
                    return identity_response(identity, second, third)
                case _:
                    return E.exception_response(E.exception(500, "Expected session value", ""))
        case _:
            return E.exception_response(E.exception(500, "Expected three resolved dependencies", ""))

def identity_response(first, person, spend):
    match person:
        case IdentityValue(id=identity, role=role):
            return credit_response(first, identity, role, spend)
        case _:
            return E.exception_response(E.exception(500, "Expected identity value", ""))

def credit_response(first, second, role, spend):
    match spend:
        case CreditValue(id=identity, credit=credit):
            return E.json_response(200, JsonObject([JsonMember("session_ids", JsonArray([JsonInt(first), JsonInt(second), JsonInt(identity)])), JsonMember("role", JsonString(role)), JsonMember("credit", JsonInt(credit))]), "")
        case _:
            return E.exception_response(E.exception(500, "Expected credit value", ""))

def sessions_response(_request, values):
    match values:
        case [first, second]:
            match first:
                case SessionValue(id=identity):
                    return second_session_response(identity, second)
                case _:
                    return E.exception_response(E.exception(500, "Expected first session value", ""))
        case _:
            return E.exception_response(E.exception(500, "Expected two session dependencies", ""))

def second_session_response(first, second):
    match second:
        case SessionValue(id=identity):
            return E.json_response(200, JsonObject([JsonMember("session_ids", JsonArray([JsonInt(first), JsonInt(identity)]))]), "")
        case _:
            return E.exception_response(E.exception(500, "Expected second session value", ""))

def trace_response(path, _request):
    match trace_lines(path):
        case Success(value=trace):
            lines = filterList(trace, lambda line: line != "")
            return E.json_response(200, JsonObject([JsonMember("trace", JsonArray(mapList(lines, lambda line: JsonString(line))))]), "")
        case Error(message=error):
            return E.exception_response(E.exception(500, error, ""))

def passthrough(_request, children):
    match listGet(children, 0):
        case Success(value=child):
            return M.Provided(value=child)
        case Error(message=error):
            return M.Rejected(error=E.exception(500, error, ""))

def credit_single_response(_request, values):
    match values:
        case [credit]:
            return credit_single_value(credit)
        case _:
            return E.exception_response(E.exception(500, "Expected credit dependency", ""))

def credit_single_value(credit_value):
    match credit_value:
        case CreditValue(id=identity, role=role):
            return credit_response(identity, identity, role, credit_value)
        case _:
            return E.exception_response(E.exception(500, "Expected credit value", ""))

def number_provider(path, request, children, header, label):
    parsed = V.integer(["header", toLowerCase(header)], P.header(request, header), -9223372036854775807, 9223372036854775807)
    match parsed:
        case M.Valid(value=number):
            return number_session(path, children, number, label)
        case M.Invalid(issues=issues):
            return M.DependencyInvalid(issues=issues)

def number_session(path, children, number, label):
    match children:
        case [SessionValue(id=identity)]:
            return logged(path, f"{label}:{identity}", NumberValue(number=number))
        case _:
            return M.Rejected(error=E.exception(500, "Expected number session", ""))

def sum_response(_request, values):
    match values:
        case [first, second]:
            match first:
                case NumberValue(number=number):
                    return sum_second(number, second)
                case _:
                    return E.exception_response(E.exception(500, "Expected first number", ""))
        case _:
            return E.exception_response(E.exception(500, "Expected two numbers", ""))

def sum_second(first, second):
    match second:
        case NumberValue(number=number):
            match checkedAdd(first, number):
                case Success(value=total):
                    return E.json_response(200, JsonObject([JsonMember("sum", JsonInt(total))]), "")
                case Error(message=error):
                    return E.exception_response(E.exception(500, error, ""))
        case _:
            return E.exception_response(E.exception(500, "Expected second number", ""))

def failing_close(path, resource, validation):
    closed = close_session(path, resource)
    match closed:
        case M.Provided():
            return M.DependencyInvalid(issues=[E.issue(["dependency", "cleanup"], "value_error", "cleanup_invalid", "bad", False, M.NoContext())]) if validation else M.Rejected(error=E.exception(409, "cleanup_failed", ""))
        case _:
            return closed

def extra_routes(path, graph):
    nested = listConcat(graph, [D.provider("relay", [D.need("credit", True)], passthrough, D.pure_close)])
    numbers = listConcat(graph, [D.provider("left", [D.need("session", True)], lambda request, children: number_provider(path, request, children, "X-Left", "left"), D.pure_close), D.provider("right", [D.need("session", True)], lambda request, children: number_provider(path, request, children, "X-Right", "right"), D.pure_close)])
    failures = [Routing.route("GET", "/deps/cleanup-http", lambda request: cleanup_failure(path, graph, request, False)), Routing.route("GET", "/deps/cleanup-validation", lambda request: cleanup_failure(path, graph, request, True))]
    return listConcat([Routing.route("GET", "/deps/validation-query", query_numbers), Routing.route("GET", "/deps/nested", lambda request: D.execute(nested, [D.need("relay", True)], request, credit_single_response)), Routing.route("GET", "/deps/validation", lambda request: D.execute(numbers, [D.need("left", True), D.need("right", True)], request, sum_response))], listConcat(failures, configuration_routes(graph)))

def cleanup_failure(path, graph, request, validation):
    added = D.provider("second", [], lambda current, children: open_session(path, current, children), lambda resource: failing_close(path, resource, validation))
    return D.execute(listConcat(graph, [added]), [D.need("session", True), D.need("second", True)], request, sessions_response)

def configuration_routes(graph):
    cyclic = listConcat(graph, [D.provider("cycle", [D.need("cycle", True)], passthrough, D.pure_close)])
    duplicate = listConcat(graph, graph)
    return [Routing.route("GET", "/deps/cycle", lambda request: D.execute(cyclic, [D.need("session", True), D.need("cycle", True)], request, sessions_response)), Routing.route("GET", "/deps/missing", lambda request: D.execute(graph, [D.need("session", True), D.need("missing", True)], request, sessions_response)), Routing.route("GET", "/deps/duplicate", lambda request: D.execute(duplicate, [D.need("session", True)], request, sessions_response))]

def query_numbers(request):
    first = V.integer(["query", "first"], P.query(request, "first"), bitNot(9223372036854775807), 9223372036854775807)
    second = V.integer(["query", "second"], P.query(request, "second"), bitNot(9223372036854775807), 9223372036854775807)
    match V.combine(first, second):
        case M.Valid(value=pair):
            return sum_second(pair.first, NumberValue(number=pair.second))
        case M.Invalid(issues=issues):
            return E.validation(issues)

def cors_policy(mode):
    if mode == "cors-restricted":
        return M.CorsPolicy(origins=["https://app.example"], methods=["GET", "POST"], headers=["X-Token"], expose=["ETag"], credentials=True, max_age=60)
    elif mode == "cors-all":
        return M.CorsPolicy(origins=["*"], methods=["*"], headers=["*"], expose=[], credentials=True, max_age=600)
    else:
        return M.CorsPolicy(origins=["*"], methods=["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"], headers=["Authorization", "Content-Type", "If-None-Match", "X-API-Key", "X-Tenant-Id", "X-Trace-Id"], expose=["ETag", "X-Trace-Id"], credentials=False, max_age=600)

def cors_application(mode):
    return A.application(routes(), [A.cors(cors_policy(mode))])

def extras_contract(request):
    return Body.bind(request, lambda doc: Body.extras(doc, [], M.Valid(value=None)), lambda _value: health(request))

def response_contract(request):
    return Body.bind(request, lambda doc: Success(value=M.Valid(value=doc)), lambda doc: Body.filtered_response(200, doc, ["content"]))

def validation_contract(request):
    return E.validation([M.ValidationIssue(location=[M.Field(name="body")], kind="fixture", message="Invalid", input=request.body, missing=False, json_input=True, context=M.NoContext())])

def body_routes():
    return [Routing.route("POST", "/contract/extras", extras_contract), Routing.route("POST", "/contract/response", response_contract), Routing.route("POST", "/contract/validation", validation_contract), Routing.route("POST", "/body/idp", lambda request: Body.bind(request, decode_idp, idp_response)), Routing.route("POST", "/body/bundle", lambda request: Body.bind(request, decode_bundle, bundle_response)), Routing.route("POST", "/body/key", lambda request: Body.bind(request, decode_key, key_response)), Routing.route("POST", "/body/context", lambda request: Body.bind(request, decode_context, context_response)), Routing.route("POST", "/body/resource", lambda request: Body.bind(request, decode_resource, resource_response))]

def decode_bundle(document):
    return Body.object_body(document, lambda doc: B.mapped(Body.text(doc, "content"), lambda value: V.mapped(value, lambda content: FixtureBundle(content=content))))

def decode_key(document):
    return Body.object_body(document, lambda doc: B.bind(Body.optional_text(doc, "label"), lambda label: B.mapped(Body.uuid(doc, "user_id"), lambda identity: V.mapped(V.combine(label, identity), lambda pair: FixtureKey(label=pair.first, user_id=pair.second)))))

def decode_context(document):
    return Body.object_body(document, lambda doc: B.bind(Body.uuid(doc, "tenant_id"), lambda tenant: B.bind(Body.text(doc, "provider_user_id"), lambda user: Body.extras(doc, ["tenant_id", "provider_user_id"], V.mapped(V.combine(tenant, user), lambda pair: FixtureContext(tenant_id=pair.first, provider_user_id=pair.second))))))

def resource_first(doc):
    return B.bind(Body.text(doc, "uri"), lambda uri: B.mapped(Body.optional_alias(doc, "mimeType", "mime_type"), lambda mime: V.combine(uri, mime)))

def resource_second(doc):
    return B.bind(Body.optional_text(doc, "text"), lambda text: B.mapped(Body.optional_text(doc, "blob"), lambda blob: V.combine(text, blob)))

def decode_resource(document):
    return Body.object_body(document, lambda doc: B.bind(resource_first(doc), lambda first: B.mapped(resource_second(doc), lambda second: V.mapped(V.combine(first, second), lambda pair: FixtureResource(uri=pair.first.first, mime_type=pair.first.second, text=pair.second.first, blob=pair.second.second)))))

def body_response(fields, allowed):
    return Body.filtered_response(200, JsonObject(listConcat(fields, [JsonMember("_secret", JsonString("never expose"))])), allowed)

def bundle_response(value):
    return body_response([JsonMember("content", JsonString(value.content))], ["content"])

def key_response(value):
    return body_response([JsonMember("label", Body.optional_json(value.label)), JsonMember("user_id", JsonString(value.user_id))], ["label", "user_id"])

def context_response(value):
    return E.json_response(200, JsonObject([JsonMember("tenant_id", JsonString(value.tenant_id)), JsonMember("provider_user_id", JsonString(value.provider_user_id))]), "")

def resource_response(value):
    return body_response([JsonMember("uri", JsonString(value.uri)), JsonMember("mimeType", Body.optional_json(value.mime_type)), JsonMember("text", Body.optional_json(value.text)), JsonMember("blob", Body.optional_json(value.blob))], ["uri", "mimeType", "text", "blob"])

def decode_idp(document):
    return Body.object_body(document, idp_fields)

def idp_fields(doc):
    first = B.bind(Body.array(doc, "allowed_algs", Body.string), lambda algorithms: B.mapped(Body.text(doc, "audience"), lambda audience: V.combine(algorithms, audience)))
    second = B.bind(Body.text(doc, "issuer"), lambda issuer: B.mapped(Body.text(doc, "jwks_uri"), lambda uri: V.combine(issuer, uri)))
    return B.bind(first, lambda left: B.mapped(second, lambda right: V.mapped(V.combine(left, right), lambda pair: FixtureIdp(allowed_algs=pair.first.first, audience=pair.first.second, issuer=pair.second.first, jwks_uri=pair.second.second))))

def idp_response(value):
    return body_response([JsonMember("allowed_algs", JsonArray(mapList(value.allowed_algs, lambda algorithm: JsonString(algorithm)))), JsonMember("audience", JsonString(value.audience)), JsonMember("issuer", JsonString(value.issuer)), JsonMember("jwks_uri", JsonString(value.jwks_uri))], ["allowed_algs", "audience", "issuer", "jwks_uri"])

# Generated fixture models from models.td

class FixtureBundle:
    """NAP bundle file body generated from its typeDiagram contract."""
    content: str

class FixtureKey:
    """NAP API-key request with nullable label and normalized UUID."""
    label: M.Optional[str]
    user_id: str

class FixtureContext:
    """Strict NAP tenant and provider identity body."""
    tenant_id: str
    provider_user_id: str

class FixtureResource:
    """NAP embedded resource with wire aliases and nullable fields."""
    uri: str
    mime_type: M.Optional[str]
    text: M.Optional[str]
    blob: M.Optional[str]

class FixtureIdp:
    """NAP identity-provider request with validated algorithm strings."""
    allowed_algs: list[str]
    audience: str
    issuer: str
    jwks_uri: str

@union
class FixtureValue:
    """Generated application-specific dependency values for HTTP acceptance."""
    class SessionValue:
        id: int
    class IdentityValue:
        id: int
        role: str
    class CreditValue:
        id: int
        role: str
        credit: int
    class NumberValue:
        number: int
