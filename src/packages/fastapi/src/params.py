# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Params
"""Request query/header/path boundaries. [FASTAPI-NAP-PARAMETERS]"""
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
    return B.bind(Urls.decode(name, True), lambda key: B.bind(Urls.decode(encoded, True), lambda decoded: Success(value=M.Parameter(name=key, value=decoded))))

def collect_pairs(parts):
    match parts:
        case []:
            return Success(value=[])
        case [head, *tail]:
            return B.bind(query_pair(head), lambda pair: B.bind(collect_pairs(tail), lambda pairs: Success(value=listConcat([pair], pairs))))

def query_pairs(request):
    """Decode ordered query name/value pairs, preserving repeated and empty values."""
    text = Urls.query_of(request.url)
    if text == '':
        return Success(value=[])
    else:
        return nonempty_pairs(text)

def nonempty_pairs(text):
    return B.bind(split(text, '&'), lambda parts: collect_pairs(filterList(parts, lambda part: part != '')))

def query_all(request, name):
    """Return every decoded query value for a name, in request order."""
    return B.bind(query_pairs(request), lambda pairs: Success(value=mapList(filterList(pairs, lambda pair: pair.name == name), lambda pair: pair.value)))

def first(values):
    match values:
        case []:
            return Error(message='Field required')
        case [value, *_rest]:
            return Success(value=value)

def query(request, name):
    """Return the last query value, or a missing/decoding error."""
    return B.bind(query_all(request, name), lambda values: first(listReverse(values)))

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
    """Read repeated case-insensitive headers, preserving value order."""
    return header_values(request.headers, name)

def header(request, name):
    """Read the first matching header or report that it is required."""
    return first(header_all(request, name))


def path(request, name):
    """Read a decoded named path capture or return an absent-capture error."""
    return mapGet(request.path_params, name)
