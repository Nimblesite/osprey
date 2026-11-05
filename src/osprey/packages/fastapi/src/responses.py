# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Responses
"""HTTP response construction at the transport boundary. [FASTAPI-HTTP-RESPONSES]"""
from fastapi import Models as M

__all__ = ["response", "exception_reply", "json_response", "not_found", "method_not_allowed", "redirect", "native"]


def response(status_code, content, media_type, headers):
    """Construct a response with explicit status, body, media type and raw headers."""
    return build_reply(status_code, content, media_type, headers, M.Absent())


def build_reply(status_code, content, media_type, headers, failure):
    return M.Response(status_code=status_code, content=content, media_type=media_type, headers=headers, failure=failure)


def exception_reply(error, content):
    """Carry a typed failure alongside its default wire response."""
    return build_reply(error.status_code, content, "application/json", error.headers, M.Present(value=error))


def json_response(status_code, content, headers):
    """Construct a response whose body is already serialized JSON text."""
    return response(status_code, content, "application/json", headers)


def not_found():
    """The standard JSON 404 response."""
    return exception_reply(M.HTTPException(status_code=404, detail="Not Found", headers=""), '{"detail":"Not Found"}')


def method_not_allowed(methods):
    """The standard JSON 405 response with the allowed method header."""
    return exception_reply(M.HTTPException(status_code=405, detail="Method Not Allowed", headers=f"Allow: {methods}"), '{"detail":"Method Not Allowed"}')


def redirect(path):
    """A 307 redirect preserving the request method."""
    return response(307, "", "", f"Location: {path}")


def native(reply):
    """Translate a typed response to the runtime HTTP ABI and CRLF header framing."""
    return HttpResponse(status=reply.status_code, headers=wire_headers(reply), contentType=reply.media_type, streamFd=-1, isComplete=True, partialBody=reply.content)


def wire_headers(reply):
    content_type = "" if reply.media_type == "" else f"Content-Type: {reply.media_type}\r\n"
    extra = "" if reply.headers == "" else reply.headers + "\r\n"
    return content_type + extra
