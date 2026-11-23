# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Responses
"""HTTP response construction at the transport boundary. [FASTAPI-HTTP-RESPONSES]"""
from fastapi import Models as M

__all__ = ["response", "exception_reply", "json_response", "not_found", "method_not_allowed", "redirect", "native"]


def response(status_code, content, media_type, headers):
    """Construct a response with explicit status, body, media type and raw headers.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.response(201, "created", "text/plain", "")
    print(f"[{reply.status_code}] [{reply.content}] [{reply.media_type}] [{reply.headers}]")
    ```
    ```output
    [201] [created] [text/plain] []
    ```
    """
    return build_reply(status_code, content, media_type, headers, M.Absent())


def build_reply(status_code, content, media_type, headers, failure):
    return M.Response(status_code=status_code, content=content, media_type=media_type, headers=headers, failure=failure)


def exception_reply(error, content):
    """Carry a typed failure alongside its default wire response.

    # Examples
    ```osprey
    from fastapi import Responses
    from fastapi import Models as M
    error = M.HTTPException(status_code=418, detail="I am a teapot", headers="")
    reply = Responses.exception_reply(error, '{"detail":"I am a teapot"}')
    print(f"[{reply.status_code}] [{reply.media_type}] [{reply.headers}]")
    ```
    ```output
    [418] [application/json] []
    ```
    """
    return build_reply(error.status_code, content, "application/json", error.headers, M.Present(value=error))


def json_response(status_code, content, headers):
    """Construct a response whose body is already serialized JSON text.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.json_response(409, '{"conflict":true}', "")
    print(f"[{reply.status_code}] [{reply.content}] [{reply.media_type}]")
    ```
    ```output
    [409] [{"conflict":true}] [application/json]
    ```
    """
    return response(status_code, content, "application/json", headers)


def not_found():
    """The standard JSON 404 response.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.not_found()
    print(f"[{reply.status_code}] [{reply.content}] [{reply.media_type}]")
    ```
    ```output
    [404] [{"detail":"Not Found"}] [application/json]
    ```
    """
    return exception_reply(M.HTTPException(status_code=404, detail="Not Found", headers=""), '{"detail":"Not Found"}')


def method_not_allowed(methods):
    """The standard JSON 405 response with the allowed method header.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.method_not_allowed(["POST", "PUT"])
    print(f"[{reply.status_code}] [{reply.content}] [{reply.headers}]")
    ```
    ```output
    [405] [{"detail":"Method Not Allowed"}] [Allow: ["POST", "PUT"]]
    ```
    """
    return exception_reply(M.HTTPException(status_code=405, detail="Method Not Allowed", headers=f"Allow: {methods}"), '{"detail":"Method Not Allowed"}')


def redirect(path):
    """A 307 redirect preserving the request method.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.redirect("/next")
    print(f"[{reply.status_code}] [{reply.content}] [{reply.headers}]")
    ```
    ```output
    [307] [] [Location: /next]
    ```
    """
    return response(307, "", "", f"Location: {path}")


def native(reply):
    """Translate a typed response to the runtime HTTP ABI and CRLF header framing.

    # Examples
    ```osprey
    from fastapi import Responses
    reply = Responses.native(Responses.json_response(200, '{"ok":true}', "X-Trace: t"))
    print(f"[{reply.status}] [{reply.contentType}] [{reply.partialBody}] [{contains(reply.headers, 'X-Trace: t')}]")
    ```
    ```output
    [200] [application/json] [{"ok":true}] [true]
    ```
    """
    return HttpResponse(status=reply.status_code, headers=wire_headers(reply), contentType=reply.media_type, streamFd=-1, isComplete=True, partialBody=reply.content)


def wire_headers(reply):
    content_type = "" if reply.media_type == "" else f"Content-Type: {reply.media_type}\r\n"
    extra = "" if reply.headers == "" else reply.headers + "\r\n"
    return content_type + extra
