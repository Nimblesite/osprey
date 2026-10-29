#!/usr/bin/env python3
"""Regressions for `apps/` gating and `--project` selection in the coverage gate.

Every shipped app is covered by the same 90% floor as a package, `--project`
collects exactly one project, an ungated app fails the inventory before any
suite runs, and an app report can never be mistaken for a package report.
"""
import json
import unittest
from package_coverage_test_support import (
    ROOT, CoverageTestCase, add_package, own_report, py_source, write_json,
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


if __name__ == "__main__":
    unittest.main()
