# osprey: flavor=python
"""Loaded real HTTP use cases. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Applications as A
from fastapi import Routing
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
