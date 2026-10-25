#!/usr/bin/env python3
"""Regressions for independent package coverage thresholds and missing evidence."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PackageThresholdTests(unittest.TestCase):
    def test_every_shipped_package_has_an_individual_threshold_of_at_least_90(self):
        config = json.loads((ROOT / "coverage-thresholds.json").read_text())
        packages = config["projects"].get("packages", {})
        shipped = {p.name for p in (ROOT / "packages").iterdir() if (p / "osprey.toml").is_file()}
        self.assertEqual(set(packages), shipped, "Every package must have its own coverage gate")
        for name, entry in packages.items():
            self.assertGreaterEqual(entry["threshold"], 90, name)


if __name__ == "__main__":
    unittest.main()
