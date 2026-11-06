# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Chain
"""Result plumbing shared by this package's modules: continue a parse with the value
of a `Success`, and take the first element of a selection or the error naming it.
"""

__all__ = ["then", "first_or_error"]


def then(result, continue_with):
    """`continue_with` applied to `result`'s value, or `result`'s error unchanged."""
    match result:
        case Success(value=value):
            return continue_with(value)
        case Error(message=message):
            return Error(message=message)


def first_or_error(found, failure):
    """The first element of `found`, or an Error carrying `failure` when it is empty."""
    match listGet(found, 0):
        case Success(value=value):
            return Success(value=value)
        case Error(message=_problem):
            return Error(message=failure)
