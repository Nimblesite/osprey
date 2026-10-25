#!/usr/bin/env python3
"""Quick-corpus qualification must reject partial runs and kill whole process groups."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from hunt_sample import execute, qualifies
from hunt_programs import discover


class InventoryTests(unittest.TestCase):
    def test_projects_are_one_program_and_libraries_and_rejections_are_not_apps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            files = {'examples/app/osprey.toml': '[project]\nentry="main.py"\nsource_roots=[".","../shared"]',
                     'examples/app/main.py': '# osprey: flavor=python\nprint(1)',
                     'examples/app/helper.osp': 'print(2)', 'examples/shared/lib.osp': 'fn id(x) = x', 'examples/library/osprey.toml': '[project]',
                     'examples/library/lib.osp': 'fn id(x) = x', 'examples/one.osp': 'print(1)',
                     'examples/one.ospml': 'print 1', 'examples/py.py': '# osprey: flavor=python\nprint(1)',
                     'examples/tool.py': 'print(3)', 'examples/failscompilation/bad.osp': 'bad'}
            for name, text in files.items():
                path = root/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            self.assertEqual(discover(root, ['examples']), ['examples/app', 'examples/one.osp', 'examples/one.ospml', 'examples/py.py'])


class QualificationTests(unittest.TestCase):
    def clean(self):
        return {'exitCode': 0, 'timedOut': False, 'wallSeconds': 1,
                'report': {'explored': {'paths': 3, 'completed': 3}, 'findings': [], 'incomplete': []}}

    def test_only_completed_bounded_finding_free_runs_qualify(self):
        self.assertTrue(qualifies(self.clean()))
        for patch in [{'exitCode': 1}, {'exitCode': -11}, {'timedOut': True}, {'wallSeconds': 11}, {'report': None}]:
            self.assertFalse(qualifies(self.clean() | patch), patch)
        for patch in [{'explored': {'paths': 3, 'completed': 2}}, {'explored': {'paths': 0, 'completed': 0}},
                      {'explored': {'paths': True, 'completed': True}},
                      {'explored': {'paths': 1, 'completed': True}},
                      {'findings': [{'class': 'unmodelled'}]}, {'incomplete': [{'reason': 'time'}]}]:
            value = self.clean()
            value['report'].update(patch)
            self.assertFalse(qualifies(value), patch)

    @unittest.skipUnless(os.name == 'posix', 'process-group contract is POSIX')
    def test_deadline_kills_the_child_group_and_preserves_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            options = argparse.Namespace(root=root, wall_time=1)
            code, timed_out, elapsed = execute(['/bin/sh', '-c', 'sleep 60 &\necho $!\nwait\n'], options, root)
            self.assertEqual(code, 124)
            self.assertTrue(timed_out)
            self.assertLess(elapsed, 3)
            pid = int((root/'stdout.log').read_text())
            time.sleep(0.1)
            state = subprocess.run(['ps', '-o', 'stat=', '-p', str(pid)], capture_output=True, text=True).stdout.strip()
            self.assertTrue(not state or state.startswith('Z'), state)


class CrashGateTests(unittest.TestCase):
    def test_unconfirmed_skipped_wrong_input_and_wrong_signal_cannot_pass(self):
        from hunt_crashes import check_replay
        case = {'signals': ['SIGABRT'], 'stdin': '9'}
        base = {'replay': 'reproduced', 'replayDetail': 'SIGABRT', 'stdin': '9'}
        self.assertEqual(check_replay(base, case), '')
        for patch in [{'replay': 'klee-only'}, {'replay': 'skipped'}, {'replay': 'not-run'},
                      {'replayDetail': 'SIGSEGV'}, {'stdin': '0'}]:
            self.assertTrue(check_replay(base | patch, case), patch)

    def test_a_planted_case_must_also_have_a_normal_path(self):
        from hunt_crashes import check_case
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'case.expected').write_text('abort main.osp:4\n')
            finding = {'class': 'abort', 'file': 'main.osp', 'line': 4, 'stdin': '9',
                       'replay': 'reproduced', 'replayDetail': 'SIGABRT'}
            case = {'expected': 'case.expected', 'cases': [{'stdin': '9', 'signals': ['SIGABRT']}]}
            record = {'exitCode': 1, 'timedOut': False, 'report': {
                'explored': {'paths': 4, 'completed': 3}, 'findings': [finding], 'incomplete': []}}
            self.assertEqual(check_case(record, case, root), '')
            record['report']['explored']['completed'] = 0
            self.assertIn('normal path', check_case(record, case, root))

    def test_every_planted_input_is_pinned_and_has_a_real_program(self):
        root = Path(__file__).resolve().parent.parent
        cases = json.loads((root/'tests/hunt/crash-cases.json').read_text())
        self.assertEqual(sum(len(case['cases']) for case in cases), 15)
        inputs = {item['stdin'] for case in cases for item in case['cases']}
        self.assertEqual(inputs, set('9PALKCRUBJFGDY'))
        for case in cases:
            self.assertTrue((root/case['program']).exists(), case)
            self.assertTrue((root/case['expected']).is_file(), case)
            self.assertTrue(all(item['signals'] for item in case['cases']), case)

    def test_real_engine_gate_is_required_by_existing_ci_contexts(self):
        root = Path(__file__).resolve().parent.parent
        workflow = (root/'.github/workflows/ci.yml').read_text()
        self.assertIn('make _test_hunt_crashes', workflow)
        self.assertIn('brew install klee', workflow)
        self.assertIn('make _test_hunt_mock', workflow)
        self.assertIn('--manifest tests/hunt/safe-runs.json', workflow)
        manifest = json.loads((root/'tests/hunt/safe-runs.json').read_text())
        self.assertGreaterEqual(len(manifest['programs']), 215)
        self.assertEqual(len(manifest['programs']), len(set(manifest['programs'])))
        self.assertTrue(all((root/program).is_file() or (root/program/'osprey.toml').is_file() for program in manifest['programs']))
        self.assertEqual(manifest['limits'], {'wallTimeSeconds': 10, 'engineTimeSeconds': 8, 'inputBytesPerStream': 2})


class ReviewReportTests(unittest.TestCase):
    def test_manifest_gate_rejects_empty_lists_and_widened_deadlines(self):
        sampler = Path(__file__).resolve().parent/'hunt_sample.py'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'manifest.json'
            manifest = {'version': 1, 'programs': ['safe.osp'], 'limits': {'wallTimeSeconds': 10, 'engineTimeSeconds': 8, 'inputBytesPerStream': 2}}
            for patch in [{'programs': []}, {'programs': ['safe.osp', 'safe.osp']},
                          {'limits': manifest['limits'] | {'wallTimeSeconds': 11}}]:
                path.write_text(json.dumps(manifest | patch))
                result = subprocess.run([sys.executable, str(sampler), '--manifest', str(path)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, result.stderr)

    def test_manifest_gate_rejects_a_run_that_becomes_incomplete(self):
        sampler = Path(__file__).resolve().parent/'hunt_sample.py'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_sampler_root(root)
            manifest = {'version': 1, 'limits': {'wallTimeSeconds': 10, 'engineTimeSeconds': 8, 'inputBytesPerStream': 2}, 'programs': ['safe.osp']}
            for program, expected in [('safe.osp', 0), ('partial.osp', 1)]:
                manifest['programs'] = [program]
                (root/'manifest.json').write_text(json.dumps(manifest))
                result = subprocess.run([sys.executable, str(sampler), '--manifest', str(root/'manifest.json'),
                    '--osprey', str(root/'osprey'), '--out', str(root/program)], cwd=root, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_concise_report_exports_only_complete_clean_runs_as_json(self):
        from hunt_sample_report import write
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            good = QualificationTests().clean() | {'program': 'safe.osp', 'qualifies': True, 'directory': str(out/'safe')}
            bad = good | {'program': 'unfinished.osp', 'report': good['report'] | {'incomplete': [{'reason': 'time'}]}}
            write(out, [good, bad])
            manifest = json.loads((out/'safe-runs.json').read_text())
            self.assertEqual(manifest['programs'], ['safe.osp'])
            self.assertIn('limits', manifest)
            text = (out/'index.html').read_text()
            self.assertIn('<th scope="col">Run</th>', text)
            self.assertIn('<th scope="col">Issues</th>', text)
            self.assertNotIn('What is asserted', text)
            self.assertNotIn('How to read the counts', text)

    def test_report_separates_program_bugs_from_engine_limitations(self):
        from hunt_sample_report import issues, run_status
        base = QualificationTests().clean()
        limited = base | {'report': base['report'] | {'findings': [{'class': 'unmodelled'}]}}
        crash = base | {'exitCode': 1, 'report': base['report'] | {'findings': [{'class': 'abort', 'replay': 'reproduced'}]}}
        self.assertEqual(issues(base), 'None found')
        self.assertEqual(issues(limited), 'Not fully checked')
        self.assertEqual(run_status(limited), 'Engine limitation')
        self.assertEqual(issues(crash), 'BUG: crash reproduced')
        self.assertEqual(issues(crash | {'planted': True}), 'Expected planted crash')
        self.assertEqual(run_status(base | {'timedOut': True}), 'Timed out')

    def test_both_review_modes_render_escape_names_and_separate_counts(self):
        from hunt_sample_report import write
        for proof in [False, True]:
            with tempfile.TemporaryDirectory() as directory:
                out = Path(directory)
                record = {'program': '<script>bad()</script>', 'directory': str(out/'case'),
                          'exitCode': 0, 'timedOut': False, 'wallSeconds': 1, 'qualifies': not proof,
                          'expectedCrash': proof, 'assertionsPassed': proof, 'report': {
                              'explored': {'paths': 4, 'completed': 3}, 'findings': [], 'incomplete': []}}
                write(out, [record], proof=proof)
                text = (out/'index.html').read_text()
                self.assertIn('Paths explored', text)
                self.assertIn('Normal exits', text)
                self.assertIn('&lt;script&gt;', text)
                self.assertNotIn('<script>', text)
                self.assertEqual((out/'quick-corpus.txt').exists(), not proof)


def fake_sampler_root(root):
    (root/'compiler/bin').mkdir(parents=True)
    (root/'compiler/bin/libosprey_runtime_klee.bc').write_bytes(b'fake runtime')
    (root/'tests/hunt').mkdir(parents=True)
    (root/'tests/hunt/crash-cases.json').write_text('[]')
    script = root/'osprey'
    script.write_text(f'''#!{sys.executable}
import json, pathlib, sys
out = pathlib.Path(sys.argv[sys.argv.index('--out')+1])
out.mkdir()
partial = sys.argv[2] == 'partial.osp'
report = {{'program': sys.argv[2], 'explored': {{'paths': 1, 'completed': int(not partial)}}, 'findings': [], 'incomplete': [{{'reason':'time'}}] if partial else []}}
(out/'report.json').write_text(json.dumps(report))
''')
    script.chmod(0o755)


if __name__ == '__main__':
    unittest.main()
