# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Params
"""Request query/header/path boundaries. [FASTAPI-HTTP-PARAMETERS]"""
from fastapi import Models as M
from fastapi import Results as B
from fastapi import Urls
__all__ = ['query', 'query_all', 'header', 'header_all', 'path', 'query_pairs']

def field_value(text, index):
    return drop(drop(text, index), 1)


def query_pair(text):
    match indexOf(text, '='):
        case Success(value=index):
            return decode_pair(take(text, index), field_value(text, index))
        case Error():
            return decode_pair(text, '')

def decode_pair(name, encoded):
    return andThen(Urls.decode(name, True), lambda key: andThen(Urls.decode(encoded, True), lambda decoded: Success(value=M.Parameter(name=key, value=decoded))))

def collect_pairs(parts):
    match parts:
        case []:
            return Success(value=[])
        case [head, *tail]:
            return andThen(query_pair(head), lambda pair: andThen(collect_pairs(tail), lambda pairs: Success(value=listConcat([pair], pairs))))

def query_pairs(request):
    """Decode ordered query name/value pairs, preserving repeated and empty values.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    def decoded(request):
        match Params.query_pairs(request):
            case Success(value=pairs):
                return f"{len(pairs)} {toString(mapList(pairs, lambda pair: pair.name + '=' + pair.value))}"
            case Error():
                return "none"

    def shown():
        request = M.Request(method="GET", url="/items?tag=a&tag=b&empty=&tag=c", headers="", body="", path_params={})
        empty = M.Request(method="GET", url="/items", headers="", body="", path_params={})
        return f"[{decoded(request)}] [{decoded(empty)}]"

    print(shown())
    ```
    ```output
    [4 ["tag=a", "tag=b", "empty=", "tag=c"]] [0 []]
    ```
    """
    text = Urls.query_of(request.url)
    if text == '':
        return Success(value=[])
    else:
        return nonempty_pairs(text)

def nonempty_pairs(text):
    return andThen(split(text, '&'), lambda parts: collect_pairs(filterList(parts, lambda part: part != '')))

def query_all(request, name):
    """Return every decoded query value for a name, in request order.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    request = M.Request(method="GET", url="/items?tag=a&tag=b&q=&tag=c", headers="", body="", path_params={})
    print(f"[{toString(Params.query_all(request, 'tag'))}] [{toString(Params.query_all(request, 'q'))}] [{toString(Params.query_all(request, 'nope'))}]")
    ```
    ```output
    [Success(["a", "b", "c"])] [Success([""])] [Success([])]
    ```
    """
    return andThen(query_pairs(request), lambda pairs: Success(value=mapList(filterList(pairs, lambda pair: pair.name == name), lambda pair: pair.value)))

def first(values):
    match values:
        case []:
            return Error(message='Field required')
        case [value, *_rest]:
            return Success(value=value)

def query(request, name):
    """Return the last query value, or a missing/decoding error.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    request = M.Request(method="GET", url="/items?tag=a&tag=b&empty=", headers="", body="", path_params={})
    print(f"[{toString(Params.query(request, 'tag'))}] [{toString(Params.query(request, 'empty'))}] [{toString(Params.query(request, 'gone'))}]")
    ```
    ```output
    [Success(b)] [Success()] [Error(Field required)]
    ```
    """
    return andThen(query_all(request, name), lambda values: first(listReverse(values)))

def header_line(line):
    match indexOf(line, '\r'):
        case Success(value=index):
            return take(line, index)
        case Error():
            return line

def header_values(headers, name):
    matching = filterList(lines(headers), lambda line: toLowerCase(take(line, len(name))) == toLowerCase(name) and startsWith(drop(line, len(name)), ':'))
    return mapList(matching, lambda line: trimStart(field_value(header_line(line), len(name))))

def header_all(request, name):
    """Read repeated case-insensitive headers, preserving value order.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    request = M.Request(method="GET", url="/items", headers="X-Token: t1\\r\\nx-token: t2\\r\\n", body="", path_params={})
    print(f"[{toString(Params.header_all(request, 'x-token'))}] [{toString(Params.header_all(request, 'X-Token'))}] [{toString(Params.header_all(request, 'Accept'))}]")
    ```
    ```output
    [["t1", "t2"]] [["t1", "t2"]] [[]]
    ```
    """
    return header_values(request.headers, name)

def header(request, name):
    """Read the first matching header or report that it is required.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    request = M.Request(method="GET", url="/items", headers="X-Token: t1\\r\\nx-token: t2\\r\\n", body="", path_params={})
    print(f"[{toString(Params.header(request, 'X-Token'))}] [{toString(Params.header(request, 'x-token'))}] [{toString(Params.header(request, 'Accept'))}]")
    ```
    ```output
    [Success(t1)] [Success(t1)] [Error(Field required)]
    ```
    """
    return first(header_all(request, name))


def path(request, name):
    """Read a decoded named path capture or return an absent-capture error.

    # Examples
    ```osprey
    from fastapi import Params
    from fastapi import Models as M
    request = M.Request(method="GET", url="/users/42", headers="", body="", path_params={"id": "42"})
    stranger = M.Request(method="GET", url="/users/42", headers="", body="", path_params={})
    print(f"[{toString(Params.path(request, 'id'))}] [{toString(Params.path(stranger, 'id'))}]")
    ```
    ```output
    [Success(42)] [Error(mapGet: key not found)]
    ```
    """
    return mapGet(request.path_params, name)
