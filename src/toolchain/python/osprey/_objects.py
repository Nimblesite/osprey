"""Strong library-local handles; payloads never enter the JSON codec. [PYTHON-OBJECT]"""

import importlib
import itertools
from ._json import decode

_IDS = itertools.count(1)


class Objects:
    """References remain live until explicitly released by either language."""

    def __init__(self):
        self.values = {}

    def keep(self, value):
        handle = next(_IDS)
        self.values[handle] = value
        return handle

    def resolve(self, handle):
        if type(handle) is not int:
            raise TypeError("Python object handles must be integers")
        return self.values[handle]

    def release(self, handle):
        self.resolve(handle)
        del self.values[handle]

    def argument(self, token):
        if not isinstance(token, dict) or len(token) != 1:
            raise TypeError("Object arguments require exactly one of value or ref")
        if "ref" in token:
            return self.resolve(token["ref"])
        if "value" in token:
            return token["value"]
        raise TypeError("Object arguments require value or ref")

    def call(self, value, name, args):
        request = decode(args)
        _validate_request(request)
        positional = [self.argument(token) for token in request.get("args", [])]
        keywords = {key: self.argument(token) for key, token in request.get("kwargs", {}).items()}
        function = getattr(value, name) if name else value
        return function(*positional, **keywords)

    def invoke(self, operation, handle, name, args):
        operation, name = operation.decode("utf-8"), name.decode("utf-8")
        if operation == "import":
            return self.keep(importlib.import_module(name))
        value = self.resolve(handle)
        if operation == "getattr":
            return self.keep(getattr(value, name))
        if operation in {"call", "call_value"}:
            result = self.call(value, name, args)
            return self.keep(result) if operation == "call" else result
        if operation == "value":
            return value
        if operation == "release":
            return self.release(handle)
        raise ValueError(f"Unknown Python object operation: {operation}")


def _validate_request(request):
    if not isinstance(request, dict) or set(request) - {"args", "kwargs"}:
        raise TypeError("Object calls require an object containing args and/or kwargs")
    if not isinstance(request.get("args", []), list):
        raise TypeError("Object positional arguments must be an array")
    if not isinstance(request.get("kwargs", {}), dict):
        raise TypeError("Object keyword arguments must be an object")
