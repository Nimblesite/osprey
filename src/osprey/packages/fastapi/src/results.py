# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Results
"""Compose fallible boundary operations without losing errors. [FAILURE-EXPLICIT]"""
__all__ = ["bind", "mapped"]


def bind(result, next):
    """Continue a successful boundary operation; propagate an error unchanged."""
    return resultAndThen(result, next)


def mapped(result, transform):
    """Transform only the successful value of a boundary operation."""
    return resultAndThen(result, lambda item: Success(value=transform(item)))
