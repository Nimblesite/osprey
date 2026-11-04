# osprey: flavor=python
"""Loaded real HTTP use cases. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Applications as A
from fastapi import Routing
from fastapi import Results as B
from fastapi import Models as M
from fastapi_tests import Fixture


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
    return expect(httpCloseClient(client), Success(value=0))


def use_server(server):
    match httpCreateClient("http://127.0.0.1:18787", 5000):
        case Success(value=client):
            requests(client)
        case Error(message=message):
            check("client creation", "success", message)
    return expect(httpStopServer(server), Success(value=0))


def lifecycle():
    routes = Routing.include_router(Fixture.routes(), "/api", Fixture.routes())
    match A.serve(18787, "127.0.0.1", routes):
        case Success(value=server):
            match A.serve(18787, "127.0.0.1", routes):
                case Success(value=unexpected):
                    check("occupied listener", "error", "success")
                    expect(httpStopServer(unexpected), Success(value=0))
                case Error():
                    expect(True, True)
            use_server(server)
        case Error(message=message):
            check("server creation", "success", message)
    match A.serve(0, "127.0.0.1", routes):
        case Success(value=server):
            return expect(httpStopServer(server), Error(message="invalid port must fail"))
        case Error():
            return expect(True, True)


def main():
    return test("FastAPI native HTTP lifecycle and NAP health/readiness", lifecycle)


def header_members(members):
    match members:
        case []:
            return Success(value=[])
        case [head, *tail]:
            return B.bind(header_member(head), lambda first: B.mapped(header_members(tail), lambda rest: listConcat([first], rest)))


def header_member(member):
    match member:
        case JsonMember(name, item):
            return B.mapped(jsonText(item), lambda text: f"{name}: {text}\r\n")


def request_headers(document):
    match document:
        case JsonObject(members):
            return B.mapped(header_members(members), lambda parts: join(parts, ""))
        case _:
            return Error(message="Oracle headers must be an object")


def scenario(document):
    path = B.bind(jsonField(document, "path"), lambda item: jsonText(item))
    status = B.bind(jsonField(document, "status"), lambda item: jsonInt(item))
    headers = B.bind(jsonField(document, "headers"), request_headers)
    body = B.bind(jsonField(document, "body_text"), lambda item: jsonText(item))
    return B.bind(path, lambda url: B.bind(headers, lambda wire: B.bind(status, lambda code: B.mapped(body, lambda expected: M.Pair(first=url, second=M.Pair(first=wire, second=M.Pair(first=code, second=expected)))))))


def oracle_response(response, expected):
    match response:
        case Success(value=handle):
            expect(httpResponseStatus(handle), expected.first)
            match httpResponseBody(handle):
                case Success(value=text):
                    expect(text, expected.second)
                case Error(message=message):
                    check("oracle response body", "success", message)
            return expect(httpResponseFree(handle), Success(value=0))
        case Error(message=message):
            return check("oracle request", "success", message)


def wire_case(client, document):
    match scenario(document):
        case Success(value=parsed):
            return oracle_response(httpGetResponse(client, parsed.first, parsed.second.first), parsed.second.second)
        case Error(message=message):
            return check("oracle case", "valid scenario", message)


def oracle_document(client, document):
    match document:
        case JsonArray(cases):
            return forEachList(cases, lambda item: wire_case(client, item))
        case _:
            return check("oracle corpus", "JSON array", "other JSON value")


def parameter_cases(client):
    source = B.bind(readFile("packages/fastapi/tests/parameters.expected.json"), lambda text: jsonDecode(text))
    match source:
        case Success(value=document):
            return oracle_document(client, document)
        case Error(message=message):
            return check("oracle corpus", "readable JSON", message)
