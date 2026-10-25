#!/usr/bin/env python3
"""Real-engine regressions for the existing 64-byte, 30-second Hunt gate."""
import json
from pathlib import Path
import subprocess
import sys
import time
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

    def test_safe_manifest_gate_rejects_a_real_reproduced_crash(self):
        root = Path(__file__).resolve().parent.parent
        out = root/'target/hunt-manifest-regression'/f'run-{time.time_ns()}'
        out.mkdir(parents=True, exist_ok=True)
        programs = ['tests/hunt/json_empty.osp', 'tests/hunt/crash.osp']
        manifest = {'version': 1, 'programs': programs, 'limits': {'wallTimeSeconds': 10, 'engineTimeSeconds': 8, 'inputBytesPerStream': 2}}
        (out/'manifest.json').write_text(json.dumps(manifest))
        process = subprocess.run([sys.executable, str(root/'scripts/hunt_sample.py'), '--manifest', str(out/'manifest.json'), '--out', str(out/'runs'), '--jobs', '1'], cwd=root, capture_output=True, text=True, timeout=45)
        self.assertEqual(process.returncode, 1, process.stdout + process.stderr)
        records = {record['program']: record for record in json.loads((out/'runs/results.json').read_text())['runs']}
        self.assertTrue(records[programs[0]]['qualifies'], records[programs[0]])
        finding = records[programs[1]]['report']['findings']
        self.assertEqual(len(finding), 1)
        self.assertEqual(finding[0]['replay'], 'reproduced')
        self.assertEqual(finding[0]['stdin'], '9')
        self.assertEqual(actual_findings(records[programs[1]]['report']), expected_findings(root/'tests/hunt/crash.expected'))
        self.assertEqual(json.loads((out/'runs/safe-runs.json').read_text())['programs'], [programs[0]])
        self.assertTrue((out/'runs/index.html').is_file())


if __name__ == '__main__':
    unittest.main()
