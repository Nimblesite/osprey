# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Models
"""Generated from models.td; run generate_models.py."""

__all__ = ['Request', 'Response', 'Route']


class Request:
    method: str
    url: str
    headers: str
    body: str


class Response:
    status_code: int
    content: str
    media_type: str
    headers: str


class Route[T]:
    method: str
    path: str
    handler: T
