# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Results
"""Compose fallible boundary operations without losing errors. [FAILURE-EXPLICIT]"""
__all__ = ["mapped"]


def mapped(result, transform):
    """Transform only the successful value of a boundary operation."""
    return andThen(result, lambda item: Success(value=transform(item)))
