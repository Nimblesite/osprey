# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Results
"""Compose fallible boundary operations without losing errors. [FAILURE-EXPLICIT]"""
__all__ = ["bind", "mapped"]


def bind(result, next):
    """Continue a successful boundary operation; propagate an error unchanged."""
    match result:
        case Success(value=item):
            return next(item)
        case Error(message=message):
            return Error(message=message)


def mapped(result, transform):
    """Transform only the successful value of a boundary operation."""
    return bind(result, lambda item: Success(value=transform(item)))
