# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Models
"""Generated from models.td; run generate_models.cpython."""

__all__ = ['Request', 'Response', 'Route', 'Parameter', 'HTTPException', 'ValidationIssue', 'Pair', 'Need', 'Dependency', 'Application', 'CorsPolicy', 'Validation', 'ValidationContext', 'DependencyResult', 'Optional', 'LocationPart']


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
    failure: Optional[HTTPException]


class Route[T]:
    """An ordered method/path pattern and its typed handler."""
    method: str
    path: str
    handler: T


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
    location: list[LocationPart]
    kind: str
    message: str
    input: str
    missing: bool
    json_input: bool
    context: ValidationContext


class Pair[A, B]:
    """Two independently typed validated values."""
    first: A
    second: B


class Need:
    """One dependency injection edge and its cache policy."""
    name: str
    use_cache: bool


class Dependency[P, C]:
    """A named provider, ordered children and explicit resource cleanup."""
    name: str
    inputs: list[Need]
    provide: P
    close: C


class Application[R, W, H]:
    """Ordered routes and request middleware."""
    routes: R
    middleware: W
    exception_handlers: dict[str, H]


class CorsPolicy:
    """Explicit browser origin, method, header and credential policy."""
    origins: list[str]
    methods: list[str]
    headers: list[str]
    expose: list[str]
    credentials: bool
    max_age: int


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


@union
class DependencyResult[T]:
    """Provided value, HTTP rejection or accumulated validation errors."""
    class Provided:
        value: T
    class Rejected:
        error: HTTPException
    class DependencyInvalid:
        issues: list[ValidationIssue]


@union
class Optional[T]:
    """A present typed body value or an explicitly absent nullable value."""
    class Present:
        value: T
    class Absent:
        pass


@union
class LocationPart:
    """A field name or numeric array index in a validation location."""
    class Field:
        name: str
    class Index:
        value: int
