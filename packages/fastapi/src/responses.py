# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Responses
"""HTTP response construction at the transport boundary. [FASTAPI-HTTP-RESPONSES]"""
from fastapi import Models as M

__all__ = ["response", "json_response", "not_found", "method_not_allowed", "redirect", "native"]


def response(status_code, content, media_type, headers):
    return M.Response(status_code=status_code, content=content, media_type=media_type, headers=headers)


def json_response(status_code, content, headers):
    return response(status_code, content, "application/json", headers)


def not_found():
    return json_response(404, "{\"detail\":\"Not Found\"}", "")


def method_not_allowed(methods):
    return json_response(405, "{\"detail\":\"Method Not Allowed\"}", f"Allow: {methods}")


def redirect(path):
    return response(307, "", "", f"Location: {path}")


def native(reply):
    return HttpResponse(status=reply.status_code, headers=wire_headers(reply), contentType=reply.media_type, streamFd=-1, isComplete=True, partialBody=reply.content)


def wire_headers(reply):
    content_type = "" if reply.media_type == "" else f"Content-Type: {reply.media_type}\r\n"
    extra = "" if reply.headers == "" else reply.headers + "\r\n"
    return content_type + extra
