"""Bind coverage reports to the source and suite whose golden output passed."""
import ast
import hashlib
import json
import re
import tomllib
from pathlib import Path

SOURCE_SUFFIXES = frozenset({".py", ".osp", ".ospml"})
GENERATED_DIRS = frozenset({"target", "__pycache__", "node_modules"})


def python_coverable(text):
    try:
        nodes = ast.walk(ast.parse(text))
        return any(isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda,
                                    ast.Call, ast.Await, ast.Raise, ast.Assert)) for node in nodes)
    except SyntaxError:
        return True


def golden_problems(path):
    try:
        text = path.read_bytes().decode("utf-8")
        summary = re.search(r"^([1-9][0-9]*) passed, 0 failed\n?\Z", text, re.MULTILINE)
        tap = re.search(r"^# tests=([1-9][0-9]*) passed=([1-9][0-9]*) failed=0 skipped=0\n?\Z", text, re.MULTILINE)
        count = len(re.findall(r"^ok ", text, re.MULTILINE))
        passed = int(summary[1]) if summary else int(tap[1]) if tap and tap[1] == tap[2] else 0
        bad_rows = re.search(r"^(?:FAIL(?: |$)|not ok(?: |$))", text, re.MULTILINE)
        return [] if passed > 0 and passed == count and not bad_rows else ["suite golden must prove positive passing assertions and zero failures"]
    except (OSError, UnicodeError):
        return ["cannot read suite golden"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_inputs(root, name):
    root = Path(root).resolve()
    package = root / "packages" / name
    suite = package / "tests"
    manifest = tomllib.loads((suite / "osprey.toml").read_text())
    roots = [package / "src", suite]
    roots += [(suite / path).resolve() for path in manifest.get("project", {}).get("source_roots", [])]
    files = {package / "osprey.toml", suite / "osprey.toml", suite / "expected_output.txt"}
    for base in roots:
        files.update(path.resolve() for path in base.rglob("*") if path.is_file()
                     and not any(part.startswith(".") or part in GENERATED_DIRS
                                 for part in path.relative_to(base).parts))
    return {path.relative_to(root).as_posix(): digest(path) for path in sorted(files)}


def evidence_path(report):
    return report.with_suffix(".evidence.json")


def seal_evidence(root, name, report, before):
    if source_inputs(root, name) != before:
        raise ValueError("source or suite changed during coverage collection")
    evidence = {"package": name, "inputs": before, "report_sha256": digest(report)}
    evidence_path(report).write_text(json.dumps(evidence, sort_keys=True) + "\n")


def evidence_problems(root, name, report):
    try:
        evidence = json.loads(evidence_path(report).read_text())
        valid = (evidence.get("package") == name and evidence.get("inputs") == source_inputs(root, name)
                 and evidence.get("report_sha256") == digest(report))
        return [] if valid else ["stale or altered coverage evidence; recollect this package"]
    except (OSError, ValueError, AttributeError):
        return ["missing verified collection evidence; recollect this package"]
