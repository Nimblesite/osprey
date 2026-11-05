# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Body
"""JSON stays at the HTTP boundary; handlers receive validated typed records. [FASTAPI-NAP-VALIDATION]"""
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Results as B
from fastapi import Validation as V
from fastapi import Params as P
from fastapi import Responses as R
from fastapi import Uuids

__all__ = ["bind", "object_body", "text", "optional_text", "optional_alias", "uuid", "extras", "filtered_response", "optional_json", "array", "string"]


def invalid(location, kind, message, input):
    return invalid_at(mapList(location, lambda name: M.Field(name=name)), kind, message, input)


def invalid_at(location, kind, message, input):
    match jsonEncode(input):
        case Success(value=encoded):
            return Success(value=M.Invalid(issues=[M.ValidationIssue(location=location, kind=kind, message=message, input=encoded, missing=False, json_input=True, context=M.NoContext())]))
        case Error(message=message):
            return Error(message=message)


def object_body(document, decoder):
    match document:
        case JsonObject():
            return decoder(document)
        case _:
            return invalid(["body"], "model_attributes_type", "Input should be a valid dictionary or object to extract fields from", document)


def text(document, name):
    match jsonField(document, name):
        case Error():
            return invalid(["body", name], "missing", "Field required", document)
        case Success(value=field):
            match field:
                case JsonString(value):
                    return Success(value=M.Valid(value=value))
                case _:
                    return invalid(["body", name], "string_type", "Input should be a valid string", field)


def optional_text(document, name):
    match jsonField(document, name):
        case Error():
            return Success(value=M.Valid(value=M.Absent()))
        case Success(value=field):
            match field:
                case JsonNull():
                    return Success(value=M.Valid(value=M.Absent()))
                case _:
                    return B.mapped(text(document, name), lambda result: V.mapped(result, lambda value: M.Present(value=value)))


def optional_alias(document, alias, name):
    match jsonField(document, alias):
        case Success():
            return optional_text(document, alias)
        case Error():
            return optional_text(document, name)


def uuid(document, name):
    match jsonField(document, name):
        case Error():
            return invalid(["body", name], "missing", "Field required", document)
        case Success(value=field):
            match field:
                case JsonString(value):
                    return Success(value=Uuids.uuid(["body", name], Success(value=value)))
                case _:
                    return invalid(["body", name], "uuid_type", "UUID input should be a string, bytes or UUID object", field)


def extras(document, allowed, validation):
    match document:
        case JsonObject(members):
            unknown = filterList(members, lambda member: not listContains(allowed, member_name(member)))
            return extra_results(unknown, validation)
        case _:
            return Error(message="Body extras require a validated object")


def extra_results(unknown, validation):
    match unknown:
        case []:
            return Success(value=validation)
        case [member, *tail]:
            match extra_issue(member):
                case Success(value=issue):
                    return extra_results(tail, M.Invalid(issues=listConcat(V.issues_of(validation), [issue])))
                case Error(message=message):
                    return Error(message=message)


def member_name(member):
    match member:
        case JsonMember(name, _value):
            return name


def extra_issue(member):
    match member:
        case JsonMember(name, value):
            return B.mapped(jsonEncode(value), lambda encoded: M.ValidationIssue(location=[M.Field(name="body"), M.Field(name=name)], kind="extra_forbidden", message="Extra inputs are not permitted", input=encoded, missing=False, json_input=True, context=M.NoContext()))


def filtered_response(status, document, allowed):
    match document:
        case JsonObject(members):
            return E.json_response(status, JsonObject(filterList(members, lambda member: listContains(allowed, member_name(member)))), "")
        case _:
            return R.response(500, "Internal Server Error", "text/plain; charset=utf-8", "")


def optional_json(value):
    match value:
        case M.Present(value=text):
            return JsonString(text)
        case M.Absent():
            return JsonNull()


def is_json(request):
    match P.header(request, "Content-Type"):
        case Error():
            return False
        case Success(value=header):
            match indexOf(header, ";"):
                case Success(value=end):
                    return json_media(trim(toLowerCase(take(header, end))))
                case Error():
                    return json_media(trim(toLowerCase(header)))


def json_media(media):
    return media == "application/json" or (startsWith(media, "application/") and endsWith(media, "+json"))


def bind(request, decoder, endpoint):
    if request.body == "":
        return rendered(invalid(["body"], "missing", "Field required", JsonNull()), endpoint)
    elif is_json(request):
        source = drop(request.body, len("\ufeff")) if startsWith(request.body, "\ufeff") else request.body
        match jsonDecodePython(source):
            case Success(value=document):
                return bound_document(document, decoder, endpoint)
            case Error(message=message):
                return json_error(source, message)
    else:
        return rendered(decoder(JsonString(request.body)), endpoint)


def rendered(result, endpoint):
    match result:
        case Error():
            return R.response(500, "Internal Server Error", "text/plain; charset=utf-8", "")
        case Success(value=outcome):
            match outcome:
                case M.Valid(value=value):
                    return endpoint(value)
                case M.Invalid(issues=issues):
                    return E.validation(issues)



def bound_document(document, decoder, endpoint):
    match document:
        case JsonNull():
            return rendered(invalid(["body"], "missing", "Field required", document), endpoint)
        case _:
            return rendered(decoder(document), endpoint)


def json_reason(reason):
    if reason == "expected a string" or reason == "expected a member name":
        return "Expecting property name enclosed in double quotes"
    elif startsWith(reason, "expected ','"):
        return "Expecting ',' delimiter"
    elif startsWith(reason, "expected ':'"):
        return "Expecting ':' delimiter"
    elif reason == "unescaped NUL in string":
        return "Invalid control character at"
    elif reason == "trailing text after the value":
        return "Extra data"
    else:
        return "Expecting value"


def json_location(source, position):
    return fold(range(position if position < len(source) else len(source)), Success(value=0), lambda result, index: B.bind(result, lambda previous: B.mapped(byteAt(source, index), lambda byte: satAdd(previous, 1) if byte < 128 or byte >= 192 else previous)))


def json_error(source, message):
    prefix = "jsonDecode: byte "
    match indexOf(drop(message, len(prefix)), ": "):
        case Success(value=end):
            number = parseInt(take(drop(message, len(prefix)), end))
            reason = drop(drop(message, len(prefix)), satAdd(end, 2))
            return json_error_response(source, number, reason)
        case Error():
            return E.exception_response(E.exception(500, message, ""))


def json_error_response(source, number, reason):
    if reason == "truncated wide JSON encoding":
        return E.exception_response(E.exception(400, "There was an error parsing the body", ""))
    else:
        return json_error_location(B.bind(number, lambda offset: json_location(source, offset)), json_reason(reason))


def json_error_location(position, reason):
    match position:
        case Error(message=message):
            return E.exception_response(E.exception(500, message, ""))
        case Success(value=offset):
            error = JsonObject([JsonMember("type", JsonString("json_invalid")), JsonMember("loc", JsonArray([JsonString("body"), JsonInt(offset)])), JsonMember("msg", JsonString("JSON decode error")), JsonMember("input", JsonObject([])), JsonMember("ctx", JsonObject([JsonMember("error", JsonString(reason))]))])
            return E.json_response(422, JsonObject([JsonMember("detail", JsonArray([error]))]), "")


def array(document, name, decoder):
    """Validate every array item, retaining ordered numeric error locations."""
    match jsonField(document, name):
        case Error():
            return invalid(["body", name], "missing", "Field required", document)
        case Success(value=field):
            match field:
                case JsonArray(items):
                    return array_items(items, name, decoder, 0)
                case _:
                    return invalid(["body", name], "list_type", "Input should be a valid list", field)


def array_items(items, name, decoder, index):
    match items:
        case []:
            return Success(value=M.Valid(value=[]))
        case [item, *tail]:
            return B.bind(decoder(item), lambda first: B.mapped(array_items(tail, name, decoder, satAdd(index, 1)), lambda rest: V.mapped(V.combine(indexed(first, name, index), rest), lambda pair: listConcat([pair.first], pair.second))))


def indexed(validation, name, index):
    match validation:
        case M.Valid(value=value):
            return M.Valid(value=value)
        case M.Invalid(issues=issues):
            return M.Invalid(issues=mapList(issues, lambda issue: M.ValidationIssue(location=listConcat([M.Field(name="body"), M.Field(name=name), M.Index(value=index)], issue.location), kind=issue.kind, message=issue.message, input=issue.input, missing=issue.missing, json_input=issue.json_input, context=issue.context)))


def string(value):
    match value:
        case JsonString(text):
            return Success(value=M.Valid(value=text))
        case _:
            return invalid_at([], "string_type", "Input should be a valid string", value)
