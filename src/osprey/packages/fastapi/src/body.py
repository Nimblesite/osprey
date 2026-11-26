# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Body
"""JSON stays at the HTTP boundary; handlers receive validated typed records. [FASTAPI-HTTP-VALIDATION]"""
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Results as B
from fastapi import Validation as V
from fastapi import Params as P
from fastapi import Responses as R
from fastapi import Uuids

__all__ = ["bind", "bind_binary", "object_body", "object_at", "nested", "text", "text_at", "optional_text", "optional_alias", "uuid", "extras", "filtered_response", "optional_json", "array", "array_at", "string"]

REQUIRED = "Field required"
"""The message of every `missing` validation error, as pydantic words it."""


def invalid(location, kind, message, input):
    return invalid_at(mapList(location, lambda name: M.Field(name=name)), kind, message, input)


def invalid_at(location, kind, message, input):
    match jsonEncode(input):
        case Success(value=encoded):
            return Success(value=M.Invalid(issues=[M.ValidationIssue(location=location, kind=kind, message=message, input=encoded, missing=False, json_input=True, context=M.NoContext())]))
        case Error(message=message):
            return Error(message=message)


def body_path():
    return [M.Field(name="body")]


def object_at(document, path, decoder):
    """Check an object at an explicit validation location.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.object_at(document, [M.Field(name="body")], lambda value, path: Body.text_at(value, "name", path))
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=name):
                        return f"valid: {name}"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    print(f"[{verdict(JsonObject([JsonMember('name', JsonString('ada'))]))}] [{verdict(JsonString('nope'))}]")
    ```
    ```output
    [valid: ada] [invalid(1)]
    ```
    """
    match document:
        case JsonObject():
            return decoder(document, path)
        case _:
            return invalid_at(path, "model_attributes_type", "Input should be a valid dictionary or object to extract fields from", document)


def object_body(document, decoder):
    """Check an object at the body's location `["body"]`, handing the decoder the value alone.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.object_body(document, lambda value: Body.text(value, "name"))
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=name):
                        return f"valid: {name}"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    print(f"[{verdict(JsonObject([JsonMember('name', JsonString('ada'))]))}] [{verdict(JsonString('nope'))}]")
    ```
    ```output
    [valid: ada] [invalid(1)]
    ```
    """
    return object_at(document, body_path(), lambda value, _path: decoder(value))


def required_member(document, name, parent, decoder):
    path = listConcat(parent, [M.Field(name=name)])
    match jsonField(document, name):
        case Error():
            return invalid_at(path, "missing", REQUIRED, document)
        case Success(value=field):
            return decoder(field, path)


def nested(document, name, parent, decoder):
    """Validate a required child object and pass its full location to the decoder.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.nested(document, "user", [M.Field(name="body")], lambda value, path: Body.text_at(value, "name", path))
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=name):
                        return f"valid: {name}"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    user = JsonObject([JsonMember("name", JsonString("grace"))])
    print(f"[{verdict(JsonObject([JsonMember('user', user)]))}] [{verdict(JsonObject([]))}]")
    ```
    ```output
    [valid: grace] [invalid(1)]
    ```
    """
    return required_member(document, name, parent, lambda field, path: object_at(field, path, decoder))


def text_at(document, name, parent):
    """Validate a required string at a nested model location.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.text_at(document, "name", [M.Field(name="body"), M.Field(name="user")])
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=name):
                        return f"valid: {name}"
                    case M.Invalid(issues=issues):
                        match listGet(issues, 0):
                            case Success(value=issue):
                                return f"invalid: {issue.kind}"
                            case Error():
                                return "none"
            case Error():
                return "error"

    user = JsonObject([JsonMember("name", JsonString("ada"))])
    wrong = JsonObject([JsonMember("name", JsonInt(7))])
    print(f"[{verdict(user)}] [{verdict(wrong)}]")
    ```
    ```output
    [valid: ada] [invalid: string_type]
    ```
    """
    return required_member(document, name, parent, string)


def text(document, name):
    """Validate a required string member of the body, located at `["body", name]`.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.text(document, "name")
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=name):
                        return f"valid: {name}"
                    case M.Invalid(issues=issues):
                        match listGet(issues, 0):
                            case Success(value=issue):
                                return f"invalid: {issue.kind}"
                            case Error():
                                return "none"
            case Error():
                return "error"

    given = JsonObject([JsonMember("name", JsonString("ada"))])
    wrong = JsonObject([JsonMember("name", JsonInt(7))])
    print(f"[{verdict(given)}] [{verdict(wrong)}] [{verdict(JsonObject([]))}]")
    ```
    ```output
    [valid: ada] [invalid: string_type] [invalid: missing]
    ```
    """
    return text_at(document, name, body_path())


def optional_text(document, name):
    """Validate an optional string member: an absent or null member is `Valid(Absent())`, a present one is `Valid(Present(text))` or its issues.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.optional_text(document, "label")
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=optional):
                        match optional:
                            case M.Present(value=label):
                                return f"present: {label}"
                            case M.Absent():
                                return "absent"
                    case M.Invalid(issues=issues):
                        return f"invalid: {toString(mapList(issues, lambda issue: issue.kind))}"
            case Error():
                return "error"

    given = JsonObject([JsonMember("label", JsonString("beta"))])
    explicit_null = JsonObject([JsonMember("label", JsonNull())])
    wrong = JsonObject([JsonMember("label", JsonInt(7))])
    print(f"[{verdict(given)}] [{verdict(explicit_null)}] [{verdict(JsonObject([]))}] [{verdict(wrong)}]")
    ```
    ```output
    [present: beta] [absent] [absent] [invalid: ["string_type"]]
    ```
    """
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
    """Validate an optional string member under `alias` when the document has that member, else under `name`.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.optional_alias(document, "mimeType", "mime_type")
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=optional):
                        match optional:
                            case M.Present(value=mime):
                                return f"present: {mime}"
                            case M.Absent():
                                return "absent"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    both = JsonObject([JsonMember("mimeType", JsonString("image/png")), JsonMember("mime_type", JsonString("text/plain"))])
    plain = JsonObject([JsonMember("mime_type", JsonString("text/plain"))])
    print(f"[{verdict(both)}] [{verdict(plain)}] [{verdict(JsonObject([]))}]")
    ```
    ```output
    [present: image/png] [present: text/plain] [absent]
    ```
    """
    match jsonField(document, alias):
        case Success():
            return optional_text(document, alias)
        case Error():
            return optional_text(document, name)


def uuid(document, name):
    """Validate a required body member as a UUID, located at `["body", name]`: a string is normalized, anything else is a `uuid_type` issue.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.uuid(document, "user_id")
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=identity):
                        return f"valid: {identity}"
                    case M.Invalid(issues=issues):
                        match listGet(issues, 0):
                            case Success(value=issue):
                                return f"invalid: {issue.kind}"
                            case Error():
                                return "none"
            case Error():
                return "error"

    upper = JsonObject([JsonMember("user_id", JsonString("123E4567-E89B-12D3-A456-426614174000"))])
    bad = JsonObject([JsonMember("user_id", JsonString("zzz"))])
    wrong = JsonObject([JsonMember("user_id", JsonInt(7))])
    print(f"[{verdict(upper)}] [{verdict(bad)}] [{verdict(wrong)}] [{verdict(JsonObject([]))}]")
    ```
    ```output
    [valid: 123e4567-e89b-12d3-a456-426614174000] [invalid: uuid_parsing] [invalid: uuid_type] [invalid: missing]
    ```
    """
    match jsonField(document, name):
        case Error():
            return invalid(["body", name], "missing", REQUIRED, document)
        case Success(value=field):
            match field:
                case JsonString(value):
                    return Success(value=Uuids.uuid(["body", name], Success(value=value)))
                case _:
                    return invalid(["body", name], "uuid_type", "UUID input should be a string, bytes or UUID object", field)


def extras(document, allowed, validation):
    """Add an `extra_forbidden` issue to `validation` for every member of the object `allowed` does not name; a non-object is an `Error`.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document, allowed):
        outcome = Body.extras(document, allowed, M.Valid(value=None))
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid():
                        return "valid"
                    case M.Invalid(issues=issues):
                        return f"invalid: {toString(mapList(issues, lambda issue: issue.kind))}"
            case Error(message=message):
                return f"error: {message}"

    body = JsonObject([JsonMember("tenant_id", JsonString("acme")), JsonMember("_secret", JsonString("x"))])
    narrow = ["tenant_id"]
    full = ["tenant_id", "_secret"]
    print(f"[{verdict(body, narrow)}] [{verdict(body, full)}] [{verdict(JsonString('nope'), narrow)}]")
    ```
    ```output
    [invalid: ["extra_forbidden"]] [valid] [error: Body extras require a validated object]
    ```
    """
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
    """Respond with `status` and a JSON body holding only the members of the object `allowed` names; a non-object is a 500.

    # Examples
    ```osprey
    from fastapi import Body

    document = JsonObject([JsonMember("name", JsonString("ada")), JsonMember("_secret", JsonString("never expose"))])
    kept = Body.filtered_response(201, document, ["name"])
    failed = Body.filtered_response(201, JsonString("nope"), ["name"])
    print(f"[{kept.status_code}] [{kept.content}] [{failed.status_code}] [{failed.content}]")
    ```
    ```output
    [201] [{"name":"ada"}] [500] [Internal Server Error]
    ```
    """
    match document:
        case JsonObject(members):
            return E.json_response(status, JsonObject(filterList(members, lambda member: listContains(allowed, member_name(member)))), "")
        case _:
            return R.response(500, "Internal Server Error", "text/plain; charset=utf-8", "")


def optional_json(value):
    """An optional body value as JSON: `Present(text)` is a JSON string and `Absent` a JSON null.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def shown(value):
        reply = Body.filtered_response(200, JsonObject([JsonMember("label", Body.optional_json(value))]), ["label"])
        return f"[{reply.status_code}] [{reply.content}]"

    print(f"{shown(M.Present(value='beta'))} {shown(M.Absent())}")
    ```
    ```output
    [200] [{"label":"beta"}] [200] [{"label":null}]
    ```
    """
    match value:
        case M.Present(value=text):
            return JsonString(text)
        case M.Absent():
            return JsonNull()


def media_type(request):
    match P.header(request, "Content-Type"):
        case Error():
            return ""
        case Success(value=header):
            match indexOf(header, ";"):
                case Success(value=end):
                    return take(header, end)
                case Error():
                    return header


def bind_binary(request, media_types, max_bytes, endpoint):
    """Pass a bounded nonempty raw body to the endpoint. [FASTAPI-HTTP-FILES]

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M
    from fastapi import Responses

    def shown(request):
        reply = Body.bind_binary(request, ["image/png"], 16, lambda body: Responses.response(201, f"stored {len(body)}", "text/plain", ""))
        return f"[{reply.status_code}] [{reply.content}]"

    png = M.Request(method="POST", url="/upload", headers="Content-Type: image/png", body="PNGDATA", path_params={})
    plain = M.Request(method="POST", url="/upload", headers="Content-Type: text/plain", body="PNGDATA", path_params={})
    empty = M.Request(method="POST", url="/upload", headers="Content-Type: image/png", body="", path_params={})
    print(f"{shown(png)} {shown(plain)} {shown(empty)}")
    ```
    ```output
    [201] [stored 7] [415] [{"detail":"Unsupported Media Type"}] [400] [{"detail":"Empty request body"}]
    ```
    """
    size = byteLength(request.body)
    if not listContains(media_types, media_type(request)):
        return E.exception_response(E.exception(415, "Unsupported Media Type", ""))
    elif size == 0:
        return E.exception_response(E.exception(400, "Empty request body", ""))
    elif size > max_bytes:
        return E.exception_response(E.exception(413, "Request body too large", ""))
    else:
        return endpoint(request.body)


def json_media(media):
    return media == "application/json" or (startsWith(media, "application/") and endsWith(media, "+json"))


def bind(request, decoder, endpoint):
    """Decode the request body, validate it with `decoder` and pass the value to `endpoint`, or render the failure.

    An empty body is a `missing` issue at `["body"]`. A JSON media type is parsed first (a leading UTF-8 BOM is dropped, JSON null is the missing issue and a parse error becomes a 400 or a 422 `json_invalid` issue); any other media type hands the raw body to the decoder as a JSON string. A decoder `Error` is a 500.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M
    from fastapi import Responses

    def post(content_type, body):
        request = M.Request(method="POST", url="/greet", headers="Content-Type: " + content_type, body=body, path_params={})
        reply = Body.bind(request, lambda doc: Body.object_body(doc, lambda value: Body.text(value, "name")), lambda name: Responses.response(200, "hello " + name, "text/plain", ""))
        return f"[{reply.status_code}] [{reply.content}]"

    json_type = "application/json"
    as_json = post(json_type, "{\\"name\\":\\"ada\\"}")
    empty = post(json_type, "")
    broken = post(json_type, "{\\"name\\":")
    as_text = post("text/plain", "ada")
    print(f"{as_json} {empty} {broken} {as_text}")
    ```
    ```output
    [200] [hello ada] [422] [{"detail":[{"type":"missing","loc":["body"],"msg":"Field required","input":null}]}] [422] [{"detail":[{"type":"json_invalid","loc":["body",8],"msg":"JSON decode error","input":{},"ctx":{"error":"Expecting value"}}]}] [422] [{"detail":[{"type":"model_attributes_type","loc":["body"],"msg":"Input should be a valid dictionary or object to extract fields from","input":"ada"}]}]
    ```
    """
    if request.body == "":
        return rendered(invalid(["body"], "missing", REQUIRED, JsonNull()), endpoint)
    elif json_media(trim(toLowerCase(media_type(request)))):
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
            return rendered(invalid(["body"], "missing", REQUIRED, document), endpoint)
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
    return fold(range(position if position < len(source) else len(source)), Success(value=0), lambda result, index: andThen(result, lambda previous: B.mapped(byteAt(source, index), lambda byte: satAdd(previous, 1) if byte < 128 or byte >= 192 else previous)))


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
        return json_error_location(andThen(number, lambda offset: json_location(source, offset)), json_reason(reason))


def json_error_location(position, reason):
    match position:
        case Error(message=message):
            return E.exception_response(E.exception(500, message, ""))
        case Success(value=offset):
            error = JsonObject([JsonMember("type", JsonString("json_invalid")), JsonMember("loc", JsonArray([JsonString("body"), JsonInt(offset)])), JsonMember("msg", JsonString("JSON decode error")), JsonMember("input", JsonObject([])), JsonMember("ctx", JsonObject([JsonMember("error", JsonString(reason))]))])
            return E.json_response(422, JsonObject([JsonMember("detail", JsonArray([error]))]), "")


def array(document, name, decoder):
    """Validate a body array, supplying each item with its full location.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.array(document, "tags", Body.string)
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=items):
                        return f"valid: {toString(items)}"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    good = JsonObject([JsonMember("tags", JsonArray([JsonString("a"), JsonString("b")]))])
    mixed = JsonObject([JsonMember("tags", JsonArray([JsonString("a"), JsonInt(7)]))])
    print(f"[{verdict(good)}] [{verdict(mixed)}]")
    ```
    ```output
    [valid: ["a", "b"]] [invalid(1)]
    ```
    """
    return array_at(document, name, body_path(), decoder)


def array_at(document, name, parent, decoder):
    """Validate an array member at any path, with ordered numeric item locations.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(document):
        outcome = Body.array_at(document, "tags", [M.Field(name="body"), M.Field(name="user")], Body.string)
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=items):
                        return f"valid: {toString(items)}"
                    case M.Invalid(issues=issues):
                        match listGet(issues, 0):
                            case Success(value=issue):
                                return f"invalid: {issue.kind}"
                            case Error():
                                return "none"
            case Error():
                return "error"

    good = JsonObject([JsonMember("tags", JsonArray([JsonString("a"), JsonString("b")]))])
    mixed = JsonObject([JsonMember("tags", JsonArray([JsonString("a"), JsonInt(7)]))])
    print(f"[{verdict(good)}] [{verdict(mixed)}]")
    ```
    ```output
    [valid: ["a", "b"]] [invalid: string_type]
    ```
    """
    return required_member(document, name, parent, lambda field, path: array_field(field, path, decoder))


def array_field(field, path, decoder):
    match field:
        case JsonArray(items):
            return array_items(items, path, decoder, 0)
        case _:
            return invalid_at(path, "list_type", "Input should be a valid list", field)


def array_items(items, path, decoder, index):
    match items:
        case []:
            return Success(value=M.Valid(value=[]))
        case [item, *tail]:
            first = decoder(item, listConcat(path, [M.Index(value=index)]))
            return andThen(first, lambda value: B.mapped(array_items(tail, path, decoder, satAdd(index, 1)), lambda rest: V.mapped(V.combine(value, rest), lambda pair: listConcat([pair.first], pair.second))))


def string(value, path):
    """Validate a string without coercion at its full field or item location.

    # Examples
    ```osprey
    from fastapi import Body
    from fastapi import Models as M

    def verdict(value):
        outcome = Body.string(value, [M.Field(name="name")])
        match outcome:
            case Success(value=result):
                match result:
                    case M.Valid(value=text):
                        return f"valid: {text}"
                    case M.Invalid(issues=issues):
                        return f"invalid({len(issues)})"
            case Error():
                return "error"

    print(f"[{verdict(JsonString('ada'))}] [{verdict(JsonInt(7))}]")
    ```
    ```output
    [valid: ada] [invalid(1)]
    ```
    """
    match value:
        case JsonString(text):
            return Success(value=M.Valid(value=text))
        case _:
            return invalid_at(path, "string_type", "Input should be a valid string", value)
