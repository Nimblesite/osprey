# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Errors
"""HTTP exception and validation wire boundary. [FASTAPI-NAP-ERRORS]"""
from fastapi import Models as M
from fastapi import Responses as R

__all__ = ["exception", "http_exception", "validation", "issue", "json_response"]


def exception(status_code, detail, headers):
    """Create a typed HTTP failure carrying status, detail and response headers."""
    return M.HTTPException(status_code=status_code, detail=detail, headers=headers)


def json_response(status, document, headers):
    """Serialize a boundary JSON value; serialization failure becomes an explicit HTTP 500."""
    match jsonEncode(document):
        case Success(value=text):
            return R.json_response(status, text, headers)
        case Error():
            return R.response(500, "Internal Server Error", "text/plain; charset=utf-8", "")


def http_exception(error):
    """Render a typed HTTP exception as a JSON detail response."""
    return json_response(error.status_code, JsonObject([JsonMember("detail", JsonString(error.detail))]), error.headers)


def issue(location, kind, message, input, missing, context):
    """Construct one typed validation issue with its location, input and context."""
    return M.ValidationIssue(location=location, kind=kind, message=message, input=input, missing=missing, context=context)


def context_fields(context):
    match context:
        case M.NoContext():
            return []
        case M.TextContext(name=name, value=text):
            return context_member(name, JsonString(text))
        case M.NumberContext(name=name, value=number):
            return context_member(name, JsonInt(number))


def context_member(name, encoded):
    return [JsonMember("ctx", JsonObject([JsonMember(name, encoded)]))]


def issue_json(error):
    fields = [JsonMember("type", JsonString(error.kind)), JsonMember("loc", JsonArray(mapList(error.location, lambda part: JsonString(part)))), JsonMember("msg", JsonString(error.message)), JsonMember("input", JsonNull() if error.missing else JsonString(error.input))]
    return JsonObject(listConcat(fields, context_fields(error.context)))


def validation(issues):
    """Render all collected validation issues as the standard HTTP 422 detail array."""
    return json_response(422, JsonObject([JsonMember("detail", JsonArray(mapList(issues, issue_json)))]), "")
