# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Results
"""Compose fallible boundary operations without losing errors. [FAILURE-EXPLICIT]"""
__all__ = ["mapped"]


def mapped(result, transform):
    """Transform only the successful value of a boundary operation.

    # Examples
    ```osprey
    from fastapi import Results
    print(f"[{toString(Results.mapped(Success(value='a'), lambda text: text + '!'))}] [{toString(Results.mapped(Error(message='Field required'), lambda text: text + '!'))}]")
    ```
    ```output
    [Success(a!)] [Error(Field required)]
    ```
    """
    return andThen(result, lambda item: Success(value=transform(item)))
