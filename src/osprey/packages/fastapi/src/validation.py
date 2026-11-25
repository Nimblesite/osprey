# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Validation
"""Typed request binding with accumulated 422 issues. [FASTAPI-HTTP-VALIDATION]"""
from fastapi import Models as M
from fastapi import Errors as E
from fastapi import Results as B

__all__ = ["required", "defaulted", "integer", "choice", "combine", "invalid", "issues_of", "mapped"]


def invalid(location, kind, message, input, context):
    """Produce a typed invalid outcome from one request validation failure.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def shown():
        outcome = Validation.invalid(["query", "n"], "greater_than_equal", "Input should be greater than or equal to 5", "2", M.NumberContext(name="ge", value=5))
        match outcome:
            case M.Valid():
                return "valid"
            case M.Invalid(issues=errors):
                match listGet(errors, 0):
                    case Success(value=issue):
                        match issue.context:
                            case M.NumberContext(name=_name, value=limit):
                                return f"[{len(errors)}] [{issue.kind}] [{limit}]"
                            case M.TextContext(name=_name, value=expected):
                                return f"[{len(errors)}] [{issue.kind}] [{expected}]"
                            case M.NoContext():
                                return f"[{len(errors)}] [{issue.kind}]"
                    case Error():
                        return "none"

    print(shown())
    ```
    ```output
    [1] [greater_than_equal] [5]
    ```
    """
    return M.Invalid(issues=[E.issue(location, kind, message, input, False, context)])


def issues_of(result):
    """Return the issue list; successful outcomes have no validation issues.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M
    bad = Validation.invalid(["query", "n"], "int_parsing", "Input should be a valid integer, unable to parse string as an integer", "x", M.NoContext())
    print(f"[{len(Validation.issues_of(bad))}] [{len(Validation.issues_of(M.Valid(value='ok')))}]")
    ```
    ```output
    [1] [0]
    ```
    """
    match result:
        case M.Valid():
            return []
        case M.Invalid(issues=errors):
            return errors


def combine(left, right):
    """Combine two typed outcomes and accumulate both issue lists on failure.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def verdict(left, right):
        match Validation.combine(left, right):
            case M.Valid(value=pair):
                return f"valid({pair.first}, {pair.second})"
            case M.Invalid(issues=errors):
                return f"invalid({len(errors)})"

    bad = Validation.invalid(["query", "n"], "string_type", "Input should be a valid string", "7", M.NoContext())
    print(f"[{verdict(M.Valid(value='a'), M.Valid(value='b'))}] [{verdict(bad, bad)}]")
    ```
    ```output
    [valid(a, b)] [invalid(2)]
    ```
    """
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
    """Require a boundary value and retain missing or decoding failures as typed issues.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def kind(result):
        match Validation.required(["query", "q"], result):
            case M.Valid(value=value):
                return f"valid: {value}"
            case M.Invalid(issues=errors):
                match listGet(errors, 0):
                    case Success(value=issue):
                        return f"{issue.kind}: {issue.message}"
                    case Error():
                        return "none"

    print(f"[{kind(Success(value='hi'))}] [{kind(Error(message='Field required'))}] [{kind(Error(message='bad escape'))}]")
    ```
    ```output
    [valid: hi] [missing: Field required] [value_error: bad escape]
    ```
    """
    match result:
        case Success(value=text):
            return M.Valid(value=text)
        case Error(message=message):
            return M.Invalid(issues=[E.issue(location, "missing" if message == "Field required" else "value_error", message, "", True, M.NoContext())])


def defaulted(result, fallback):
    """Supply a default only for absence; propagate actual decoding errors.

    # Examples
    ```osprey
    from fastapi import Validation
    print(f"[{toString(Validation.defaulted(Error(message='Field required'), '10'))}] [{toString(Validation.defaulted(Success(value='7'), '0'))}] [{toString(Validation.defaulted(Error(message='bad escape'), '0'))}]")
    ```
    ```output
    [Success(10)] [Success(7)] [Error(bad escape)]
    ```
    """
    match result:
        case Success(value=text):
            return Success(value=text)
        case Error(message=message):
            return Success(value=fallback) if message == "Field required" else Error(message=message)


def choice(location, result, allowed):
    """Validate a string against the allowed literal values.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def kind(result):
        match Validation.choice(["query", "sort"], result, ["asc", "desc"]):
            case M.Valid(value=value):
                return f"valid: {value}"
            case M.Invalid(issues=errors):
                match listGet(errors, 0):
                    case Success(value=issue):
                        match issue.context:
                            case M.TextContext(name=_name, value=expected):
                                return f"{issue.kind} (ctx {expected})"
                            case M.NumberContext(name=_name, value=_limit):
                                return f"{issue.kind}"
                            case M.NoContext():
                                return f"{issue.kind}"
                    case Error():
                        return "none"

    print(f"[{kind(Success(value='asc'))}] [{kind(Success(value='up'))}]")
    ```
    ```output
    [valid: asc] [literal_error (ctx 'asc' or 'desc')]
    ```
    """
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
    """Parse an integer and enforce inclusive bounds with standard 422 issue metadata.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def kind(result):
        match Validation.integer(["query", "n"], result, 1, 100):
            case M.Valid(value=number):
                return f"valid: {number}"
            case M.Invalid(issues=errors):
                match listGet(errors, 0):
                    case Success(value=issue):
                        return f"{issue.kind}: {issue.message}"
                    case Error():
                        return "none"

    print(f"[{kind(Success(value='10'))}] [{kind(Success(value='1_0'))}] [{kind(Success(value='200'))}] [{kind(Success(value='x'))}]")
    ```
    ```output
    [valid: 10] [valid: 10] [less_than_equal: Input should be less than or equal to 100] [int_parsing: Input should be a valid integer, unable to parse string as an integer]
    ```
    """
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
    """Transform a validated value without dropping accumulated failures.

    # Examples
    ```osprey
    from fastapi import Validation
    from fastapi import Models as M

    def verdict(outcome):
        match outcome:
            case M.Valid(value=value):
                return f"valid({value})"
            case M.Invalid(issues=errors):
                return f"invalid({len(errors)})"

    def shown():
        bad = Validation.invalid(["query", "n"], "int_parsing", "Input should be a valid integer, unable to parse string as an integer", "x", M.NoContext())
        upper = lambda text: toUpperCase(text)
        return f"[{verdict(Validation.mapped(M.Valid(value='os'), upper))}] [{verdict(Validation.mapped(bad, upper))}]"

    print(shown())
    ```
    ```output
    [valid(OS)] [invalid(1)]
    ```
    """
    match result:
        case M.Valid(value=value):
            return M.Valid(value=convert(value))
        case M.Invalid(issues=issues):
            return M.Invalid(issues=issues)
