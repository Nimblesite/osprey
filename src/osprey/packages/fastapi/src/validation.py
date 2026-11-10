# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Validation
"""Typed request binding with accumulated 422 issues. [FASTAPI-HTTP-VALIDATION]"""
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Results as B

__all__ = ["required", "defaulted", "integer", "choice", "combine", "invalid", "issues_of", "mapped"]


def invalid(location, kind, message, input, context):
    """Produce a typed invalid outcome from one request validation failure."""
    return M.Invalid(issues=[E.issue(location, kind, message, input, False, context)])


def issues_of(result):
    """Return the issue list; successful outcomes have no validation issues."""
    match result:
        case M.Valid():
            return []
        case M.Invalid(issues=errors):
            return errors


def combine(left, right):
    """Combine two typed outcomes and accumulate both issue lists on failure."""
    match left:
        case M.Valid(value=first):
            match right:
                case M.Valid(value=second):
                    return M.Valid(value=M.Pair(first=first, second=second))
                case M.Invalid(issues=errors):
                    return M.Invalid(issues=errors)
        case M.Invalid(issues=errors):
            return M.Invalid(issues=listConcat(errors, issues_of(right)))


def required(location, result):
    """Require a boundary value and retain missing or decoding failures as typed issues."""
    match result:
        case Success(value=text):
            return M.Valid(value=text)
        case Error(message=message):
            return M.Invalid(issues=[E.issue(location, "missing" if message == "Field required" else "value_error", message, "", True, M.NoContext())])


def defaulted(result, fallback):
    """Supply a default only for absence; propagate actual decoding errors."""
    match result:
        case Success(value=text):
            return Success(value=text)
        case Error(message=message):
            return Success(value=fallback) if message == "Field required" else Error(message=message)


def choice(location, result, allowed):
    """Validate a string against the allowed literal values."""
    match required(location, result):
        case M.Invalid(issues=errors):
            return M.Invalid(issues=errors)
        case M.Valid(value=text):
            if listContains(allowed, text):
                return M.Valid(value=text)
            else:
                expected = join(mapList(allowed, lambda item: f"'{item}'"), " or ")
                return invalid(location, "literal_error", f"Input should be {expected}", text, M.TextContext(name="expected", value=expected))


def digit(text):
    return text != "" and contains("0123456789", text)


def separators(text, previous_digit):
    if text == "":
        return True
    else:
        head = take(text, 1)
        tail = drop(text, 1)
        return previous_digit and digit(take(tail, 1)) and separators(tail, False) if head == "_" else separators(tail, digit(head))


def whole_number(text):
    spelling = trim(text)
    return andThen(replace(spelling, "_", ""), decimal_number) if separators(spelling, False) else Error(message="Not an integer")


def decimal_number(text):
    match indexOf(text, "."):
        case Error():
            return parseInt(text)
        case Success(value=index):
            fraction = drop(drop(text, index), 1)
            return andThen(replace(fraction, "0", ""), lambda rest: parseInt(take(text, index)) if fraction != "" and rest == "" else Error(message="Not an integer"))


def bound(location, text, number, minimum, maximum):
    if number < minimum:
        return invalid(location, "greater_than_equal", f"Input should be greater than or equal to {minimum}", text, M.NumberContext(name="ge", value=minimum))
    elif number > maximum:
        return invalid(location, "less_than_equal", f"Input should be less than or equal to {maximum}", text, M.NumberContext(name="le", value=maximum))
    else:
        return M.Valid(value=number)


def integer(location, result, minimum, maximum):
    """Parse an integer and enforce inclusive bounds with standard 422 issue metadata."""
    match required(location, result):
        case M.Invalid(issues=errors):
            return M.Invalid(issues=errors)
        case M.Valid(value=text):
            match whole_number(text):
                case Success(value=number):
                    return bound(location, text, number, minimum, maximum)
                case Error():
                    return invalid(location, "int_parsing", "Input should be a valid integer, unable to parse string as an integer", text, M.NoContext())



def mapped(result, convert):
    """Transform a validated value without dropping accumulated failures."""
    match result:
        case M.Valid(value=value):
            return M.Valid(value=convert(value))
        case M.Invalid(issues=issues):
            return M.Invalid(issues=issues)
