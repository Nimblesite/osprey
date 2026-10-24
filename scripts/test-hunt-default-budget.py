#!/usr/bin/env python3
"""Real-engine regressions for the existing 64-byte, 30-second Hunt gate."""
import json
from pathlib import Path
import subprocess
import unittest

from hunt_expect import actual_findings, expected_findings, validate


class DefaultBudgetTests(unittest.TestCase):
    def test_clean_and_crash_finish_with_default_64_bytes_in_30_seconds(self):
        root = Path(__file__).resolve().parent.parent
        (root/'target/hunt-default-budget-regression').mkdir(parents=True, exist_ok=True)
        for name in ('clean', 'crash'):
            with self.subTest(program=name):
                out = root/'target/hunt-default-budget-regression'/name
                process = subprocess.run([
                    str(root/'target/release/osprey'), 'hunt', f'tests/hunt/{name}.osp',
                    '--json', '--quiet', '--no-replay', '--max-time', '30', '--out', str(out),
                ], cwd=root, capture_output=True, text=True, timeout=150)
                self.assertTrue(process.stdout.strip(), process.stderr)
                report = json.loads(process.stdout)
                validate(report)
                self.assertEqual(actual_findings(report), expected_findings(root/f'tests/hunt/{name}.expected'))
                self.assertEqual(process.returncode, int(bool(report['findings'])))


if __name__ == '__main__':
    unittest.main()
