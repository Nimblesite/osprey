# osprey: flavor=python
# osprey: namespace=fastapi_tests
# osprey: module=Fixture
"""Real HTTP service driven by e2e.py. Implements [FASTAPI-HTTP-ROUTING]."""
from fastapi import Responses as R
from fastapi import Routing


def health(request):
    return R.json_response(200, "{\"status\":\"ok\"}", "")


def ready(request):
    return R.json_response(503, "{\"detail\":\"database unavailable\"}", "Cache-Control: no-store")


def echo(request):
    return R.response(201, request.body, "application/octet-stream", "X-Echo: yes")


def routes():
    return [Routing.route("GET", "/health", health), Routing.route("GET", "/readyz", ready), Routing.route("POST", "/echo", echo)]



__all__ = ["routes"]
