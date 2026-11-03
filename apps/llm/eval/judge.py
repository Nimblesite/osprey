"""Bounded pure-function evaluation for [LLM-BASELINE]; no model or oracle substitution."""
import ast
import json
from pathlib import Path
import subprocess
import sys

BUILTINS = ("abs", "all", "any", "bool", "dict", "enumerate", "filter", "float", "int", "isinstance", "len", "list", "map", "max", "min", "range", "reversed", "round", "set", "sorted", "str", "sum", "tuple", "zip", "chr", "ord")
METHODS = set("get items keys values split rsplit splitlines join lower upper strip lstrip rstrip startswith endswith replace count index append extend pop sort reverse find isalnum isalpha isdigit isspace casefold removeprefix removesuffix add discard update copy setdefault".split())
FORBIDDEN = (ast.Import, ast.ImportFrom, ast.Global, ast.Nonlocal, ast.ClassDef, ast.With, ast.AsyncFunctionDef)


def validate(source):
    tree = ast.parse(source)
    if not any(isinstance(node, ast.FunctionDef) and node.name == "solve" for node in tree.body):
        raise ValueError("Define solve(value).")
    for node in ast.walk(tree):
        if isinstance(node, FORBIDDEN):
            raise ValueError("Only pure functions without imports are accepted.")
        if isinstance(node, ast.Name) and (node.id.startswith("_") or node.id in ("open", "exec", "eval", "compile", "getattr", "setattr", "input", "globals", "locals", "breakpoint")):
            raise ValueError("Disallowed name: " + node.id)
        if isinstance(node, ast.Attribute) and node.attr not in METHODS:
            raise ValueError("Disallowed attribute: " + node.attr)
        if isinstance(node, ast.FunctionDef) and node.decorator_list:
            raise ValueError("Decorators are not accepted.")
    return tree


def same(actual, expected):
    if isinstance(actual, bool) or isinstance(expected, bool):
        return type(actual) is type(expected) and actual == expected
    if isinstance(actual, list) and isinstance(expected, list):
        return len(actual) == len(expected) and all(same(a, b) for a, b in zip(actual, expected))
    if isinstance(actual, dict) and isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(same(actual[k], expected[k]) for k in actual)
    return actual == expected


def check(source, cases, timeout=3):
    try:
        validate(source)
        payload = json.dumps({"source": source, "inputs": [case["input"] for case in cases]})
        with __import__("tempfile").TemporaryDirectory(prefix="osprey-eval-") as directory:
            run = subprocess.run([sys.executable, "-I", str(Path(__file__).resolve()), "--worker"], input=payload, capture_output=True, text=True, cwd=directory, env={}, timeout=timeout)
        if run.returncode:
            return {"passed": False, "error": run.stderr[-2000:] or f"worker exit {run.returncode}"}
        actual = json.loads(run.stdout)
        passed = len(actual) == len(cases) and all(same(a, c["expected"]) for a, c in zip(actual, cases))
        return {"passed": passed, "actual": actual}
    except subprocess.TimeoutExpired:
        return {"passed": False, "error": "candidate timeout"}
    except (SyntaxError, ValueError, OSError) as error:
        return {"passed": False, "error": str(error)}


def worker():
    import builtins
    import resource
    resource.setrlimit(resource.RLIMIT_CPU, (3, 3))
    resource.setrlimit(resource.RLIMIT_FSIZE, (1048576, 1048576))
    payload = json.load(sys.stdin)
    namespace = {"__builtins__": {name: getattr(builtins, name) for name in BUILTINS}}
    exec(compile(validate(payload["source"]), "<candidate>", "exec"), namespace)
    results = [namespace["solve"](value) for value in payload["inputs"]]
    json.dump(results, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    worker()
