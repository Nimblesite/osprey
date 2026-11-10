# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Urls
"""URL percent decoding, including UTF-8 and form-style plus. [FASTAPI-HTTP-PARAMETERS]"""
from fastapi import Results as B
__all__ = ['decode', 'path_of', 'query_of']

def path_of(url):
    """Return the raw path preceding the query separator."""
    match indexOf(url, '?'):
        case Success(value=index):
            return take(url, index)
        case Error():
            return url

def query_of(url):
    """Return the raw query following the first query separator, or an empty string."""
    return drop(drop(url, len(path_of(url))), 1)

def hex_byte(text):
    if len(text) < 2:
        return Error(message='Incomplete percent escape')
    else:
        return complete_hex(text)

def complete_hex(text):
    return andThen(indexOf('0123456789abcdef', toLowerCase(take(text, 1))), lambda high: andThen(indexOf('0123456789abcdef', toLowerCase(take(drop(text, 1), 1))), lambda low: Success(value=bitOr(shiftLeft(high, 4), low))))

def piece(value, rest):
    return Decoded(value=value, rest=rest)

def replacement(rest):
    return Success(value=piece('�', rest))

def continuation(text):
    if not startsWith(text, '%'):
        return Error(message='Expected encoded continuation')
    else:
        return continuation_byte(text)

def continuation_byte(text):
    return andThen(hex_byte(drop(text, 1)), lambda byte: Success(value=byte) if byte >= 128 and byte <= 191 else Error(message='Invalid UTF-8 continuation'))

def utf8(text, remaining, scalar, minimum, maximum):
    if remaining == 0:
        return andThen(fromCodePoint(scalar), lambda value: Success(value=piece(value, text)))
    else:
        return extend_utf8(text, remaining, scalar, minimum, maximum)

def extend_utf8(text, remaining, scalar, minimum, maximum):
    match continuation(text):
        case Error():
            return replacement(text)
        case Success(value=byte):
            if byte < minimum or byte > maximum:
                return replacement(text)
            else:
                count = 0 if remaining == 1 else 1 if remaining == 2 else 2
                return utf8(drop(text, 3), count, bitOr(shiftLeft(scalar, 6), bitAnd(byte, 63)), 128, 191)

def encoded_byte(byte, rest):
    if byte == 0:
        return Error(message='NUL is not representable in the native HTTP string boundary')
    elif byte < 128:
        return andThen(fromCodePoint(byte), lambda value: Success(value=piece(value, rest)))
    elif byte >= 194 and byte <= 223:
        return utf8(rest, 1, bitAnd(byte, 31), 128, 191)
    elif byte >= 224 and byte <= 239:
        return utf8(rest, 2, bitAnd(byte, 15), 160 if byte == 224 else 128, 159 if byte == 237 else 191)
    elif byte >= 240 and byte <= 244:
        return utf8(rest, 3, bitAnd(byte, 7), 144 if byte == 240 else 128, 143 if byte == 244 else 191)
    else:
        return replacement(rest)

def percent_piece(text):
    match hex_byte(drop(text, 1)):
        case Error():
            return Success(value=piece('%', drop(text, 1)))
        case Success(value=byte):
            return encoded_byte(byte, drop(text, 3))

def next_piece(text, plus):
    if startsWith(text, '%'):
        return percent_piece(text)
    else:
        return literal_piece(text, plus)

def literal_piece(text, plus):
    value = ' ' if plus and startsWith(text, '+') else take(text, 1)
    return Success(value=piece(value, drop(text, 1)))

def decode(text, plus):
    """Decode UTF-8 percent escapes and optional form-style plus; reject unrepresentable NUL."""
    if text == '':
        return Success(value='')
    else:
        return nonempty(text, plus)

def nonempty(text, plus):
    return andThen(next_piece(text, plus), lambda part: andThen(decode(part.rest, plus), lambda rest: Success(value=part.value + rest)))

# Generated private models from models.td


class Decoded:
    """One decoded URL fragment and the unconsumed input."""
    value: str
    rest: str
