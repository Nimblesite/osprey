# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Errors
"""HTTP exception and validation wire boundary. [FASTAPI-HTTP-ERRORS]"""
from fastapi import Models as M
from fastapi import Responses as R
from fastapi import Results as B

__all__ = ["exception", "exception_response", "validation", "issue", "json_response"]


def exception(status_code, detail, headers):
    """Create a typed HTTP failure carrying status, detail and response headers."""
    return M.HTTPException(status_code=status_code, detail=detail, headers=headers)


def json_reply(document, render):
    match jsonEncode(document):
        case Success(value=text):
            return render(text)
        case Error():
            return R.response(500, "Internal Server Error", "text/plain; charset=utf-8", "")


def json_response(status, document, headers):
    """Serialize a boundary JSON value; serialization failure becomes an explicit HTTP 500."""
    return json_reply(document, lambda text: R.json_response(status, text, headers))


def exception_response(error):
    """Carry the typed exception and its default JSON detail response."""
    return json_reply(JsonObject([JsonMember("detail", JsonString(error.detail))]), lambda text: R.exception_reply(error, text))


def issue(location, kind, message, input, missing, context):
    """Construct one typed validation issue with its location, input and context."""
    return M.ValidationIssue(location=mapList(location, lambda name: M.Field(name=name)), kind=kind, message=message, input=input, missing=missing, json_input=False, context=context)


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
    source = jsonDecodePython(error.input) if error.json_input else Success(value=JsonNull() if error.missing else JsonString(error.input))
    return B.mapped(source, lambda input: issue_fields(error, input))


def issue_fields(error, input):
    fields = [JsonMember("type", JsonString(error.kind)), JsonMember("loc", JsonArray(mapList(error.location, location_json))), JsonMember("msg", JsonString(error.message)), JsonMember("input", input)]
    return JsonObject(listConcat(fields, context_fields(error.context)))


def validation(issues):
    """Render all collected validation issues as the standard HTTP 422 detail array."""
    collected = foldList(issues, Success(value=[]), lambda result, item: resultAndThen(result, lambda previous: B.mapped(issue_json(item), lambda encoded: listConcat(previous, [encoded]))))
    match collected:
        case Success(value=encoded):
            return json_response(422, JsonObject([JsonMember("detail", JsonArray(encoded))]), "")
        case Error(message=message):
            return R.response(500, f"Invalid validation issue input: {message}", "text/plain; charset=utf-8", "")


def location_json(part):
    match part:
        case M.Field(name=name):
            return JsonString(name)
        case M.Index(value=index):
            return JsonInt(index)
