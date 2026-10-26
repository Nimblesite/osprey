#!/usr/bin/env python3
"""Regressions for independent package coverage thresholds and missing evidence.

The real `coverage-thresholds.json` is asserted to gate every shipped package
with its own entry, a 90% minimum and optional higher thresholds. The checker is driven
through a throwaway checkout so each rule — threshold boundary, own-source-only
accounting, cross-host paths, malformed rows, oracle replay, stale evidence and
ungated packages — is proven by behaviour rather than by mirroring the code.
"""
import json
import runpy
import tempfile
import unittest
from pathlib import Path
from package_coverage_evidence import record_measurements, seal_evidence, source_inputs
from package_coverage_test_support import (
    ROOT, FOOTER, GOLDEN, add_package, bad_report_cases, file_report,
    check_c_runtime, make_fixture, own_report, py_source, write_gates, write_json,
)


class PackageThresholdTests(unittest.TestCase):
    def test_threshold_configuration_has_no_measurements(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        self.assertNotIn("package_measurements", config)
        for group in config["projects"].values():
            for entry in group.values():
                self.assertTrue(set(entry).isdisjoint({"covered", "total", "percentage"}))

    def test_oracle_literals_roundtrip_every_control_character_and_unicode(self):
        runner = runpy.run_path(str(ROOT / "scripts/oracles/oracle.py"))
        writer = runner["checked_writer"]()
        self.assertEqual(writer.literal("\r"), '"\\r"')

    def test_every_shipped_package_has_an_individual_threshold_of_at_least_90(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        packages = config["projects"].get("packages", {})
        shipped = {p.name for p in (ROOT / "packages").iterdir() if (p / "osprey.toml").is_file()}
        self.assertEqual(set(packages), shipped, "Every package must have its own coverage gate")
        for name, entry in packages.items():
            threshold = entry.get("threshold", 90)
            self.assertGreaterEqual(threshold, 90, name)
            self.assertIsInstance(threshold, (int, float), f"{name}: threshold must be numeric")
            self.assertLessEqual(threshold, 100, name)
            self.assertEqual(entry["language"], "osprey", name)
            self.assertTrue((ROOT / "packages" / name / "tests" / "osprey.toml").is_file(), name)
            self.assertTrue((ROOT / "packages" / name / "tests" / "expected_output.txt").is_file(), name)

    def test_only_thresholds_above_the_minimum_are_explicit(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        for group in config["projects"].values():
            for name, entry in group.items():
                if "threshold" in entry:
                    self.assertGreater(entry["threshold"], 90, name)


class CheckerTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.base = Path(directory.name)

    def fixture(self, packages, thresholds=None):
        return make_fixture(tempfile.mkdtemp(dir=self.base), packages, thresholds)

    def simple_fixture(self, name="demo", lines=4, expected=GOLDEN, source=None, relative="src/lib.py"):
        return self.fixture({name: {"sources": {relative: py_source(lines) if source is None else source}, "expected": expected}})

    def threshold_fixture(self, threshold, total):
        return self.fixture({"demo": {"sources": {"src/lib.py": py_source(total)}}}, {"demo": threshold})

    def assert_coverage(self, fixture, covered, total, code):
        fixture.write_report("demo", own_report("demo", covered, total))
        result = fixture.check()
        self.assert_exit(result, code)
        return result

    def assert_exit(self, result, code, context=""):
        self.assertEqual(result.returncode, code, context + result.stdout + result.stderr)

    def assert_invalid(self, fixture, problem):
        result = fixture.check()
        self.assert_exit(result, 1)
        self.assertIn(problem, result.stdout)
        self.assertIsNone(fixture.result("demo")["percentage"])

    def assert_measurement(self, fixture, name, counts):
        row = fixture.measurements()["packages"][name]
        self.assertEqual(tuple(row[key] for key in ("covered", "total", "percentage")), counts)
        self.assertEqual(fixture.config()["projects"]["packages"][name]["threshold"], 90)

    def assert_problem(self, result, message, stderr=False):
        self.assert_exit(result, 1)
        self.assertIn(message, result.stderr if stderr else result.stdout)

    def test_omitted_threshold_enforces_90_and_preserves_failing_measurements(self):
        for covered, exit_code in ((9, 0), (8, 1)):
            with self.subTest(covered=covered):
                fixture = self.simple_fixture(lines=10)
                write_json(fixture.root / "coverage-thresholds.json", {
                    "projects": {"packages": {"demo": {"language": "osprey"}}}})
                fixture.write_report("demo", own_report("demo", covered, 10))
                result = fixture.check("--record-measurements")
                self.assert_exit(result, exit_code)
                self.assertEqual(fixture.result("demo")["percentage"], covered * 10)
                self.assertNotIn("threshold", fixture.result("demo"))
                self.assertNotIn("(threshold 90%)", result.stdout)
                self.assertEqual(fixture.measurements()["packages"]["demo"]["covered"], covered)
                self.assertNotIn("threshold", fixture.config()["projects"]["packages"]["demo"])

    def test_higher_package_threshold_still_rejects_90_percent(self):
        fixture = self.threshold_fixture(95, 100)
        for covered, code in ((90, 1), (94, 1), (95, 0)):
            with self.subTest(covered=covered):
                result = self.assert_coverage(fixture, covered, 100, code)
                self.assertIn("95%", result.stdout)
                self.assertEqual(fixture.result("demo")["threshold"], 95)

    def test_fractional_ratchet_enforces_exact_counts(self):
        fixture = self.threshold_fixture(90.2, 1000)
        for covered, code in ((900, 1), (901, 1), (902, 0)):
            with self.subTest(covered=covered):
                self.assert_coverage(fixture, covered, 1000, code)
                self.assertEqual(fixture.result("demo")["threshold"], 90.2)

    def test_nonfinite_package_thresholds_fail_closed(self):
        fixture = self.simple_fixture()
        for threshold in (float("nan"), float("inf"), -float("inf")):
            with self.subTest(threshold=threshold):
                write_gates(fixture.root, {"demo": threshold})
                self.assert_exit(fixture.check(), 2)

    def test_decimal_threshold_does_not_falsely_reject_exact_boundary(self):
        fixture = self.threshold_fixture(90.4, 1375)
        for covered, code in ((1242, 1), (1243, 0)):
            with self.subTest(covered=covered):
                self.assert_coverage(fixture, covered, 1375, code)

    def test_c_runtime_omitted_threshold_enforces_90_and_keeps_higher_overrides(self):
        for percentage, entry, code in ((89, {}, 2), (90, {}, 0), (94, {"threshold": 95}, 2), (95, {"threshold": 95}, 0)):
            with self.subTest(percentage=percentage, entry=entry):
                result = check_c_runtime(Path(tempfile.mkdtemp(dir=self.base)), percentage, entry)
                self.assert_exit(result, code)

    def test_c_runtime_invalid_thresholds_fail_closed(self):
        for value in (89, 101, 90.5, "90", True, None):
            with self.subTest(threshold=value):
                result = check_c_runtime(Path(tempfile.mkdtemp(dir=self.base)), 100, {"threshold": value})
                self.assert_exit(result, 2)
                self.assertNotIn("probe OK", result.stdout)

    def test_check_only_requires_a_successfully_verified_collection(self):
        fixture = self.simple_fixture()
        write_json(fixture.reports / "demo.json", own_report("demo", 4, 4))
        result = fixture.check()
        self.assertEqual(result.returncode, 1, "A raw report does not prove its suite passed: " + result.stdout)

    def test_recording_rejects_inputs_changed_by_the_gate(self):
        fixture = self.simple_fixture()
        config = fixture.config()
        def changing_gate(*_args):
            path = fixture.root / "packages/demo/src/lib.py"
            path.write_text(path.read_text() + "\n")
            return {"covered": 4, "total": 4, "percentage": 100.0, "missing_evidence": []}
        problem = record_measurements(fixture.root, fixture.root / "coverage-thresholds.json", fixture.reports, config, changing_gate)
        self.assertIn("inputs changed", problem)
        self.assertEqual(fixture.config(), config)

    def test_concurrent_recording_cannot_overwrite_configuration(self):
        fixture = self.simple_fixture()
        fixture.write_report("demo", own_report("demo", 4, 4))
        config = fixture.config()
        record = fixture.reports / "measurements.json"
        record.write_text("previous record\n")
        for lock in (fixture.root / ".coverage-thresholds.json.lock", fixture.reports / ".measurements.json.lock"):
            lock.write_text("active recorder\n")
            result = fixture.check("--record-measurements")
            self.assert_problem(result, "cannot record measurements", stderr=True)
            self.assertEqual(fixture.config(), config)
            self.assertEqual(record.read_text(), "previous record\n")
            self.assertTrue(lock.exists(), "A competing writer must not remove the owner's lock")
            lock.unlink()

    def test_recording_writes_a_separate_report_without_mutating_thresholds(self):
        fixture = self.simple_fixture()
        fixture.write_report("demo", own_report("demo", 4, 4))
        path = fixture.root / "coverage-thresholds.json"
        original = path.read_bytes()
        self.assert_exit(fixture.check("--record-measurements"), 0)
        self.assertEqual(path.read_bytes(), original)
        self.assertEqual(fixture.measurements()["packages"]["demo"], {"covered": 4, "total": 4, "percentage": 100.0})
        self.assertIn("recorded_at", fixture.measurements())
        self.assertNotIn("package_measurements", fixture.config())

    def test_matching_a_zero_or_failing_golden_is_not_success(self):
        for expected in ("0 passed, 0 failed\n", "FAIL demo\n1 passed, 0 failed\n", "1 passed, 0 failed\n"):
            with self.subTest(expected=expected):
                fixture = self.simple_fixture(expected=expected)
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
                result = fixture.collect()
                self.assertEqual(result.returncode, 2, "Matching invalid oracle output cannot pass: " + result.stdout)

    def test_changed_source_or_golden_invalidates_collected_evidence(self):
        for relative in ("src/lib.py", "tests/expected_output.txt", "tests/osprey.toml"):
            with self.subTest(relative=relative):
                fixture = self.simple_fixture()
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
                self.assertEqual(fixture.collect("--record-measurements").returncode, 0)
                path = fixture.root / "packages" / "demo" / relative
                path.write_text(path.read_text() + "\n# changed after collection\n")
                result = fixture.check("--record-measurements")
                self.assertEqual(result.returncode, 1, "Changed inputs need fresh evidence: " + result.stdout)
                self.assert_measurement(fixture, "demo", (None, None, None))

    def test_zero_suites_and_changed_line_endings_cannot_replay_as_success(self):
        fixture = self.simple_fixture()
        for stdout in (GOLDEN.replace("\n", "\r\n") + FOOTER, GOLDEN + FOOTER.replace("1 passed", "0 passed")):
            with self.subTest(stdout=stdout):
                fixture.write_spec({"demo": {"report": own_report("demo", 4, 4), "stdout": stdout}})
                result = fixture.collect()
                self.assert_exit(result, 1)

    def test_changed_data_fixtures_invalidate_collected_evidence(self):
        fixture = self.simple_fixture()
        data = fixture.root / "packages/demo/tests/fixtures/input.json"
        write_json(data, {"value": 1})
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        self.assertEqual(fixture.collect().returncode, 0)
        write_json(data, {"value": 2})
        result = fixture.check()
        self.assertEqual(result.returncode, 1, "Changed fixture needs fresh evidence: " + result.stdout)

    def test_threshold_boundary_passes_at_90_and_fails_below(self):
        fixture = self.fixture({
            "edge": {"sources": {"src/lib.py": py_source(10)}},
            "below": {"sources": {"src/lib.py": py_source(10)}},
        })
        fixture.write_report("edge", own_report("edge", 9, 10))
        fixture.write_report("below", own_report("below", 8, 10))
        result = fixture.check("--record-measurements")
        self.assert_exit(result, 1)
        self.assertTrue(fixture.result("edge")["pass"], fixture.result("edge"))
        self.assertEqual(fixture.result("edge")["percentage"], 90.0)
        self.assertFalse(fixture.result("below")["pass"])
        self.assertEqual(fixture.result("below")["percentage"], 80.0)
        self.assertEqual(fixture.result("edge")["files"][0]["uncovered_lines"], [10])
        self.assertEqual(fixture.result("below")["files"][0]["uncovered_lines"], [9, 10])
        self.assert_measurement(fixture, "edge", (9, 10, 90.0))
        self.assert_measurement(fixture, "below", (8, 10, 80.0))

    def test_perfect_test_and_helper_lines_cannot_mask_uncovered_source(self):
        fixture = self.simple_fixture("mask", 10)
        fixture.write_report("mask", {"files": {
            "packages/mask/src/lib.py": {"lines": {str(i): 0 for i in range(1, 11)}},
            "packages/mask/tests/main.py": {"lines": {str(i): 90 for i in range(1, 101)}},
            "packages/testing/src/check.py": {"lines": {str(i): 90 for i in range(1, 51)}},
        }})
        write_json(fixture.root / "coverage-thresholds.json", {**fixture.config(),
                   "package_measurements": {"packages": {"mask": {"percentage": 100}}}})
        result = fixture.check()
        self.assert_exit(result, 1)
        mask = fixture.result("mask")
        self.assertEqual((mask["covered"], mask["total"]), (0, 10))
        self.assertEqual(mask["percentage"], 0.0)

    def test_cross_host_and_relative_paths_map_to_local_source(self):
        fixture = self.simple_fixture()
        cases = {
            "ubuntu": file_report("/home/runner/work/osprey/osprey/packages/demo/src/lib.py", 4, 4),
            "relative": file_report("packages/demo/tests/../src/lib.py", 4, 4),
        }
        for label, report in cases.items():
            with self.subTest(label):
                fixture.write_report("demo", report)
                result = fixture.check()
                self.assert_exit(result, 0)
                self.assertEqual(fixture.result("demo")["covered"], 4)
        duplicate = {"files": {
            "/home/runner/work/osprey/osprey/packages/demo/src/lib.py": {"lines": {"1": 1}},
            "packages/demo/tests/../src/lib.py": {"lines": {"2": 1}},
        }}
        fixture.write_report("demo", duplicate)
        result = fixture.check()
        self.assert_exit(result, 1)
        self.assertIn("duplicate source path", result.stdout)

    def assert_bad_report(self, fixture, label, report):
        fixture.write_report("bad", report)
        result = fixture.check()
        self.assert_exit(result, 1, label)
        self.assertFalse(fixture.result("bad")["pass"], label)
        self.assertTrue(fixture.result("bad")["missing_evidence"], label)

    def test_untrustworthy_reports_fail(self):
        fixture = self.simple_fixture("bad", 5)
        for label, report in bad_report_cases().items():
            with self.subTest(label):
                self.assert_bad_report(fixture, label, report)
        for label, raw in {"not-json": "{not json", "empty": ""}.items():
            with self.subTest(label):
                (fixture.reports / "bad.json").write_text(raw)
                result = fixture.check()
                self.assert_exit(result, 1, label)
                self.assertIn("malformed coverage report", result.stdout)

    def test_missing_report_and_missing_required_source_evidence_fail(self):
        fixture = self.fixture({
            "gone": {"sources": {"src/lib.py": py_source(4), "src/only.py": py_source(3)}},
        })
        result = fixture.check("--record-measurements")
        self.assert_problem(result, "missing coverage report")
        self.assert_measurement(fixture, "gone", (None, None, None))
        fixture.write_report("gone", own_report("gone", 4, 4))
        result = fixture.check()
        self.assert_exit(result, 1)
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
        self.assert_exit(result, 1)
        self.assertIn("missing source evidence: src/lib.py", result.stdout)

    def test_runtime_initializers_aliases_and_lambdas_need_source_evidence(self):
        for body in ("value = Lib.entry(1)\n", "entry = lambda value: value + 1\n", "entry = Lib.entry\n"):
            with self.subTest(body=body):
                fixture = self.fixture({"demo": {"sources": {
                    "src/lib.py": py_source(4), "src/facade.py": "from demo import Lib\n" + body,
                }}})
                fixture.write_report("demo", own_report("demo", 4, 4))
                result = fixture.check()
                self.assertEqual(result.returncode, 1, "Omitted runtime source cannot pass: " + result.stdout)
                self.assertIn("missing source evidence: src/facade.py", result.stdout)

    def test_missing_function_rows_cannot_shrink_the_denominator(self):
        source = "def covered(value):\n    return value\n\ndef omitted(value):\n    return value\n"
        fixture = self.simple_fixture(source=source)
        fixture.write_report("demo", own_report("demo", 2, 2))
        result = fixture.check()
        self.assertEqual(result.returncode, 1, "Missing function rows must fail: " + result.stdout)
        self.assertIn("missing function definition counter: lib.py:4", result.stdout)

    def test_missing_return_and_branch_rows_cannot_report_full_coverage(self):
        source = "def choose(flag):\n    if flag:\n        return 1\n    else:\n        return 2\n"
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": source, "src/good.py": py_source(4)}}})
        report = own_report("demo", 1, 1)
        report["files"].update(own_report("demo", 4, 4, "src/good.py")["files"])
        fixture.write_report("demo", report)
        result = fixture.check()
        self.assert_exit(result, 1, "Only the function entry was measured: ")
        for line in (2, 3, 5):
            self.assertIn(f"missing executable statement counter: lib.py:{line}", result.stdout)
        self.assertIsNone(fixture.result("demo")["percentage"], "Incomplete evidence has no measured percentage")

    def test_abstract_effect_declarations_have_no_body_to_cover(self):
        source = "@effect\nclass Buffer:\n    def length(buffer) -> int: ...\n\n" + py_source(4)
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": source}}})
        fixture.write_report("demo", {"files": {"packages/demo/src/lib.py": {
            "lines": {str(line): 1 for line in range(5, 9)},
        }}})
        self.assert_exit(fixture.check(), 0)

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
        self.assert_exit(result, 1)
        self.assertIn("missing source evidence: src/base.ospml", result.stdout)

    def test_exported_default_functions_cannot_disappear_from_source_inventory(self):
        fixture = self.fixture({"demo": {"sources": {
            "src/covered.osp": "fn covered(value) = value\n",
            "src/exported.osp": "export fn uncovered(value) = value\n",
        }}})
        fixture.write_report("demo", own_report("demo", 1, 1, "src/covered.osp"))
        self.assert_invalid(fixture, "missing source evidence: src/exported.osp")
        covered = own_report("demo", 1, 1, "src/covered.osp")["files"]
        exported = own_report("demo", 1, 1, "src/exported.osp")["files"]
        fixture.write_report("demo", {"files": {**covered, **exported}})
        self.assert_exit(fixture.check(), 0)

    def test_ml_and_default_match_arms_cannot_disappear_from_reports(self):
        for suffix, source in (("ospml", "choose flag =\n    match flag\n        true => 1\n        false => 2\n"),
                               ("osp", "fn choose(flag) = match flag {\n    true => 1\n    false => 2\n}\n"),
                               ("ospml", "choose value = match value\n    Success(value=item) => item\n    Error(message=message) => 0\n"),
                               ("osp", "fn choose(value) = match value {\n    Success(value=item) => item\n    Error(message=message) => 0\n}\n")):
            with self.subTest(flavor=suffix):
                fixture = self.fixture({"demo": {"sources": {f"src/lib.{suffix}": source}}})
                fixture.write_report("demo", own_report("demo", 1, 1, f"src/lib.{suffix}"))
                self.assert_invalid(fixture, "missing match arm counter")

    def test_repeated_json_counters_cannot_replace_uncovered_lines(self):
        fixture = self.simple_fixture()
        report = fixture.reports / "demo.json"
        report.parent.mkdir(parents=True)
        report.write_text('{"files":{"packages/demo/src/lib.py":{"lines":{"1":1,"2":1,"3":1,"4":0,"4":1}}}}')
        seal_evidence(fixture.root, "demo", report, source_inputs(fixture.root, "demo"))
        self.assert_invalid(fixture, "duplicate JSON key")

    def test_documentation_match_examples_are_not_executable_arms(self):
        source = '(**\n    true => ignored\n    (* nested " quotation *)\n*)\nchoose x = x\n'
        fixture = self.simple_fixture(source=source, relative="src/lib.ospml")
        fixture.write_report("demo", {"files": {"packages/demo/src/lib.ospml": {"lines": {"5": 1}}}})
        self.assert_exit(fixture.check(), 0)

    def test_failed_collection_invalidates_a_stale_report(self):
        fixture = self.simple_fixture()
        fixture.write_report("demo", own_report("demo", 4, 4))
        fixture.write_spec({"demo": {"exit": 1, "report": own_report("demo", 4, 4)}})
        result = fixture.collect()
        self.assert_exit(result, 1)
        self.assertFalse((fixture.reports / "demo.json").exists(), "failed collection must invalidate evidence")
        self.assertTrue((fixture.reports / "demo.log").is_file())
        self.assertFalse(fixture.result("demo")["pass"])
        self.assertIn("collection failed with exit 1", fixture.result("demo")["missing_evidence"][0])

    def test_oracle_replay_catches_printed_failures_and_missing_tests(self):
        fixture = self.simple_fixture()
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
                self.assert_exit(result, 1, label)
                self.assertFalse((fixture.reports / "demo.json").exists(), label)
                self.assertTrue(fixture.result("demo")["missing_evidence"], label)
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        self.assertEqual(fixture.collect().returncode, 0)

    def test_all_package_suites_overlap_and_write_isolated_reports(self):
        names = ["demo", *(f"peer{i}" for i in range(17))]
        fixture = self.fixture({name: {"sources": {"src/lib.py": py_source(10)}} for name in names})
        fixture.write_spec({name: {"report": own_report(name, 9, 10), "barrier": names} for name in names})
        result = fixture.collect()
        self.assert_exit(result, 0)
        self.assertTrue((fixture.reports / "demo.json").is_file())
        log = (fixture.reports / "demo.log").read_text()
        self.assertIn("--coverage-json", log)
        self.assertIn("ok demo", log)
        self.assertEqual(fixture.summary()["mode"], "collect")
        demo = fixture.result("demo")
        self.assertEqual((demo["covered"], demo["total"], demo["percentage"]), (9, 10, 90.0))
        self.assertEqual(demo["files"][0]["path"], "lib.py")
        self.assertTrue(demo["pass"])
        fixture.write_spec({name: {"report": own_report(name, 9, 10), "memory": "arc"} for name in names})
        self.assert_exit(fixture.collect("--memory", "arc"), 0)
        self.assertEqual({row["name"] for row in fixture.summary()["packages"]}, set(names))
        self.assertTrue(all(row["pass"] for row in fixture.summary()["packages"]))

    def test_ungated_package_fails_even_for_a_focused_package(self):
        fixture = self.simple_fixture()
        self.assert_exit(fixture.check("--package", "demo", "--record-measurements"), 2)
        self.assertNotIn("package_measurements", fixture.config())
        add_package(fixture.root, "extra")
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        result = fixture.collect("--package", "demo")
        self.assert_exit(result, 2)
        self.assertIn("extra: package has no coverage gate entry", result.stderr)
        self.assertFalse((fixture.reports / "demo.json").exists(), "inventory errors must run nothing")

    def test_bad_inventory_and_unknown_package_fail(self):
        fixture = self.simple_fixture()
        cases = [{"demo": 90, "ghost": 90}, {"demo": 89}, {"demo": 101}, {"demo": "90"}, {"demo": True}]
        for entries in cases:
            with self.subTest(entries=entries):
                write_gates(fixture.root, entries)
                result = fixture.check()
                self.assert_exit(result, 2)
        write_gates(fixture.root, {"demo": 90})
        result = fixture.check("--package", "unknown")
        self.assert_exit(result, 2)
        self.assertIn("unknown package", result.stderr)
        (fixture.root / "packages" / "demo" / "tests" / "osprey.toml").unlink()
        result = fixture.check()
        self.assert_exit(result, 2)
        self.assertIn("missing tests/osprey.toml", result.stderr)

if __name__ == "__main__":
    unittest.main()
