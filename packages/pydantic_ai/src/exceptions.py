# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Exceptions
"""pydantic_ai.exceptions as one Result error type.

CPython raises `ModelHTTPError`, `ModelAPIError`, `UserError`,
`UnexpectedModelBehavior` and `UsageLimitExceeded`; the port returns them as
the variants of `ModelError` in a `Result`, with `message_of` spelling the text
`str(exception)` printed. Port of pydantic_ai/exceptions.py (2.46.0).
"""

__all__ = ["ModelError", "Outcome", "message_of", "status_code_of", "http_error", "is_ok", "error_of", "error_message"]


@union
class ModelError:
    """The failures a model request or agent run can end with."""
    class ModelHTTPError:
        """A non-success HTTP status from the model API; `body` is the raw response text."""
        status_code: int
        model_name: str
        body: str
    class ModelAPIError:
        """A transport failure or an unparseable response from the model API."""
        model_name: str
        message: str
    class UserError:
        """The program used the library incorrectly."""
        message: str
    class UnexpectedModelBehavior:
        """The model answered in a shape the run cannot continue from."""
        message: str
        body: str
    class UsageLimitExceeded:
        """A run exceeded its request limit."""
        message: str


@union
class Outcome[T]:
    """What a model request or an agent run ends with: CPython's return value or its raised exception."""
    class Ok:
        """The value CPython returned."""
        value: T
    class Failed:
        """The exception CPython raised."""
        error: ModelError


def is_ok(outcome) -> bool:
    """Whether an outcome is `Ok`."""
    match outcome:
        case Ok():
            return True
        case Failed():
            return False


def error_of(outcome) -> ModelError:
    """The error of a failed outcome; `UserError("ok")` for a success."""
    match outcome:
        case Ok():
            return UserError(message="ok")
        case Failed(error=error):
            return error


def error_message(outcome) -> str:
    """`str(exception)` of a failed outcome, empty for a success."""
    match outcome:
        case Ok():
            return ""
        case Failed(error=error):
            return message_of(error)


def http_error(status_code, model_name, body):
    """`ModelHTTPError(status_code, model_name, body)`."""
    return ModelHTTPError(status_code=status_code, model_name=model_name, body=body)


def message_of(error) -> str:
    """`str(exception)`: the message CPython's exception carries."""
    match error:
        case ModelHTTPError(status_code=status_code, model_name=model_name, body=body):
            return f"status_code: {status_code}, model_name: {model_name}, body: {body}"
        case ModelAPIError(message=message):
            return message
        case UserError(message=message):
            return message
        case UnexpectedModelBehavior(message=message, body=body):
            return message if body == "" else f"{message}, body:\n{body}"
        case UsageLimitExceeded(message=message):
            return message


def status_code_of(error) -> int:
    """`exception.status_code` for a `ModelHTTPError`, 0 for every other variant."""
    match error:
        case ModelHTTPError(status_code=status_code):
            return status_code
        case ModelAPIError():
            return 0
        case UserError():
            return 0
        case UnexpectedModelBehavior():
            return 0
        case UsageLimitExceeded():
            return 0
