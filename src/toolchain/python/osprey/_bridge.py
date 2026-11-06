"""Keep callback buffers rooted until the C bridge has copied them. [PYTHON-CALL]"""

import ctypes
import importlib
import json
from ._objects import Objects
from ._json import decode

CALLBACK = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p)
OBJECT_CALLBACK = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int64,
                                  ctypes.c_char_p, ctypes.c_char_p)


def _invoke(module, name, args):
    arguments = decode(args)
    if not isinstance(arguments, list):
        raise TypeError("Python call arguments must be a JSON array")
    function = importlib.import_module(module.decode("utf-8"))
    for part in name.decode("utf-8").split("."):
        function = getattr(function, part)
    return function(*arguments)


class Bridge:
    """One callback and its current response, owned by a loaded library."""

    def __init__(self):
        self.response = None
        self.objects = Objects()
        self.fallback = ctypes.create_string_buffer(
            b'{"error":{"type":"RuntimeError","message":"Python callback failed to encode its reply"}}')

    def call(self, module, name, args):
        return self.respond(lambda: _invoke(module, name, args))

    def object(self, operation, handle, name, args):
        return self.respond(lambda: self.objects.invoke(operation, handle, name, args))

    def respond(self, work):
        try:
            return self.encode(work)
        except BaseException:
            return ctypes.addressof(self.fallback)

    def encode(self, work):
        try:
            result = json.dumps({"ok": work()}, allow_nan=False)
        except BaseException as error:
            result = json.dumps({"error": {"type": type(error).__name__, "message": str(error)}})
        self.response = ctypes.create_string_buffer(result.encode("utf-8"))
        return ctypes.addressof(self.response)
