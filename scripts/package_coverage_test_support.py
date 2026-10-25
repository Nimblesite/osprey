"""Throwaway checkout and compiler fixtures for package coverage contracts."""
import json
import os
import subprocess
import sys
from pathlib import Path
from package_coverage_evidence import seal_evidence, source_inputs

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts" / "package_coverage.py"
FOOTER = "# coverage total: 90.0% (9/10 lines)\n# suites: 1 passed, 0 failed\n"
GOLDEN = "ok demo\n1 passed, 0 failed\n"
FAKE_COMPILER = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
report = Path(args[args.index("--coverage-json") + 1])
suite = Path(args[args.index("test") + 1])
spec = json.loads(Path(os.environ["FAKE_PACKAGE_SPEC"]).read_text()).get(suite.parent.name, {})
if spec.get("report") is not None:
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(spec["report"]))
stdout = spec.get("stdout")
if stdout is None:
    stdout = (suite / "expected_output.txt").read_text() + spec.get("footer", FOOTER)
sys.stdout.write(stdout)
sys.stderr.write(spec.get("stderr", ""))
sys.exit(spec.get("exit", 0))
'''.replace("FOOTER", json.dumps(FOOTER))


def py_source(line_count):
    steps = [f"    step{i} = value + {i}" for i in range(line_count - 2)]
    return "\n".join(["def entry(value):", *steps, "    return value"]) + "\n"


def file_report(path, covered, total):
    lines = {str(line): 1 if line <= covered else 0 for line in range(1, total + 1)}
    return {"files": {path: {"lines": lines}}}


def own_report(name, covered, total, relative="src/lib.py"):
    return file_report(f"packages/{name}/{relative}", covered, total)


def write_json(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload) + "\n")


class Fixture:
    def __init__(self, root, compiler):
        self.root = root
        self.compiler = compiler
        self.reports = root / "target" / "coverage" / "packages"

    def run(self, *args):
        env = {**os.environ, "FAKE_PACKAGE_SPEC": str(self.root / "spec.json")}
        return subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(self.root), *args],
            capture_output=True, text=True, env=env,
        )

    def collect(self, *extra):
        return self.run(
            "--collect", "--compiler", str(self.compiler),
            "--reports", str(self.reports),
            "--thresholds", str(self.root / "coverage-thresholds.json"), *extra,
        )

    def check(self, *extra):
        return self.run(
            "--reports", str(self.reports),
            "--thresholds", str(self.root / "coverage-thresholds.json"), *extra,
        )

    def write_report(self, name, report):
        path = self.reports / f"{name}.json"
        write_json(path, report)
        seal_evidence(self.root, name, path, source_inputs(self.root, name))

    def write_spec(self, spec):
        write_json(self.root / "spec.json", spec)

    def summary(self):
        return json.loads((self.reports / "summary.json").read_text())

    def config(self):
        return json.loads((self.root / "coverage-thresholds.json").read_text())

    def result(self, name):
        return next(entry for entry in self.summary()["packages"] if entry["name"] == name)


def write_gates(root, entries):
    gates = {name: {"threshold": value, "language": "osprey"} for name, value in entries.items()}
    write_json(root / "coverage-thresholds.json", {"projects": {"packages": gates}})


def bad_report_cases():
    good = "packages/bad/src/lib.py"
    return {
        "no-files-object": {},
        "no-own-source": {"files": {"packages/bad/tests/main.py": {"lines": {"1": 1}}}},
        "line-zero": {"files": {good: {"lines": {"0": 1}}}},
        "non-integer-line": {"files": {good: {"lines": {"two": 1}}}},
        "line-past-end": {"files": {good: {"lines": {"6": 1}}}},
        "negative-count": {"files": {good: {"lines": {"1": -1}}}},
        "non-integer-count": {"files": {good: {"lines": {"1": "3"}}}},
        "empty-line-table": {"files": {good: {"lines": {}}}},
        "missing-source-file": {"files": {"packages/bad/src/ghost.py": {"lines": {"1": 1}}}},
    }


def add_package(root, name):
    package = root / "packages" / name
    (package / "tests").mkdir(parents=True, exist_ok=True)
    (package / "osprey.toml").write_text(f'[project]\nname = "{name}"\n')
    (package / "tests" / "osprey.toml").write_text(f'[project]\nname = "{name}-tests"\n')


def add_sources(root, name, sources, expected=GOLDEN):
    package = root / "packages" / name
    (package / "tests" / "expected_output.txt").write_text(expected)
    for relative, text in sources.items():
        path = package / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def make_fixture(base, packages, thresholds=None):
    root = Path(base)
    write_gates(root, thresholds if thresholds is not None else {name: 90 for name in packages})
    for name, spec in packages.items():
        add_package(root, name)
        add_sources(root, name, spec.get("sources", {}), spec.get("expected", GOLDEN))
    compiler = root / "fake-compiler.py"
    compiler.write_text(FAKE_COMPILER)
    compiler.chmod(0o755)
    return Fixture(root, compiler)


