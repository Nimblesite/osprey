# osprey: flavor=python
"""Loaded real HTTP use cases. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Applications as A
from fastapi import Results as B
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Params as P
from fastapi import Paths
from fastapi import Routing
from fastapi import Responses as R
from fastapi import Validation as V
from fastapi import Dependencies as D
from fastapi import Urls
from fastapi_tests import Fixture
from fastapi_expected import Expected


def assert_reply(result, status, body):
    match result:
        case Error(message=message):
            return check("HTTP transport", "success", message)
        case Success(value=reply):
            expect(httpResponseStatus(reply), status)
            expect(httpResponseBody(reply), Success(value=body))
            return expect(httpResponseFree(reply), Success(value=0))


def binary_reply(result, payload):
    match result:
        case Error(message=message):
            return check("Binary echo transport", "success", message)
        case Success(value=reply):
            expect(httpResponseStatus(reply), 201)
            expect(httpResponseHeader(reply, "Content-Type"), Success(value="application/octet-stream"))
            expect(httpResponseHeader(reply, "Content-Length"), Success(value=toString(byteLength(payload))))
            received = httpResponseBody(reply)
            expect(httpResponseFree(reply), Success(value=0))
            match received:
                case Success(value=text):
                    expect(byteLength(text), byteLength(payload))
                    return expect(text == payload, True)
                case Error(message=message):
                    return check("Binary echo response", "success", message)


def binary_request(client, encoded):
    match jsonDecodePython(encoded):
        case Success(value=document):
            match document:
                case JsonString(payload):
                    return binary_reply(httpPostResponse(client, "/echo", payload, ""), payload)
                case _:
                    return check("Binary echo fixture", "string", "non-string")
        case Error(message=message):
            return check("Binary echo fixture", "string", message)


def binary_requests(client, payloads):
    match payloads:
        case []:
            return None
        case [payload, *rest]:
            binary_request(client, payload)
            return binary_requests(client, rest)


def request_metadata_cases(client):
    headers = "X-Probe: one\r\nx-probe: two,three\r\nX-PROBE: "
    path = "/contract/request/%C3%A9/a%2Fb"
    query = "b=2&a=x%26y%3Dz&b=3&empty=&flag"
    assert_reply(httpGetResponse(client, path + "?" + query, headers), 200,
                 '{"path":"/contract/request/é/a/b","query":"b=2&a=x%26y%3Dz&b=3&empty=&flag","pairs":[["b","2"],["a","x&y=z"],["b","3"],["empty",""],["flag",""]],"headers":["one","two,three",""],"literal":false}')
    assert_reply(httpGetResponse(client, "/contract/request/only", ""), 200,
                 '{"path":"/contract/request/only","query":"","pairs":[],"headers":[],"literal":true}')
    assert_reply(httpGetResponse(client, "/contract/router/%3F%23%25%2520?k=a+b&k=%2B%3D&k=%ZZ", ""), 200,
                 '{"path":"/contract/router/?#%%20","query":"k=a+b&k=%2B%3D&k=%ZZ","pairs":[["k","a b"],["k","+="],["k","%ZZ"]],"headers":[],"literal":false}')
    assert_reply(httpGetResponse(client, "/contract/request/no?q=%00", ""), 400,
                 '{"detail":"NUL is not representable in the native HTTP string boundary"}')


def metadata_pair(pair):
    match pair:
        case M.Parameter(name=name, value=value):
            return JsonArray([JsonString(name), JsonString(value)])


def metadata_document(request, path, pairs):
    return JsonObject([JsonMember("path", JsonString(path)), JsonMember("query", JsonString(Urls.query_of(request.url))),
                       JsonMember("pairs", JsonArray(mapList(pairs, lambda pair: metadata_pair(pair)))),
                       JsonMember("headers", JsonArray(mapList(P.header_all(request, "X-Probe"), lambda value: JsonString(value)))),
                       JsonMember("literal", JsonBool(Paths.matches("/contract/request/only", path)))])


def request_metadata(request):
    decoded = B.bind(Urls.decode(Routing.path_of(request.url), False), lambda path: B.mapped(P.query_pairs(request), lambda pairs: metadata_document(request, path, pairs)))
    match decoded:
        case Success(value=document):
            return E.json_response(200, document, "")
        case Error(message=message):
            return E.exception_response(M.HTTPException(status_code=400, detail=message, headers=""))


def metadata_application(request):
    child = M.Route(method="GET", path="/contract/request/{item:path}", handler=lambda current: request_metadata(current))
    return A.dispatch(M.Application(routes=[child], middleware=[], exception_handlers={}), request)


def metadata_router(request):
    child = Routing.route("GET", "/contract/router/{item:path}", request_metadata)
    return Routing.dispatch([child], request)


def manual_response(request):
    mode = unwrap_or(P.query(request, "mode"), "")
    if mode == "redirect":
        return R.redirect("/health?probe=manual")
    elif mode == "method":
        return R.method_not_allowed("POST")
    else:
        return R.not_found()


def manual_validation(request):
    result = V.integer(["query", "limit"], P.query(request, "limit"), 1, 3)
    match result:
        case M.Valid(value=limit):
            return E.json_response(200, JsonObject([JsonMember("limit", JsonInt(limit))]), "")
        case M.Invalid():
            return E.validation(V.issues_of(result))


def contract_provider(request, _children):
    """Exercise each DependencyResult and mixed LocationPart and ValidationContext values on the wire."""
    mode = unwrap_or(P.query(request, "mode"), "")
    if mode == "allow":
        return Fixture.fake_identity(request, _children)
    elif mode == "deny":
        return M.Rejected(error=E.exception(403, "provider denied", ""))
    else:
        issue = M.ValidationIssue(location=[M.Field(name="body"), M.Index(value=2), M.Field(name="role")], kind="literal_error", message="Expected writer", input=mode, missing=False, json_input=False, context=M.TextContext(name="expected", value="'writer'"))
        return M.DependencyInvalid(issues=[issue])


def contract_dependency(request):
    graph = [D.provider("role", [], contract_provider, D.pure_close)]
    return D.execute(graph, [D.need("role", True)], request, lambda _request, values: E.json_response(200, JsonInt(length(values)), ""))


def manual_contract_cases(client):
    redirected = httpGetResponse(client, "/contract/response?mode=redirect", "")
    match redirected:
        case Error(message=message):
            check("Manual redirect", "success", message)
        case Success(value=handle):
            expect(httpResponseStatus(handle), 307)
            expect(httpResponseHeader(handle, "Location"), Success(value="/health?probe=manual"))
            expect(httpResponseBody(handle), Success(value=""))
            expect(httpResponseFree(handle), Success(value=0))
    assert_reply(httpGetResponse(client, "/contract/response?mode=method", ""), 405, '{"detail":"Method Not Allowed"}')
    assert_reply(httpGetResponse(client, "/contract/response?mode=unknown", ""), 404, '{"detail":"Not Found"}')
    assert_reply(httpGetResponse(client, "/contract/limit?limit=2", ""), 200, '{"limit":2}')
    assert_reply(httpGetResponse(client, "/contract/limit?limit=4", ""), 422,
                 '{"detail":[{"type":"less_than_equal","loc":["query","limit"],"msg":"Input should be less than or equal to 3","input":"4","ctx":{"le":3}}]}')
    assert_reply(httpGetResponse(client, "/contract/dependency?mode=allow", ""), 200, "1")
    assert_reply(httpGetResponse(client, "/contract/dependency?mode=deny", ""), 403, '{"detail":"provider denied"}')
    assert_reply(httpGetResponse(client, "/contract/dependency?mode=reader", ""), 422,
                 "{\"detail\":[{\"type\":\"literal_error\",\"loc\":[\"body\",2,\"role\"],\"msg\":\"Expected writer\",\"input\":\"reader\",\"ctx\":{\"expected\":\"'writer'\"}}]}")


def requests(client):
    assert_reply(httpGetResponse(client, "/health", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/health?probe=1", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/api/health", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/api/contract/exception", "X-Trace-Id: mounted"), 429, '{"error":"limit reached","trace_id":"mounted"}')
    assert_reply(httpGetResponse(client, "/readyz", ""), 503, "{\"detail\":\"database unavailable\"}")
    assert_reply(httpGetResponse(client, "/health/", ""), 307, "")
    assert_reply(httpGetResponse(client, "/absent", ""), 404, "{\"detail\":\"Not Found\"}")
    assert_reply(httpPostResponse(client, "/health", "", ""), 405, "{\"detail\":\"Method Not Allowed\"}")
    assert_reply(httpPostResponse(client, "/echo", "héllo ☕", ""), 201, "héllo ☕")
    assert_reply(httpGetResponse(client, "/bundle/%00", ""), 400, "{\"detail\":\"NUL is not representable in the native HTTP string boundary\"}")
    binary_requests(client, ['"\\u0000"', '"\\u0000tail"', '"head\\u0000tail"', '"head\\u0000"', '"é\\u0000😀\\u0000tail"'])
    parameter_cases(client)
    request_metadata_cases(client)
    manual_contract_cases(client)
    configuration_cases(client)
    return expect(httpCloseClient(client), Success(value=0))


def use_server(server, port):
    match httpCreateClient(f"http://127.0.0.1:{port}", 5000):
        case Success(value=client):
            requests(client)
        case Error(message=message):
            check("client creation", "success", message)
    return expect(httpStopServer(server), Success(value=0))


def native_application(port):
    base = Fixture.application(port)
    mounted = A.include_router(base, "/api", base.routes)
    return A.include_router(mounted, "", [Routing.route("GET", "/contract/request/{item:path}", metadata_application),
                                         Routing.route("GET", "/contract/router/{item:path}", metadata_router),
                                         Routing.route("GET", "/contract/response", manual_response),
                                         Routing.route("GET", "/contract/dependency", contract_dependency),
                                         Routing.route("GET", "/contract/limit", manual_validation)])


def open_native(ports):
    return open_application(ports, "normal")


def open_application(ports, mode):
    match ports:
        case []:
            return Error(message="No loopback test listener is available")
        case [port, *tail]:
            match A.serve(port, "127.0.0.1", native_application(port) if mode == "normal" else Fixture.cors_application(mode)):
                case Success(value=server):
                    return Success(value=M.Pair(first=server, second=port))
                case Error(message=message):
                    return next_listener(tail, port, message, mode)


def next_listener(ports, previous, message, mode):
    match open_application(ports, mode):
        case Success(value=opened):
            return Success(value=opened)
        case Error(message=error):
            return Error(message=f"{previous}: {message}; {error}")


def occupied_listener(port):
    match A.serve(port, "127.0.0.1", native_application(port)):
        case Success(value=unexpected):
            check("occupied listener", "error", "success")
            return expect(httpStopServer(unexpected), Success(value=0))
        case Error():
            return expect(True, True)


def fallback_listener(port):
    """Force the first bind to fail, then prove the fallback serves real HTTP."""
    match open_native([port, 18795, 18797, 18799]):
        case Success(value=opened):
            match httpCreateClient(f"http://127.0.0.1:{opened.second}", 5000):
                case Success(value=client):
                    assert_reply(httpGetResponse(client, "/health", ""), 200, '{"status":"ok"}')
                    expect(httpCloseClient(client), Success(value=0))
                case Error(message=message):
                    check("fallback client", "success", message)
            return expect(httpStopServer(opened.first), Success(value=0))
        case Error(message=message):
            return check("fallback listener", "success", message)


def lifespan_status(port):
    match httpCreateClient(f"http://127.0.0.1:{port}", 1000):
        case Error(message=message):
            check("lifespan client", "created", message)
            return 0
        case Success(value=client):
            status = unwrap_or(httpGet(client, "/health", ""), 0)
            expect(httpCloseClient(client), Success(value=0))
            return status


def lifespan_step(path, step, result):
    written = B.bind(readFile(path), lambda previous: writeFile(path, previous + step + "\n"))
    return B.bind(written, lambda _written: result)


def lifespan_start(path, port, mode):
    expect(lifespan_status(port), 200 if mode == "bind" else 0)
    return lifespan_step(path, "start", Error(message="startup refused") if mode == "startup" else Success(value="ready"))


def lifespan_app(state):
    return A.application([Routing.route("GET", "/health", lambda _request: E.json_response(200, JsonString(state), ""))], [])


def lifespan_work(path, port, mode, _server):
    match httpCreateClient(f"http://127.0.0.1:{port}", 1000):
        case Error(message=message):
            return Error(message=message)
        case Success(value=client):
            assert_reply(httpGetResponse(client, "/health", ""), 200, '"ready"')
            expect(httpCloseClient(client), Success(value=0))
            return lifespan_step(path, "work", Error(message="work failed") if mode == "combined" or mode == "work" else Success(value=42))


def lifespan_stop(path, port, mode, state):
    expect(state, "ready")
    expect(lifespan_status(port), 200 if mode == "bind" else 0)
    return lifespan_step(path, "stop", Error(message="shutdown refused") if mode == "combined" or mode == "shutdown" else Success(value=None))


def lifespan_case(path, port, mode, trace):
    expect(writeFile(path, ""), Success(value=0))
    outcome = A.with_lifespan(port, "127.0.0.1", lambda: lifespan_start(path, port, mode), lifespan_app, lambda server: lifespan_work(path, port, mode, server), lambda state: lifespan_stop(path, port, mode, state))
    expect(readFile(path), Success(value=trace))
    return outcome


def lifespan_cases(port):
    path = f"target/fastapi-lifespan-{port}.trace"
    expect(lifespan_case(path, port, "startup", "start\n"), Error(message="startup refused"))
    expect(lifespan_case(path, port, "normal", "start\nwork\nstop\n"), Success(value=42))
    expect(lifespan_case(path, port, "work", "start\nwork\nstop\n"), Error(message="work failed"))
    expect(lifespan_case(path, port, "shutdown", "start\nwork\nstop\n"), Error(message="shutdown refused"))
    return expect(lifespan_case(path, port, "combined", "start\nwork\nstop\n"), Error(message="work failed; lifespan shutdown failed: shutdown refused"))


def lifecycle():
    match open_native([18787, 18789, 18791, 18793]):
        case Success(value=opened):
            occupied_listener(opened.second)
            fallback_listener(opened.second)
            match lifespan_case(f"target/fastapi-lifespan-{opened.second}.trace", opened.second, "bind", "start\nstop\n"):
                case Error(message=message):
                    expect(message != "", True)
                case Success():
                    check("lifespan bind", "error", "success")
            use_server(opened.first, opened.second)
            lifespan_cases(opened.second)
        case Error(message=message):
            check("server creation", "success", message)
    match A.serve(0, "127.0.0.1", native_application(0)):
        case Success(value=server):
            return expect(httpStopServer(server), Error(message="invalid port must fail"))
        case Error():
            return expect(True, True)



def main():
    test("FastAPI native HTTP lifecycle and NAP health/readiness", lifecycle)
    test("FastAPI browser CORS reference over real HTTP", cors_lifecycle)
    return test("FastAPI NAP generated request bodies over real HTTP", body_lifecycle)


def scenario(document):
    path = B.bind(jsonField(document, "path"), lambda item: jsonText(item))
    status = B.bind(jsonField(document, "status"), lambda item: jsonInt(item))
    body = B.bind(jsonField(document, "body_text"), lambda item: jsonText(item))
    return B.bind(path, lambda url: B.bind(status, lambda code: B.mapped(body, lambda expected: M.Pair(first=url, second=M.Pair(first=code, second=expected)))))


def oracle_response(response, expected):
    match response:
        case Success(value=handle):
            expect(httpResponseStatus(handle), expected.first)
            if expected.first != 307:
                expected_type = "text/plain; charset=utf-8" if expected.second == "Internal Server Error" else "application/json"
                expect(httpResponseHeader(handle, "Content-Type"), Success(value=expected_type))
            if expected.first == 401:
                expect(httpResponseHeader(handle, "WWW-Authenticate"), Success(value="Bearer"))
            match httpResponseBody(handle):
                case Success(value=text):
                    expect(text, expected.second)
                case Error(message=message):
                    check("oracle response body", "success", message)
            return expect(httpResponseFree(handle), Success(value=0))
        case Error(message=message):
            return check("oracle request", "success", message)


def wire_case(client, document, headers):
    match scenario(document):
        case Success(value=parsed):
            return oracle_response(httpGetResponse(client, parsed.first, headers), parsed.second)
        case Error(message=message):
            return check("oracle case", "valid scenario", message)


def oracle_document(client, document, headers):
    match document:
        case JsonArray(cases):
            expect(len(headers), len(cases))
            return forEach(range(len(cases)), lambda index: wire_record(client, listGet(cases, index), listGet(headers, index)))
        case _:
            return check("oracle corpus", "JSON array", "other JSON value")


def wire_record(client, document, headers):
    match B.bind(document, lambda item: B.mapped(headers, lambda wire: M.Pair(first=item, second=wire))):
        case Success(value=pair):
            return wire_case(client, pair.first, pair.second)
        case Error(message=message):
            return check("oracle wire record", "present JSON case and Latin-1 headers", message)


def parameter_cases(client):
    document = jsonDecode(Expected.CASES_JSON)
    headers = B.bind(readFile("src/osprey/packages/fastapi/tests/parameters.headers.bin"), lambda text: split(text, "\x1e"))
    source = B.bind(document, lambda item: B.mapped(headers, lambda wire: M.Pair(first=item, second=wire)))
    match source:
        case Success(value=pair):
            return oracle_document(client, pair.first, pair.second)
        case Error(message=message):
            return check("oracle corpus", "readable JSON and Latin-1 wire fixtures", message)



def configuration_cases(client):
    assert_reply(httpGetResponse(client, "/deps/cycle", ""), 500, '{"detail":"Dependency cycle: cycle"}')
    assert_reply(httpGetResponse(client, "/dependency-trace", ""), 200, '{"trace":["open:1","close:1"]}')
    assert_reply(httpGetResponse(client, "/deps/missing", ""), 500, '{"detail":"Expected exactly one dependency provider: missing"}')
    assert_reply(httpGetResponse(client, "/dependency-trace", ""), 200, '{"trace":["open:1","close:1"]}')
    assert_reply(httpGetResponse(client, "/deps/duplicate", ""), 500, '{"detail":"Expected exactly one dependency provider: session"}')
    return assert_reply(httpGetResponse(client, "/dependency-trace", ""), 200, '{"trace":[]}')



def cors_lifecycle():
    return foldList(["cors-nap", "cors-restricted", "cors-all"], None, lambda _state, mode: cors_mode(mode))


def cors_mode(mode):
    match open_application([18781, 18783, 18785], mode):
        case Error(message=message):
            return check("CORS listener", "success", message)
        case Success(value=opened):
            cors_client(opened.second, mode)
            return expect(httpStopServer(opened.first), Success(value=0))


def cors_client(port, mode):
    # The real socket client checks all reference bytes/headers, including OPTIONS.
    command = f"python3 src/osprey/packages/fastapi/tests/e2e.cpython --cors-port {port} {mode}"
    return acceptance_client(command)


def acceptance_client(command):
    match spawnProcess(command, cors_diagnostic):
        case Error(message=message):
            return check("CORS HTTP client", "success", message)
        case Success(value=pid):
            status = awaitProcess(pid)
            cleanupProcess(pid)
            return expect(status, 0)


def cors_diagnostic(_pid, _kind, text):
    return print(text) if (_kind == 1 or _kind == 2) and text != "" else None



def body_lifecycle():
    match open_application([18781, 18783, 18785], "normal"):
        case Error(message=message):
            return check("body listener", "success", message)
        case Success(value=opened):
            acceptance_client(f"python3 src/osprey/packages/fastapi/tests/e2e.cpython --body-port {opened.second}")
            return expect(httpStopServer(opened.first), Success(value=0))
