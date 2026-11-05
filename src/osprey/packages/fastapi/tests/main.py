# osprey: flavor=python
"""Loaded real HTTP use cases. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Applications as A
from fastapi import Routing
from fastapi import Results as B
from fastapi import Models as M
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


def requests(client):
    assert_reply(httpGetResponse(client, "/health", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/health?probe=1", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/api/health", ""), 200, "{\"status\":\"ok\"}")
    assert_reply(httpGetResponse(client, "/readyz", ""), 503, "{\"detail\":\"database unavailable\"}")
    assert_reply(httpGetResponse(client, "/health/", ""), 307, "")
    assert_reply(httpGetResponse(client, "/absent", ""), 404, "{\"detail\":\"Not Found\"}")
    assert_reply(httpPostResponse(client, "/health", "", ""), 405, "{\"detail\":\"Method Not Allowed\"}")
    assert_reply(httpPostResponse(client, "/echo", "héllo ☕", ""), 201, "héllo ☕")
    assert_reply(httpGetResponse(client, "/bundle/%00", ""), 400, "{\"detail\":\"NUL is not representable in the native HTTP string boundary\"}")
    parameter_cases(client)
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
    return A.application(Routing.include_router(base.routes, "/api", base.routes), base.middleware)


def open_native(ports):
    match ports:
        case []:
            return Error(message="No loopback test listener is available")
        case [port, *tail]:
            match A.serve(port, "127.0.0.1", native_application(port)):
                case Success(value=server):
                    return Success(value=M.Pair(first=server, second=port))
                case Error(message=message):
                    return next_listener(tail, port, message)


def next_listener(ports, previous, message):
    match open_native(ports):
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


def lifecycle():
    match open_native([18787, 18789, 18791, 18793]):
        case Success(value=opened):
            occupied_listener(opened.second)
            fallback_listener(opened.second)
            use_server(opened.first, opened.second)
        case Error(message=message):
            check("server creation", "success", message)
    match A.serve(0, "127.0.0.1", native_application(0)):
        case Success(value=server):
            return expect(httpStopServer(server), Error(message="invalid port must fail"))
        case Error():
            return expect(True, True)



def main():
    return test("FastAPI native HTTP lifecycle and NAP health/readiness", lifecycle)


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
