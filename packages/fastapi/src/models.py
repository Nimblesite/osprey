# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Models
"""Generated from models.td; run generate_models.py."""

__all__ = ['Request', 'Response', 'Route', 'Decoded', 'Parameter', 'HTTPException', 'ValidationIssue', 'Pair', 'Validation', 'ValidationContext']


class Request:
    """Incoming HTTP request with raw URL/body and decoded path captures."""
    method: str
    url: str
    headers: str
    body: str
    path_params: dict[str, str]


class Response:
    """Outgoing HTTP status, content, media type and headers."""
    status_code: int
    content: str
    media_type: str
    headers: str


class Route[T]:
    """An ordered method/path pattern and its typed handler."""
    method: str
    path: str
    handler: T


class Decoded:
    """One decoded URL fragment and the unconsumed input."""
    value: str
    rest: str


class Parameter:
    """One decoded query name/value pair."""
    name: str
    value: str


class HTTPException:
    """A typed HTTP failure with status, detail and headers."""
    status_code: int
    detail: str
    headers: str


class ValidationIssue:
    """A validation failure with location, original input and typed context."""
    location: list[str]
    kind: str
    message: str
    input: str
    missing: bool
    context: ValidationContext


class Pair[A, B]:
    """Two independently typed validated values."""
    first: A
    second: B


@union
class Validation[T]:
    """A validated value or accumulated request validation issues."""
    class Valid:
        value: T
    class Invalid:
        issues: list[ValidationIssue]


@union
class ValidationContext:
    """Typed optional metadata for a validation issue."""
    class NoContext:
        pass
    class TextContext:
        name: str
        value: str
    class NumberContext:
        name: str
        value: int
