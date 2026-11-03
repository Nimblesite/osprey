"""Python-hosted native Osprey libraries. Implements [PYTHON-HOST-ABI]."""

import ctypes
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading

from ._bridge import Bridge, CALLBACK, OBJECT_CALLBACK

__all__ = ["build", "load"]
_LIBRARIES = {}
_FILES = {}
_LOADING = set()
_CTYPES = {"int": ctypes.c_int64, "float": ctypes.c_double,
           "int8": ctypes.c_int8, "int16": ctypes.c_int16, "int32": ctypes.c_int32,
           "uint8": ctypes.c_uint8, "uint16": ctypes.c_uint16, "uint32": ctypes.c_uint32,
           "float16": ctypes.c_uint16, "float32": ctypes.c_float,
           "bool": ctypes.c_bool, "string": ctypes.c_char_p, "Unit": None}


def _argument(value, kind):
    if kind == "string":
        if not isinstance(value, str):
            raise TypeError("Osprey string arguments require str")
        if "\0" in value:
            raise ValueError("Osprey C ABI strings cannot contain NUL")
        return value.encode("utf-8")
    expected = bool if kind == "bool" else float if kind in ("float", "float32") else int
    if type(value) is not expected:
        raise TypeError(f"Osprey {kind} arguments require {expected.__name__}")
    if expected is int:
        bits = ctypes.sizeof(_CTYPES[kind]) * 8
        signed = kind.startswith("int")
        low, high = (-(2**(bits - 1)), 2**(bits - 1)) if signed else (0, 2**bits)
        if not low <= value < high:
            raise OverflowError(f"Osprey {kind} arguments must fit {'signed' if signed else 'unsigned'} {bits} bits")
    if kind == "float32" and math.isfinite(value) and not math.isfinite(ctypes.c_float(value).value):
        raise OverflowError("Osprey float32 arguments must fit binary32")
    return value


def _function(library, descriptor):
    function = getattr(library._native, descriptor["symbol"])
    kinds = descriptor["parameters"]
    function.argtypes = [_CTYPES[kind] for kind in kinds]
    function.restype = _CTYPES[descriptor["returns"]]
    def call(*arguments, **keywords):
        library._check_thread()
        arguments = _bind(descriptor, arguments, keywords)
        converted = [_argument(value, kind) for value, kind in zip(arguments, kinds)]
        result = function(*converted)
        if descriptor["returns"] == "string" and result is None:
            raise RuntimeError("Osprey returned a null string")
        return result.decode("utf-8") if descriptor["returns"] == "string" else result
    call.__name__ = descriptor["name"]
    call.__doc__ = _signature(descriptor)
    return call


def _signature(descriptor):
    parameters = ", ".join(f"{name}: {kind}" for name, kind in
                           zip(descriptor["parameter_names"], descriptor["parameters"]))
    return f'{descriptor["name"]}({parameters}) -> {descriptor["returns"]}'


def _bind(descriptor, arguments, keywords):
    names = descriptor["parameter_names"]
    if len(arguments) > len(names):
        raise TypeError(f'{descriptor["name"]} expects {len(names)} arguments')
    assigned = dict(zip(names, arguments))
    for name, value in keywords.items():
        if name not in names or name in assigned:
            raise TypeError(f'Unexpected or duplicate argument {name!r} for {descriptor["name"]}')
        assigned[name] = value
    if len(assigned) != len(names):
        raise TypeError(f'{descriptor["name"]} expects {len(names)} arguments')
    return [assigned[name] for name in names]


class Library:
    """A loaded library, its rooted callbacks and Python object registry."""

    def __init__(self, path):
        self._thread = threading.current_thread()
        self._native = ctypes.PyDLL(str(path))
        self._bridge = Bridge()
        self._callback = CALLBACK(self._bridge.call)
        self._object_callback = OBJECT_CALLBACK(self._bridge.object)
        install = self._native.osprey_python_install
        install.argtypes, install.restype = [CALLBACK, OBJECT_CALLBACK], None
        install(self._callback, self._object_callback)
        self._functions = self._read_exports()
        entry = self._native.osprey_main
        entry.argtypes, entry.restype = [], ctypes.c_int32
        status = entry()
        if status:
            raise RuntimeError(f"Osprey initialization failed with status {status}")

    def _check_thread(self):
        if threading.current_thread() is not self._thread:
            raise RuntimeError("Osprey libraries must be called from their loading thread")

    def _read_exports(self):
        abi = self._native.osprey_python_abi
        abi.argtypes, abi.restype = [], ctypes.c_char_p
        metadata = json.loads(abi().decode("utf-8"))
        if metadata["version"] != 1:
            raise ValueError("Unsupported Osprey Python ABI version")
        return {row["name"]: _function(self, row) for row in metadata["exports"]}

    def __getattr__(self, name):
        try:
            return self._functions[name]
        except KeyError:
            raise AttributeError(name) from None

    def __dir__(self):
        return sorted(set(super().__dir__()) | set(self._functions))

    def call(self, name, /, *arguments, **keywords):
        """Call an export, including a qualified project name such as app::sum."""
        return self._functions[name](*arguments, **keywords)

    def keep(self, value):
        """Root a Python object, including a tensor, and return an opaque int handle."""
        self._check_thread()
        return self._bridge.objects.keep(value)

    def resolve(self, handle):
        """Read a live object handle without copying or converting its payload."""
        self._check_thread()
        return self._bridge.objects.resolve(handle)

    def release(self, handle):
        """Release one strong object reference; stale handles raise KeyError."""
        self._check_thread()
        self._bridge.objects.release(handle)


def load(path):
    """Load an existing --target=python library and run its entry once."""
    path = Path(path).resolve(strict=True)
    image = _identity(path)
    if path in _FILES and _FILES[path] != image:
        raise RuntimeError("A loaded library was replaced; use a new output path")
    library = _load_image(path, image)
    library._check_thread()
    _FILES[path] = image
    return library


def _identity(path):
    status = path.stat()
    return status.st_dev, status.st_ino


def _load_image(path, image):
    if image in _LOADING:
        raise RuntimeError("Reentrant loading of an initializing Osprey library")
    if image not in _LIBRARIES:
        _LOADING.add(image)
        try:
            _LIBRARIES[image] = Library(path)
        finally:
            _LOADING.remove(image)
    return _LIBRARIES[image]


def build(source, output=None, *, compiler=None):
    """Compile a file or project and load it; requires an installed compiler."""
    temporary = tempfile.TemporaryDirectory(prefix="osprey-python-") if output is None else None
    suffix = ".dylib" if sys.platform == "darwin" else ".so"
    path = Path(output or Path(temporary.name) / ("library" + suffix)).resolve()
    if path in _FILES or (path.exists() and _identity(path) in _LIBRARIES):
        raise ValueError("Cannot overwrite a loaded Osprey library; choose a new output path")
    result = subprocess.run([compiler or os.environ.get("OSPREY", "osprey"), str(source),
                             "--compile", "--target=python", "-o", str(path)],
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr or result.stdout)
    library = load(path)
    library._temporary = temporary
    return library
