# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Uuids
"""UUID request conversion with reference-compatible issues. [FASTAPI-NAP-PARAMETERS]"""
from fastapi import Models as M
from fastapi import Results as B
from fastapi import Validation as V
__all__ = ['uuid']

def unwrapped(text):
    if startsWith(text, 'urn:uuid:'):
        return Success(value=drop(text, 9))
    elif startsWith(text, '{') and endsWith(text, '}'):
        return B.bind(indexOf(drop(text, 1), '}'), lambda index: Success(value=take(drop(text, 1), index)))
    else:
        return Success(value=text)

def bad_character(text, original):
    return B.bind(checkedSub(len(original), len(text)), lambda offset: B.bind(checkedAdd(offset, 1), lambda index: Error(message=f"invalid character: expected an optional prefix of `urn:uuid:` followed by [0-9a-fA-F-], found `{take(text, 1)}` at {index}")))


def characters(text, original):
    if text == '':
        return Success(value=original)
    elif contains('0123456789abcdefABCDEF-', take(text, 1)):
        return characters(drop(text, 1), original)
    else:
        return bad_character(text, original)

def canonical(hex):
    return take(hex, 8) + '-' + take(drop(hex, 8), 4) + '-' + take(drop(hex, 12), 4) + '-' + take(drop(hex, 16), 4) + '-' + drop(hex, 20)

def lengths(parts):
    match parts:
        case [a, b, c, d, e]:
            return group_length(a, 8, 0, group_length(b, 4, 1, group_length(c, 4, 2, group_length(d, 4, 3, group_length(e, 12, 4, Success(value=None))))))
        case _:
            count = len(parts)
            return Error(message=f'invalid group count: expected 5, found {count}')

def group_length(text, expected, group, next):
    found = len(text)
    return next if found == expected else Error(message=f'invalid group length in group {group}: expected {expected}, found {found}')

def formatted(text, wrapped):
    if not contains(text, '-') and not wrapped:
        found = len(text)
        return Success(value=canonical(toLowerCase(text))) if found == 32 else Error(message=f'invalid length: expected length 32 for simple format, found {found}')
    else:
        return B.bind(split(text, '-'), lambda parts: B.bind(lengths(parts), lambda _unused: Success(value=toLowerCase(text))))


def parsed(text):
    return B.bind(unwrapped(text), lambda inner: B.bind(characters(inner, inner), lambda _unused: formatted(inner, startsWith(text, '{') or startsWith(text, 'urn:uuid:'))))


def invalid_uuid(location, text, message):
    return V.invalid(location, 'uuid_parsing', f'Input should be a valid UUID, {message}', text, M.TextContext(name="error", value=message))


def uuid(location, result):
    """Validate and normalize a UUID, retaining the original input in any validation issue."""
    match V.required(location, result):
        case M.Invalid(issues=errors):
            return M.Invalid(issues=errors)
        case M.Valid(value=text):
            match parsed(text):
                case Success(value=normalized):
                    return M.Valid(value=normalized)
                case Error(message=message):
                    return invalid_uuid(location, text, message)
