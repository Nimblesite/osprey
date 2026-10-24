"""Runs one CPython oracle: `python -I scripts/oracles/oracle.py <name>`.

An oracle is named after the package it checks (`numpy.py`, `torch.py`, `re.py`), so
running it as a script would put this directory first on `sys.path` and make its own
`import numpy` find itself. This runner keeps the directory off the path, registers the
shared writer under its module name, loads `<name>.py` from its file and calls `main()`.
"""
import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent


def load(name, path):
    """Import the module at `path` as `name` without touching `sys.path`."""
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def main(argv):
    if len(argv) != 2:
        raise SystemExit("usage: oracle.py <package>")
    sys.path = [entry for entry in sys.path if pathlib.Path(entry or ".").resolve() != HERE]
    load("expected_module", HERE / "expected_module.py")
    load(f"oracle_{argv[1]}", HERE / f"{argv[1]}.py").main()


if __name__ == "__main__":
    main(sys.argv)
