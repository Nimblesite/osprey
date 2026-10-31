#!/usr/bin/env python3
"""Regressions for `apps/` gating and `--project` selection in the coverage gate.

Every shipped app is covered by the same 90% floor as a package, `--project`
collects exactly one project, an ungated app fails the inventory before any
suite runs, and an app report can never be mistaken for a package report.
"""
import json
import os
import tomllib
import unittest
from package_coverage_evidence import digest, source_inputs
from package_coverage_test_support import (
    ENTRY_FOOTER, ENTRY_MANIFEST, ENTRY_ORACLE, ROOT, CoverageTestCase, add_package, entry_report,
    masked_report, own_report, py_source, suite_manifest, write_json,
)


class AppThresholdTests(CoverageTestCase):
    def test_every_shipped_app_is_registered_under_projects_apps_at_the_90_floor(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        apps = config["projects"].get("apps", {})
        shipped = {p.name for p in (ROOT / "apps").iterdir() if (p / "osprey.toml").is_file()}
        self.assertEqual(set(apps), shipped, "Every shipped app must have its own coverage gate")
        self.assert_gated_group(apps, ROOT / "apps")


class ProjectTests(CoverageTestCase):
    def two_projects(self):
        return self.fixture({"demo": {"sources": {"src/lib.py": py_source(10)}}},
                            apps={"workbench": {"sources": {"src/lib.py": py_source(10)}}})

    def test_apps_project_is_gated_at_the_90_floor(self):
        fixture = self.fixture({"demo": {"sources": {"src/lib.py": py_source(10)}}},
                               apps={"workbench": {"sources": {"src/lib.py": py_source(10)}}})
        fixture.write_report("demo", own_report("demo", 9, 10))
        for covered, code in ((9, 0), (8, 1)):
            with self.subTest(covered=covered):
                fixture.write_report("workbench", own_report("workbench", covered, 10, kind="apps"), kind="apps")
                result = fixture.check()
                self.assert_exit(result, code)
                workbench = fixture.result("apps/workbench")
                self.assertEqual((workbench["covered"], workbench["total"]), (covered, 10))
                self.assertEqual(workbench["percentage"], covered * 10)
                self.assertNotIn("threshold", workbench)

    def test_ungated_apps_dir_fails_the_inventory_even_for_a_focused_project(self):
        fixture = self.simple_fixture()
        add_package(fixture.root, "extra", "apps")
        result = fixture.check("--project", "packages/demo")
        self.assert_exit(result, 2)
        self.assertIn("apps/extra: app has no coverage gate entry", result.stderr)
        self.assertFalse((fixture.reports / "demo.json").exists(), "inventory errors must run nothing")

    def test_apps_gate_entry_without_a_directory_fails(self):
        fixture = self.simple_fixture()
        config = fixture.config()
        config["projects"]["apps"] = {"ghost": {"language": "osprey"}}
        write_json(fixture.root / "coverage-thresholds.json", config)
        result = fixture.check()
        self.assert_exit(result, 2)
        self.assertIn("apps/ghost: gate entry has no apps/ghost/osprey.toml", result.stderr)

    def test_project_flag_selects_and_collects_exactly_one_project(self):
        fixture = self.two_projects()
        fixture.write_report("demo", own_report("demo", 9, 10))
        fixture.write_report("workbench", own_report("workbench", 9, 10, kind="apps"), kind="apps")
        self.assert_exit(fixture.check("--project", "apps/workbench"), 0)
        self.assertEqual({row["name"] for row in fixture.summary()["packages"]}, {"apps/workbench"})
        self.assert_exit(fixture.check("--project", "packages/demo"), 0)
        self.assertEqual({row["name"] for row in fixture.summary()["packages"]}, {"demo"})
        fixture.write_spec({"workbench": {"report": own_report("workbench", 9, 10, kind="apps")}})
        (fixture.reports / "demo.json").unlink()
        result = fixture.collect("--project", "apps/workbench")
        self.assert_exit(result, 0)
        self.assertTrue((fixture.reports / "apps-workbench.json").is_file())
        self.assertFalse((fixture.reports / "demo.json").exists(), "a focused project must not collect peers")
        self.assertEqual(fixture.result("apps/workbench")["percentage"], 90.0)
        for bad in ("unknown/app", "demo"):
            self.assertIn(f"unknown project {bad!r}", fixture.check("--project", bad).stderr)

    def test_package_flag_still_means_packages_only(self):
        fixture = self.two_projects()
        fixture.write_report("workbench", own_report("workbench", 9, 10, kind="apps"), kind="apps")
        self.assertIn("unknown package 'workbench'", fixture.check("--package", "workbench").stderr)
        fixture.write_report("demo", own_report("demo", 9, 10))
        self.assert_exit(fixture.check("--package", "demo"), 0)
        self.assertEqual({row["name"] for row in fixture.summary()["packages"]}, {"demo"})

    def test_app_reports_never_collide_with_a_same_named_package(self):
        fixture = self.fixture({"workbench": {"sources": {"src/lib.py": py_source(10)}}},
                               apps={"workbench": {"sources": {"src/lib.py": py_source(10)}}})
        fixture.write_report("workbench", own_report("workbench", 9, 10))
        fixture.write_report("workbench", own_report("workbench", 9, 10, kind="apps"), kind="apps")
        result = fixture.check()
        self.assert_exit(result, 0)
        self.assertEqual({row["name"] for row in fixture.summary()["packages"]}, {"workbench", "apps/workbench"})
        self.assertTrue(all((fixture.reports / name).is_file() for name in ("workbench.json", "apps-workbench.json")))


class EntryProbeTests(CoverageTestCase):
    """A project's own entry run is measured by a second, sealed collector run."""

    def app(self, entry=None, oracle=ENTRY_ORACLE, primary=None):
        return self.workbench(primary=primary, entry=entry, oracle=oracle)

    def passed_entry(self, covered=10):
        return {"report": entry_report(covered), "stdout": ENTRY_ORACLE + ENTRY_FOOTER}

    def test_entry_hits_are_summed_into_the_suite_report_not_replaced(self):
        # Masks: suite 1..7 x2, entry 1 x3 and 8..10 x1, so only a true sum hits 100%.
        primary = masked_report({line: 2 for line in range(1, 8)})
        entry = masked_report({1: 3, **{line: 1 for line in range(8, 11)}})
        fixture = self.app({"report": entry, "stdout": ENTRY_ORACLE + ENTRY_FOOTER}, primary=primary)
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        expected = {1: 5, **{line: 2 for line in range(2, 8)}, **{line: 1 for line in range(8, 11)}}
        merged = fixture.merged_lines()
        self.assertEqual(len(merged), 1, "one logical own file after merging")
        self.assertEqual(next(iter(merged.values())), expected,
                         "entry hits add to the suite's own counts; they never replace, cap or vanish")
        self.assertTrue((fixture.reports / "apps-workbench.entry.json").is_file())
        self.assertTrue((fixture.reports / "apps-workbench.entry.log").is_file())
        self.assert_exit(fixture.check("--project", "apps/workbench"), 0)

    def test_alias_spellings_merge_into_one_logical_entry(self):
        primary = entry_report(9, 10, "apps/workbench/tests/../src/lib.py")
        entry = entry_report(10)
        self.assertNotEqual(list(primary["files"]), list(entry["files"]), "the raw keys must differ")
        fixture = self.app({"report": entry, "stdout": ENTRY_ORACLE + ENTRY_FOOTER}, primary=primary)
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        self.assertEqual(len(fixture.collect_files()), 1, "tests/../src and src are the same source")
        self.assertEqual(next(iter(fixture.merged_lines().values()))[10], 1)

    def test_missing_entry_oracle_keeps_normal_collection(self):
        fixture = self.app(oracle=None)
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        self.assertFalse((fixture.reports / "apps-workbench.entry.json").exists())
        self.assertEqual(fixture.result("apps/workbench")["covered"], 9)

    def test_bad_entry_oracle_text_is_refused(self):
        fixture = self.app({"report": entry_report(10), "stdout": "something else\n" + ENTRY_FOOTER})
        self.assert_entry_refused(fixture, "entry probe output differs from entry.expectedoutput")
        self.assertFalse((fixture.reports / "apps-workbench.json").exists())

    def test_entry_probe_nonzero_status_is_refused(self):
        self.assert_entry_refused(self.app({**self.passed_entry(), "exit": 3}), "entry probe exited 3")

    def test_entry_probe_counter_must_hit_the_entry_module(self):
        for label, report in (("absent", {"files": {}}), ("never executed", entry_report(0))):
            with self.subTest(label=label):
                fixture = self.app({"report": report, "stdout": ENTRY_ORACLE + ENTRY_FOOTER})
                self.assert_entry_refused(fixture, "entry probe", also=("lib.py",))

    def test_entry_probe_missing_report_is_refused(self):
        self.assert_entry_refused(self.app({"stdout": ENTRY_ORACLE + ENTRY_FOOTER}),
                                  "missing coverage report")

    def test_sources_changing_during_the_entry_probe_are_refused(self):
        changed = {**self.passed_entry(), "mutate": "apps/workbench/src/extra.py"}
        self.assert_entry_refused(self.app(changed), "source or suite changed during coverage collection")


class PrimaryInvalidEvidenceTests(CoverageTestCase):
    """The suite report is validated before any entry hit is merged into it."""

    def app(self, primary, entry=None, oracle=ENTRY_ORACLE):
        probe = {"report": entry_report(10), "stdout": ENTRY_ORACLE + ENTRY_FOOTER}
        return self.workbench(primary=primary, entry=probe if entry is None else entry, oracle=oracle)

    def corrupted(self, count):
        report = entry_report(9, 10)
        report["files"]["apps/workbench/src/lib.py"]["lines"]["10"] = count
        return {"report": report}

    def assert_refused(self, fixture, message):
        self.assert_entry_refused(fixture, message)
        self.assertFalse((fixture.reports / "apps-workbench.json").exists(),
                         "refused evidence must not leave a merged report")

    def test_a_negative_primary_counter_is_not_compensated_by_entry_hits(self):
        self.assert_refused(self.app(self.corrupted(-1)), "bad hit count")

    def test_a_boolean_primary_counter_is_not_summed_into_an_integer(self):
        self.assert_refused(self.app(self.corrupted(True)), "bad hit count")

    def test_primary_alias_duplicates_are_refused_before_merging(self):
        report = entry_report(9, 10)
        report["files"]["apps/workbench/tests/../src/lib.py"] = report["files"]["apps/workbench/src/lib.py"]
        self.assert_refused(self.app({"report": report}), "duplicate source path after normalization")

    def test_a_failed_primary_run_stops_before_the_entry_probe(self):
        fixture = self.app({"report": entry_report(9, 10), "exit": 3})
        self.assert_refused(fixture, "collection failed with exit 3")
        self.assertFalse((fixture.reports / "apps-workbench.entry.json").exists(),
                         "the entry probe must not run after a failed suite")

    def test_a_bad_primary_oracle_stops_before_the_entry_probe(self):
        fixture = self.app({"report": entry_report(9, 10), "stdout": "unexpected\n" + ENTRY_FOOTER})
        self.assert_refused(fixture, "differs from expected_output.txt")
        self.assertFalse((fixture.reports / "apps-workbench.entry.json").exists())


DEPENDENCY = "../../examples/shared/level.ospml"


class SingleFileDependencyTests(CoverageTestCase):
    """A declared source root that is one file must be fingerprinted like a folder."""

    def app(self, with_oracle=True):
        suite = suite_manifest(".", "../src", DEPENDENCY)
        fixture = self.workbench(oracle=ENTRY_ORACLE if with_oracle else None, suite_manifest=suite)
        dependency = fixture.root / "apps" / "workbench" / "tests" / DEPENDENCY
        dependency.parent.mkdir(parents=True, exist_ok=True)
        dependency.write_text("shared dependency\n")
        return fixture, dependency

    def test_a_single_file_dependency_sits_in_the_input_set_with_its_hash(self):
        fixture, dependency = self.app(with_oracle=False)
        inputs = source_inputs(fixture.root, "workbench", "apps")
        key = dependency.resolve().relative_to(fixture.root.resolve()).as_posix()
        self.assertIn(key, inputs, "a declared file root must be fingerprinted itself")
        self.assertEqual(inputs[key], digest(dependency), "the exact dependency hash belongs in the set")
        dependency.write_text("shared dependency, edited\n")
        self.assertNotEqual(source_inputs(fixture.root, "workbench", "apps")[key], inputs[key])

    def test_editing_a_single_file_dependency_invalidates_an_existing_report(self):
        fixture, dependency = self.app(with_oracle=False)
        fixture.write_report("workbench", entry_report(10), kind="apps")
        self.assert_exit(fixture.check("--project", "apps/workbench"), 0)
        dependency.write_text("shared dependency, edited\n")
        result = fixture.check("--project", "apps/workbench")
        self.assert_exit(result, 1)
        self.assertIn("stale or altered coverage evidence", result.stdout)

    def test_changing_a_single_file_dependency_during_the_probe_is_refused(self):
        fixture, _ = self.app()
        fixture.write_spec({"workbench": {"report": entry_report(9, 10)},
                            "apps": {"report": entry_report(10), "stdout": ENTRY_ORACLE + ENTRY_FOOTER,
                                     "mutate": "apps/workbench/tests/" + DEPENDENCY}})
        result = self.assert_entry_refused(fixture, "source or suite changed during coverage collection")


class EntryManifestTests(CoverageTestCase):
    """A broken entry declaration is a project diagnostic, never a traceback."""

    def app(self, manifest=None):
        probe = {"report": entry_report(10), "stdout": ENTRY_ORACLE + ENTRY_FOOTER}
        return self.workbench(entry=probe, manifest=ENTRY_MANIFEST if manifest is None else manifest)

    def assert_diagnosed(self, fixture, message):
        result = self.assert_entry_refused(fixture, "entry probe:", also=(message,))
        self.assertNotIn("Traceback", result.stdout + result.stderr, "a manifest fault is a diagnostic")

    def test_missing_entry_declaration_is_diagnosed(self):
        self.assert_diagnosed(self.app('[project]\nname = "workbench"\n'), "project.entry is not a source path")

    def test_entry_outside_the_source_root_is_diagnosed(self):
        self.assert_diagnosed(self.app('[project]\nname = "workbench"\nentry = "tests/main.ospml"\n'),
                              "is not under src/")

    def test_non_string_entry_declarations_are_diagnosed(self):
        for value in ("3", '["src/lib.py"]', "true"):
            with self.subTest(entry=value):
                self.assert_diagnosed(self.app(f'[project]\nname = "workbench"\nentry = {value}\n'),
                                      "project.entry is not a source path")

    def test_a_non_table_project_is_diagnosed(self):
        self.assert_diagnosed(self.app('project = "workbench"\n'), "has no [project] table")

    def test_unreadable_or_malformed_manifests_are_diagnosed(self):
        for label, text in (("malformed", "project = [\n"), ("not utf-8", "project = \"\udcff\"\n")):
            with self.subTest(label=label):
                fixture = self.app(None)
                manifest = fixture.root / "apps" / "workbench" / "osprey.toml"
                manifest.write_bytes(text.encode("utf-8", "surrogateescape"))
                self.assert_diagnosed(fixture, "cannot read")


class SuiteReceiptTests(CoverageTestCase):
    """The suite's own raw report survives the merge as a distinct receipt."""

    ENTRY = {1: 3, **{line: 1 for line in range(8, 11)}}

    def app(self, entry=None):
        primary = masked_report({line: 2 for line in range(1, 8)})
        probe = entry if entry is not None else {"report": masked_report(self.ENTRY),
                                                  "stdout": ENTRY_ORACLE + ENTRY_FOOTER}
        return self.workbench(primary=primary, entry=probe)

    def receipt_lines(self, fixture):
        report = json.loads((fixture.reports / "apps-workbench.suite.json").read_text())
        body = report["files"]["apps/workbench/src/lib.py"]["lines"]
        return {int(line): count for line, count in body.items()}

    def test_the_suite_receipt_keeps_the_original_raw_counts(self):
        fixture = self.app()
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        self.assertEqual(self.receipt_lines(fixture),
                         {line: 2 if line <= 7 else 0 for line in range(1, 11)},
                         "the receipt is the suite's own raw report, not the merge")
        merged = next(iter(fixture.merged_lines().values()))
        self.assertEqual(merged, {1: 5, **{line: 2 for line in range(2, 8)},
                                  **{line: 1 for line in range(8, 11)}})

    def test_a_stale_suite_receipt_is_cleared_before_collection(self):
        fixture = self.app({"report": masked_report(self.ENTRY), "stdout": ENTRY_ORACLE + ENTRY_FOOTER,
                            "exit": 3})
        stale = fixture.reports / "apps-workbench.suite.json"
        write_json(stale, masked_report({line: 9 for line in range(1, 11)}))
        result = self.assert_entry_refused(fixture, "entry probe exited 3")
        self.assertFalse(stale.exists(), "a refused collection must not leave a stale receipt")

    def test_an_unclearable_stale_receipt_is_diagnosed(self):
        fixture = self.app()
        (fixture.reports / "apps-workbench.suite.json").mkdir(parents=True)
        result = self.assert_entry_refused(fixture, "cannot remove stale evidence")
        self.assertNotIn("Traceback", result.stdout + result.stderr)
        self.assertFalse((fixture.reports / "apps-workbench.json").exists())


class EntryOracleFaultTests(CoverageTestCase):
    """A declared but unusable oracle is a diagnostic, never a silent skip."""

    def app(self):
        probe = {"report": entry_report(10), "stdout": ENTRY_ORACLE + ENTRY_FOOTER}
        fixture = self.workbench(entry=probe, oracle=None)
        return fixture, fixture.root / "apps" / "workbench" / "tests" / "entry.expectedoutput"

    def test_a_directory_entry_oracle_is_diagnosed(self):
        fixture, oracle = self.app()
        oracle.mkdir()
        result = self.assert_entry_refused(fixture, "entry oracle")
        self.assertIn("not a readable file", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_a_dangling_entry_oracle_symlink_is_diagnosed(self):
        fixture, oracle = self.app()
        os.symlink(fixture.root / "missing-oracle", oracle)
        result = self.assert_entry_refused(fixture, "entry oracle")
        self.assertFalse((fixture.reports / "apps-workbench.entry.json").exists(),
                         "an unusable oracle must not be skipped silently")


class RawCollectorOutputTests(CoverageTestCase):
    def test_collector_output_that_is_not_utf8_is_a_diagnostic(self):
        fixture = self.simple_fixture()
        fixture.write_spec({"demo": {"report": own_report("demo", 9, 10), "raw_stdout": [0xff, 0xfe]}})
        result = fixture.collect("--project", "packages/demo")
        self.assert_exit(result, 1)
        self.assertIn("not valid UTF-8", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)


class StaleReceiptTests(CoverageTestCase):
    """Receipts from an earlier collection must not survive a later failure."""

    def app(self):
        probe = {"report": entry_report(10), "stdout": ENTRY_ORACLE + ENTRY_FOOTER}
        return self.workbench(entry=probe)

    def receipts(self, fixture):
        return [fixture.reports / f"apps-workbench{suffix}" for suffix in (".suite.json", ".entry.json")]

    def test_a_failed_second_collection_leaves_no_receipts(self):
        fixture = self.app()
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        self.assertTrue(all(path.is_file() for path in self.receipts(fixture)), "a pass writes both receipts")
        self.assertTrue((fixture.reports / "apps-workbench.evidence.json").is_file(), "a pass seals evidence")
        fixture.write_spec({"workbench": {"report": entry_report(9, 10), "exit": 3}})
        result = self.assert_entry_refused(fixture, "collection failed with exit 3")
        stale = self.receipts(fixture) + [fixture.reports / "apps-workbench.json",
                                          fixture.reports / "apps-workbench.evidence.json"]
        for path in stale:
            self.assertFalse(path.exists(), f"{path.name} outlived the failed collection")

    def assert_symlink_survives(self, leaf):
        fixture = self.app()
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        sentinel = fixture.root / f"external-{leaf.strip('.')}.json"
        sentinel.write_text('{"files": {}}\n')
        link = fixture.reports / f"apps-workbench{leaf}"
        link.unlink()
        os.symlink(sentinel, link)
        fixture.write_spec({"workbench": {"report": entry_report(9, 10), "exit": 3}})
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 1)
        self.assertEqual(sentinel.read_text(), '{"files": {}}\n', "the external target must survive")
        self.assertFalse(os.path.lexists(link), "the link itself is what gets removed")

    def test_a_symlinked_artifact_never_hands_the_delete_to_its_target(self):
        for leaf in (".suite.json", ".json"):
            with self.subTest(leaf=leaf):
                self.assert_symlink_survives(leaf)

    def test_removing_the_oracle_clears_its_old_receipts(self):
        fixture = self.app()
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        (fixture.root / "apps" / "workbench" / "tests" / "entry.expectedoutput").unlink()
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        for path in self.receipts(fixture):
            self.assertFalse(path.exists(), f"{path.name} outlived the removed oracle")


class ExternalRootTests(CoverageTestCase):
    """A declared root outside the checkout fails closed with a diagnostic."""

    def app(self, declared):
        return self.workbench(suite_manifest=suite_manifest(".", "../src", declared))

    def assert_diagnosed(self, fixture, detail):
        result = self.assert_entry_refused(fixture, "cannot verify collection", also=(detail,))
        self.assertNotIn("Traceback", result.stdout + result.stderr)
        self.assertFalse((fixture.reports / "apps-workbench.json").exists(),
                         "refused input evidence must not leave a report")

    def outside(self):
        outside = self.base / "outside-root"
        outside.mkdir()
        (outside / "dep.ospml").write_text("external dependency\n")
        return outside

    def test_an_absolute_declared_root_outside_the_checkout_is_diagnosed(self):
        self.assert_diagnosed(self.app(str(self.outside())), "not in the subpath")

    def test_a_declared_root_symlinked_outside_the_checkout_is_diagnosed(self):
        outside = self.outside()
        fixture = self.app("external")
        (fixture.root / "apps" / "workbench" / "tests" / "external").symlink_to(outside)
        self.assert_diagnosed(fixture, "not in the subpath")

    def test_a_malformed_suite_manifest_is_diagnosed(self):
        fixture = self.workbench()
        (fixture.root / "apps" / "workbench" / "tests" / "osprey.toml").write_text("project = [\n")
        self.assert_diagnosed(fixture, "Invalid value")


class SuiteManifestShapeTests(CoverageTestCase):
    """Manifest shapes are type-checked: a mistyped root is a diagnostic, not a guess."""

    CASES = (
        ("project is not a table", "project = false"),
        ("source_roots is a number", "\n".join(['[project]', 'name = "workbench-tests"', "source_roots = 42", ""])),
        ("source_roots is a string", "\n".join(['[project]', 'name = "workbench-tests"', 'source_roots = "missing"', ""])),
        ("source_roots item is a number", "\n".join(['[project]', 'name = "workbench-tests"', "source_roots = [1]", ""])),
    )

    def test_mistyped_manifest_shapes_are_per_project_diagnostics(self):
        for label, text in self.CASES:
            with self.subTest(label=label):
                fixture = self.workbench()
                (fixture.root / "apps" / "workbench" / "tests" / "osprey.toml").write_text(text)
                result = self.assert_entry_refused(fixture, "cannot verify collection")
                self.assertNotIn("Traceback", result.stdout + result.stderr)


HOSTILE_ROOTS = (
    r"C:\work\osprey\examples\proof",
    r"C:\a b\c\"d\e",
    "/posix/with\"quote",
    "comma,inside",
    "hash#inside",
)


class EmptySourceRootTests(CoverageTestCase):
    """`source_roots = [""]` is the project directory, as the compiler accepts it."""

    def test_an_empty_root_fingerprints_like_the_project_directory(self):
        empty = self.workbench(suite_manifest=suite_manifest(".", ""), oracle=None)
        dotted = self.workbench(suite_manifest=suite_manifest(".", "."), oracle=None)
        left = source_inputs(empty.root, "workbench", "apps")
        right = source_inputs(dotted.root, "workbench", "apps")
        self.assertEqual(set(left), set(right), "an empty root adds no different files")
        for key in left:
            if key.endswith("osprey.toml"):
                continue  # the manifests differ only by the spelling under test
            self.assertEqual(left[key], right[key], key)

    def test_a_suite_with_an_empty_source_root_collects(self):
        fixture = self.workbench(suite_manifest=suite_manifest(".", ""), oracle=None)
        self.assert_exit(fixture.collect("--project", "apps/workbench"), 0)
        self.assertEqual(fixture.result("apps/workbench")["covered"], 9)


class SuiteManifestSerializationTests(CoverageTestCase):
    """The shared fixture writes manifests a real TOML reader accepts."""

    def test_hostile_roots_round_trip_through_a_real_toml_reader(self):
        manifest = tomllib.loads(suite_manifest(*HOSTILE_ROOTS))
        self.assertEqual(manifest["project"]["source_roots"], list(HOSTILE_ROOTS))

    def test_ordinary_roots_are_unchanged_by_the_serialization(self):
        manifest = tomllib.loads(suite_manifest(".", "../src", "../../examples/proof"))
        self.assertEqual(manifest["project"]["source_roots"],
                         [".", "../src", "../../examples/proof"])


if __name__ == "__main__":
    unittest.main()
