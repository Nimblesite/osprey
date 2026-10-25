#!/usr/bin/env python3
"""Regressions for independent package coverage thresholds and missing evidence.

The real `coverage-thresholds.json` is asserted to gate every shipped package
with its own 90%-or-better entry, and `scripts/package_coverage.py` is driven
through a throwaway checkout so each rule — threshold boundary, own-source-only
accounting, cross-host paths, malformed rows, oracle replay, stale evidence and
ungated packages — is proven by behaviour rather than by mirroring the code.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from package_coverage_evidence import seal_evidence, source_inputs

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts" / "package_coverage.py"
FOOTER = "# coverage total: 90.0% (9/10 lines)\n# suites: 1 passed, 0 failed\n"
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


def add_sources(root, name, sources, expected="ok demo\n"):
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
        add_sources(root, name, spec.get("sources", {}), spec.get("expected", "ok demo\n"))
    compiler = root / "fake-compiler.py"
    compiler.write_text(FAKE_COMPILER)
    compiler.chmod(0o755)
    return Fixture(root, compiler)


class PackageThresholdTests(unittest.TestCase):
    def test_every_shipped_package_has_an_individual_threshold_of_at_least_90(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        packages = config["projects"].get("packages", {})
        shipped = {p.name for p in (ROOT / "packages").iterdir() if (p / "osprey.toml").is_file()}
        self.assertEqual(set(packages), shipped, "Every package must have its own coverage gate")
        for name, entry in packages.items():
            self.assertGreaterEqual(entry["threshold"], 90, name)
            self.assertIsInstance(entry["threshold"], int, f"{name}: threshold must be an integer")
            self.assertLessEqual(entry["threshold"], 100, name)
            self.assertEqual(entry["language"], "osprey", name)
            self.assertTrue((ROOT / "packages" / name / "tests" / "osprey.toml").is_file(), name)
            self.assertTrue((ROOT / "packages" / name / "tests" / "expected_output.txt").is_file(), name)


class CheckerTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.base = Path(directory.name)

    def fixture(self, packages, thresholds=None):
        return make_fixture(tempfile.mkdtemp(dir=self.base), packages, thresholds)

    def test_check_only_requires_a_successfully_verified_collection(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        write_json(fixture.reports / "demo.json", own_report("demo", 4, 4))
        result = fixture.check()
        self.assertEqual(result.returncode, 1, "A raw report does not prove its suite passed: " + result.stdout)

    def test_changed_source_or_golden_invalidates_collected_evidence(self):
        for relative in ("src/lib.py", "tests/expected_output.txt", "tests/osprey.toml"):
            with self.subTest(relative=relative):
                fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
                self.assertEqual(fixture.collect().returncode, 0)
                path = fixture.root / "packages" / "demo" / relative
                path.write_text(path.read_text() + "\n# changed after collection\n")
                result = fixture.check()
                self.assertEqual(result.returncode, 1, "Changed inputs need fresh evidence: " + result.stdout)

    def test_zero_suites_and_changed_line_endings_cannot_replay_as_success(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        for stdout in ("ok demo\r\n" + FOOTER, "ok demo\n" + FOOTER.replace("1 passed", "0 passed")):
            with self.subTest(stdout=stdout):
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4), "stdout": stdout}})
                result = fixture.collect()
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_threshold_boundary_passes_at_90_and_fails_below(self):
        fixture = self.fixture({
            "edge": {"sources": {"src/lib.py": py_source(10)}},
            "below": {"sources": {"src/lib.py": py_source(10)}},
        })
        fixture.write_report("edge", own_report("edge", 9, 10))
        fixture.write_report("below", own_report("below", 8, 10))
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertTrue(fixture.result("edge")["pass"], fixture.result("edge"))
        self.assertEqual(fixture.result("edge")["percentage"], 90.0)
        self.assertFalse(fixture.result("below")["pass"])
        self.assertEqual(fixture.result("below")["percentage"], 80.0)

    def test_perfect_test_and_helper_lines_cannot_mask_uncovered_source(self):
        fixture = self.fixture({"mask": {"sources": {"src/lib.py": py_source(10)}}})
        fixture.write_report("mask", {"files": {
            "packages/mask/src/lib.py": {"lines": {str(i): 0 for i in range(1, 11)}},
            "packages/mask/tests/main.py": {"lines": {str(i): 90 for i in range(1, 101)}},
            "packages/testing/src/check.py": {"lines": {str(i): 90 for i in range(1, 51)}},
        }})
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        mask = fixture.result("mask")
        self.assertEqual((mask["covered"], mask["total"]), (0, 10))
        self.assertEqual(mask["percentage"], 0.0)

    def test_cross_host_and_relative_paths_map_to_local_source(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        cases = {
            "ubuntu": file_report("/home/runner/work/osprey/osprey/packages/demo/src/lib.py", 4, 4),
            "relative": file_report("packages/demo/tests/../src/lib.py", 4, 4),
        }
        for label, report in cases.items():
            with self.subTest(label):
                fixture.write_report("demo", report)
                result = fixture.check()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(fixture.result("demo")["covered"], 4)
        duplicate = {"files": {
            "/home/runner/work/osprey/osprey/packages/demo/src/lib.py": {"lines": {"1": 1}},
            "packages/demo/tests/../src/lib.py": {"lines": {"2": 1}},
        }}
        fixture.write_report("demo", duplicate)
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("duplicate source path", result.stdout)

    def assert_bad_report(self, fixture, label, report):
        fixture.write_report("bad", report)
        result = fixture.check()
        self.assertEqual(result.returncode, 1, label + result.stdout + result.stderr)
        self.assertFalse(fixture.result("bad")["pass"], label)
        self.assertTrue(fixture.result("bad")["missing_evidence"], label)

    def test_untrustworthy_reports_fail(self):
        fixture = self.fixture({"bad": {"sources": {"src/lib.py": py_source(5)}}})
        for label, report in bad_report_cases().items():
            with self.subTest(label):
                self.assert_bad_report(fixture, label, report)
        for label, raw in {"not-json": "{not json", "empty": ""}.items():
            with self.subTest(label):
                (fixture.reports / "bad.json").write_text(raw)
                result = fixture.check()
                self.assertEqual(result.returncode, 1, label + result.stdout + result.stderr)
                self.assertIn("malformed coverage report", result.stdout)

    def test_missing_report_and_missing_required_source_evidence_fail(self):
        fixture = self.fixture({
            "gone": {"sources": {"src/lib.py": py_source(4), "src/only.py": py_source(3)}},
        })
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("missing coverage report", result.stdout)
        fixture.write_report("gone", own_report("gone", 4, 4))
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("missing source evidence: src/only.py", result.stdout)

    def test_data_only_tables_are_not_required_but_real_source_is(self):
        fixture = self.fixture({"tabled": {"sources": {
            "src/lib.py": py_source(4),
            "src/table.py": "VALUES = [1, 2, 3]\n",
        }}})
        fixture.write_report("tabled", own_report("tabled", 4, 4))
        self.assertEqual(fixture.check().returncode, 0)
        fixture.write_report("tabled", {"files": {
            "packages/tabled/src/table.py": {"lines": {"1": 1}},
        }})
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("missing source evidence: src/lib.py", result.stdout)

    def test_ml_and_default_flavor_functions_need_evidence(self):
        fixture = self.fixture({
            "mldemo": {"sources": {"src/base.ospml": "namespace demo\n\nexport find (items, matches) = listGet items 0\n"}},
            "defaultdemo": {"sources": {"src/main.osp": "fn entry(value) = value\n"}},
        })
        fixture.write_report("mldemo", file_report("packages/mldemo/tests/../src/base.ospml", 1, 1))
        fixture.write_report("defaultdemo", file_report("packages/defaultdemo/src/main.osp", 1, 1))
        self.assertEqual(fixture.check().returncode, 0)
        fixture.write_report("mldemo", {"files": {}})
        result = fixture.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("missing source evidence: src/base.ospml", result.stdout)

    def test_failed_collection_invalidates_a_stale_report(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        fixture.write_report("demo", own_report("demo", 4, 4))
        fixture.write_spec({"demo": {"exit": 1, "report": own_report("demo", 4, 4)}})
        result = fixture.collect()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertFalse((fixture.reports / "demo.json").exists(), "failed collection must invalidate evidence")
        self.assertTrue((fixture.reports / "demo.log").is_file())
        self.assertFalse(fixture.result("demo")["pass"])
        self.assertIn("collection failed with exit 1", fixture.result("demo")["missing_evidence"][0])

    def test_oracle_replay_catches_printed_failures_and_missing_tests(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        cases = {
            "printed-failure": "FAIL demo.one\n" + FOOTER,
            "missing-tests": FOOTER,
            "extra-line": "ok demo\nok demo.extra\n" + FOOTER,
            "no-runner-footer": "ok demo\n",
        }
        for label, stdout in cases.items():
            with self.subTest(label):
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4), "stdout": stdout}})
                result = fixture.collect()
                self.assertEqual(result.returncode, 1, label + result.stdout + result.stderr)
                self.assertFalse((fixture.reports / "demo.json").exists(), label)
                self.assertTrue(fixture.result("demo")["missing_evidence"], label)
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        self.assertEqual(fixture.collect().returncode, 0)

    def test_collection_writes_report_log_and_summary(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(10)}}})
        fixture.write_spec({"demo": {"report": own_report("demo", 9, 10)}})
        result = fixture.collect()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((fixture.reports / "demo.json").is_file())
        log = (fixture.reports / "demo.log").read_text()
        self.assertIn("--coverage-json", log)
        self.assertIn("ok demo", log)
        self.assertEqual(fixture.summary()["mode"], "collect")
        demo = fixture.result("demo")
        self.assertEqual((demo["covered"], demo["total"], demo["percentage"]), (9, 10, 90.0))
        self.assertEqual(demo["files"][0]["path"], "lib.py")
        self.assertTrue(demo["pass"])

    def test_ungated_package_fails_even_for_a_focused_package(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        add_package(fixture.root, "extra")
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        result = fixture.collect("--package", "demo")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("extra: package has no coverage gate entry", result.stderr)
        self.assertFalse((fixture.reports / "demo.json").exists(), "inventory errors must run nothing")

    def test_bad_inventory_and_unknown_package_fail(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(4)}}})
        cases = [{"demo": 90, "ghost": 90}, {"demo": 89}, {"demo": 101}, {"demo": "90"}, {"demo": True}]
        for entries in cases:
            with self.subTest(entries=entries):
                write_gates(fixture.root, entries)
                result = fixture.check()
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        write_gates(fixture.root, {"demo": 90})
        result = fixture.check("--package", "unknown")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("unknown package", result.stderr)
        (fixture.root / "packages" / "demo" / "tests" / "osprey.toml").unlink()
        result = fixture.check()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("missing tests/osprey.toml", result.stderr)

if __name__ == "__main__":
    unittest.main()
