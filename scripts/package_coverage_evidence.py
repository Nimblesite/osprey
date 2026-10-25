"""Bind coverage reports to the source and suite whose golden output passed."""
import hashlib
import json
import tomllib
from pathlib import Path

SOURCE_SUFFIXES = frozenset({".py", ".osp", ".ospml"})


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
                     and path.suffix in SOURCE_SUFFIXES
                     and not any(part.startswith(".") for part in path.relative_to(base).parts))
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
