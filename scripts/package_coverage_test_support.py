"""Throwaway checkout and compiler fixtures for package coverage contracts."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from package_coverage_evidence import seal_evidence, source_inputs
from package_coverage_projects import APPS_DIR, PACKAGES_DIR, report_name

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts" / "package_coverage.py"
FOOTER = "# coverage total: 90.0% (9/10 lines)\n# suites: 1 passed, 0 failed\n"
GOLDEN = "ok demo\n1 passed, 0 failed\n"
FAKE_COMPILER = '''#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
args = sys.argv[1:]
report = Path(args[args.index("--coverage-json") + 1])
suite = Path(args[args.index("test") + 1])
spec = json.loads(Path(os.environ["FAKE_PACKAGE_SPEC"]).read_text()).get(suite.parent.name, {})
assert "--memory=" + spec.get("memory", "gc") in args, "collector must explicitly select GC or ARC"
if spec.get("barrier"):
    started = Path(os.environ["FAKE_PACKAGE_SPEC"]).parent / "started"
    started.mkdir(exist_ok=True)
    (started / suite.parent.name).touch()
    deadline = time.monotonic() + 5
    while not all((started / name).exists() for name in spec["barrier"]):
        if (started / "failed").exists() or time.monotonic() >= deadline:
            (started / "failed").touch()
            raise RuntimeError("package suites did not overlap")
        time.sleep(0.01)
if spec.get("mutate"):
    target = Path(os.environ["FAKE_PACKAGE_SPEC"]).parent / spec["mutate"]
    target.write_text("mutated during collection\\n")
if spec.get("report") is not None:
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(spec["report"]))
if spec.get("raw_stdout") is not None:
    sys.stdout.buffer.write(bytes(spec["raw_stdout"]))
    sys.stdout.flush()
stdout = spec.get("stdout")
if stdout is None:
    stdout = spec.get("raw_stdout") and "" or (suite / "expected_output.txt").read_text() + spec.get("footer", FOOTER)
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


def own_report(name, covered, total, relative="src/lib.py", kind=PACKAGES_DIR):
    return file_report(f"{kind}/{name}/{relative}", covered, total)


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

    def write_report(self, name, report, kind=PACKAGES_DIR):
        path = self.reports / f"{report_name(kind, name)}.json"
        write_json(path, report)
        seal_evidence(self.root, name, path, source_inputs(self.root, name, kind), kind)

    def write_spec(self, spec):
        write_json(self.root / "spec.json", spec)

    def summary(self):
        return json.loads((self.reports / "summary.json").read_text())

    def config(self):
        return json.loads((self.root / "coverage-thresholds.json").read_text())

    def measurements(self):
        return json.loads((self.reports / "measurements.json").read_text())

    def result(self, name):
        return next(entry for entry in self.summary()["packages"] if entry["name"] == name)

    def collect_files(self):
        """The own-file rows of a collected report, in report order."""
        report = json.loads((self.reports / "apps-workbench.json").read_text())
        return [body for key, body in report["files"].items() if key.endswith("src/lib.py")]

    def merged_lines(self):
        """Every merged own file's counters, keyed by line number, per raw key."""
        report = json.loads((self.reports / "apps-workbench.json").read_text())
        return {key: {int(line): count for line, count in body["lines"].items()}
                for key, body in report["files"].items() if key.endswith("src/lib.py")}


def write_gates(root, entries, apps=None):
    gates = {name: {"threshold": value, "language": "osprey"} for name, value in entries.items()}
    projects = {"packages": gates}
    if apps is not None:
        projects["apps"] = apps
    write_json(root / "coverage-thresholds.json", {"projects": projects})


def c_runtime_gate_cases():
    return [
        (89, {}, 2, "c"), (90, {}, 0, "c"),
        (94, {"threshold": 95}, 2, "c"), (95, {"threshold": 95}, 0, "c"),
        (94, {"threshold": 95}, 2, "h"), (95, {"threshold": 95}, 0, "h"),
        (None, {"threshold": 95}, 2, "h"),
    ]


def check_c_runtime(root, percentage, entry, suffix="c"):
    write_json(root / "coverage-thresholds.json", {"projects": {"c_runtime": {"probe": entry}}})
    summary = root / "compiler/bin/cov/probe/summary.txt"
    summary.parent.mkdir(parents=True)
    summary.write_text("" if percentage is None else f"File '../../../runtime/probe.{suffix}'\nLines executed:{percentage}% of 100\n")
    source = (ROOT / "scripts/make/tests.mk").read_text()
    recipe = source.split("_coverage_check_c_runtime:\n", 1)[1].split("\n\n", 1)[0]
    gate = recipe[recipe.index("\t@command -v jq"):]
    makefile = f"COVERAGE_THRESHOLDS_FILE := coverage-thresholds.json\ncheck:\n{gate}\n"
    return subprocess.run(["make", "-s", "-f", "-", "check"], input=makefile, cwd=root, capture_output=True, text=True)


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


def add_package(root, name, kind=PACKAGES_DIR):
    project = root / kind / name
    (project / "tests").mkdir(parents=True, exist_ok=True)
    (project / "osprey.toml").write_text(f'[project]\nname = "{name}"\n')
    (project / "tests" / "osprey.toml").write_text(f'[project]\nname = "{name}-tests"\n')


def add_sources(root, name, sources, expected=GOLDEN, kind=PACKAGES_DIR):
    project = root / kind / name
    (project / "tests" / "expected_output.txt").write_text(expected)
    for relative, text in sources.items():
        path = project / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def add_project_files(root, kind, name, spec):
    add_package(root, name, kind)
    add_sources(root, name, spec.get("sources", {}), spec.get("expected", GOLDEN), kind)


ENTRY_ORACLE = '{"error":"vscode.frame.unknown-operation"}\n'
ENTRY_FOOTER = "# coverage total: 100.0% (2/2 lines)\n# suites: 1 passed, 0 failed\n"
ENTRY_MANIFEST = '[project]\nname = "workbench"\nentry = "src/lib.py"\n'


def entry_report(covered, total=10, key="apps/workbench/src/lib.py"):
    lines = {str(line): 1 if line <= covered else 0 for line in range(1, total + 1)}
    return {"files": {key: {"lines": lines}}}


def masked_report(hits, key="apps/workbench/src/lib.py"):
    return {"files": {key: {"lines": {str(line): hits.get(line, 0) for line in range(1, 11)}}}}


def suite_manifest(*roots):
    """A suite manifest naming `roots`; the array is JSON, which is also valid TOML.

    Hand quoting left a Windows `C:\\...` root unescaped, so the fixture was
    invalid TOML and a test could pass for that accident rather than its cause.
    """
    listed = json.dumps(list(roots), ensure_ascii=False)
    return "\n".join(["[" + "project]", 'name = "workbench-tests"', f"source_roots = {listed}", ""])


def app_spec(primary=None, entry=None):
    """The fake-compiler spec for one app fixture: its suite, and its entry probe."""
    if primary is None:
        suite = {"report": entry_report(9, 10)}
    else:
        suite = {"report": primary} if "files" in primary else dict(primary)
    return {"workbench": suite, "apps": {} if entry is None else entry}


class CoverageTestCase(unittest.TestCase):
    """Temporary-checkout fixture and coverage assertions shared by both suites."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.base = Path(directory.name)

    def fixture(self, packages, thresholds=None, apps=None):
        return make_fixture(tempfile.mkdtemp(dir=self.base), packages, thresholds, apps)

    def workbench(self, primary=None, entry=None, oracle=ENTRY_ORACLE, manifest=ENTRY_MANIFEST,
                  suite_manifest=None):
        """The one apps/workbench fixture every entry-probe regression starts from."""
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(10)}}},
                               apps={"workbench": {"sources": {"src/lib.py": py_source(10)}}})
        project = fixture.root / "apps" / "workbench"
        (project / "osprey.toml").write_text(manifest)
        if suite_manifest is not None:
            (project / "tests" / "osprey.toml").write_text(suite_manifest)
        if oracle is not None:
            (project / "tests" / "entry.expectedoutput").write_text(oracle)
        fixture.write_spec(app_spec(primary, entry))
        return fixture

    def simple_fixture(self, name="demo", lines=4, expected=GOLDEN, source=None, relative="src/lib.py"):
        return self.fixture({name: {"sources": {relative: py_source(lines) if source is None else source}, "expected": expected}})

    def threshold_fixture(self, threshold, total):
        return self.fixture({"demo": {"sources": {"src/lib.py": py_source(total)}}}, {"demo": threshold})

    def assert_exit(self, result, code, context=""):
        self.assertEqual(result.returncode, code, context + result.stdout + result.stderr)

    def assert_entry_refused(self, fixture, message, also=()):
        """Collect the Workbench app once and require the refusal `message`."""
        result = fixture.collect("--project", "apps/workbench")
        self.assert_exit(result, 1)
        for expected in (message, *also):
            self.assertIn(expected, result.stdout)
        return result

    def assert_problem(self, result, message, stderr=False):
        self.assert_exit(result, 1)
        self.assertIn(message, result.stderr if stderr else result.stdout)

    def assert_coverage(self, fixture, covered, total, code):
        fixture.write_report("demo", own_report("demo", covered, total))
        result = fixture.check()
        self.assert_exit(result, code)
        return result

    def assert_invalid(self, fixture, problem):
        result = fixture.check()
        self.assert_exit(result, 1)
        self.assertIn(problem, result.stdout)
        self.assertIsNone(fixture.result("demo")["percentage"])

    def assert_measurement(self, fixture, name, counts):
        row = fixture.measurements()["packages"][name]
        self.assertEqual(tuple(row[key] for key in ("covered", "total", "percentage")), counts)
        self.assertEqual(fixture.config()["projects"]["packages"][name]["threshold"], 90)

    def assert_bad_report(self, fixture, label, report):
        fixture.write_report("bad", report)
        result = fixture.check()
        self.assert_exit(result, 1, label)
        self.assertFalse(fixture.result("bad")["pass"], label)
        self.assertTrue(fixture.result("bad")["missing_evidence"], label)

    def assert_gated_group(self, group, base):
        for name, entry in group.items():
            threshold = entry.get("threshold", 90)
            self.assertGreaterEqual(threshold, 90, name)
            self.assertIsInstance(threshold, (int, float), f"{name}: threshold must be numeric")
            self.assertLessEqual(threshold, 100, name)
            self.assertEqual(entry["language"], "osprey", name)
            self.assertTrue((base / name / "tests" / "osprey.toml").is_file(), name)
            self.assertTrue((base / name / "tests" / "expected_output.txt").is_file(), name)


def make_fixture(base, packages, thresholds=None, apps=None):
    root = Path(base)
    app_gates = None if apps is None else {name: {"language": "osprey"} for name in apps}
    write_gates(root, thresholds if thresholds is not None else {name: 90 for name in packages}, app_gates)
    for kind, group in ((PACKAGES_DIR, packages), (APPS_DIR, apps or {})):
        for name, spec in group.items():
            add_project_files(root, kind, name, spec)
    compiler = root / "fake-compiler.py"
    compiler.write_text(FAKE_COMPILER)
    compiler.chmod(0o755)
    return Fixture(root, compiler)


