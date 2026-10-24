#!/usr/bin/env python3
"""Real-engine planted failures must be found AND reproduced [SYMEX-CRASH-GATE]."""
import argparse
import json
from pathlib import Path
import sys
import time

from hunt_expect import actual_findings, expected_findings, validate
from hunt_sample import execute, run_one


def check_case(record, case, root):
    try:
        report = record['report']
        validate(report)
        if report['explored']['completed'] < 1:
            return 'planted input case must also have a normal path'
        if record['timedOut'] or record['exitCode'] != 1:
            return 'expected a bounded native crash (exit 1)'
        if actual_findings(report) != expected_findings(root/case['expected']):
            return 'finding class or source location does not match the planted fault'
        if len(report['findings']) != len(case['cases']):
            return 'one or more planted input cases were missed'
        for finding in report['findings']:
            expected = next((item for item in case['cases'] if item['stdin'] == finding.get('stdin')), None)
            if expected is None:
                return 'unexpected trigger input'
            error = check_replay(finding, expected)
            if error:
                return error
        return '' if report['findings'] else 'planted crash was missed'
    except (KeyError, TypeError, ValueError, OSError) as error:
        return str(error)


def check_replay(finding, case):
    if finding.get('replay') != 'reproduced' or finding.get('replayDetail') not in case['signals']:
        return 'finding did not reproduce as the expected native signal'
    if 'stdin' in case and finding.get('stdin') != case['stdin']:
        return 'wrong concrete stdin for the planted fault'
    if 'file' in case and case['file'] not in finding.get('files', []):
        return 'file input that triggers the planted fault is missing'
    return ''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, default=Path('target/hunt-crashes'))
    parser.add_argument('--osprey', type=Path, default=Path('target/release/osprey'))
    args = parser.parse_args()
    cases = json.loads(Path('tests/hunt/crash-cases.json').read_text())
    options = argparse.Namespace(root=Path.cwd(), out=args.out.resolve()/f"run-{time.time_ns()}", osprey=args.osprey.resolve(),
                                 wall_time=10, max_time=8, input_bytes=1)
    print(f'Crash reports: {options.out}', flush=True)
    results = [verify(case, options) for case in cases]
    passed = sum(record['assertionsPassed'] for record in results)
    print(f'{passed}/{len(results)} programs passed; {sum(len(case["cases"]) for case in cases)} planted input cases')
    (options.out/'results.json').write_text(json.dumps(results, indent=2)+'\n')
    from hunt_sample_report import write
    write(options.out, results, proof=True)
    return 0 if passed == len(results) else 1


def benign(case, record, options):
    folder = Path(record['directory'])/'benign'
    folder.mkdir()
    source = folder/'stdin.bin'
    source.write_bytes(b'Z')
    command = [str(options.osprey), case['program'], '--run']
    (folder/'command.json').write_text(json.dumps(command, indent=2)+'\n')
    code, timed_out, elapsed = execute(command, options, folder, stdin=source)
    record['benign'] = {'exitCode': code, 'timedOut': timed_out, 'wallSeconds': elapsed, 'stdin': 'Z'}
    expected = 'safe\n' if case['name'] in ('invalid_free', 'double_free') else 'result 0\n'
    return '' if code == 0 and not timed_out and (folder/'stdout.log').read_text() == expected else 'benign input Z did not finish with the expected output'


def verify(case, options):
    record = run_one(case['program'], options)
    error = check_case(record, case, options.root)
    error = error or benign(case, record, options)
    record.update(expectedCrash=True, assertionsPassed=not error, assertionError=error, case=case['name'])
    (Path(record['directory'])/'run.json').write_text(json.dumps(record, indent=2)+'\n')
    print(f"{case['name']}: {'FAIL: '+error if error else 'PASS'}", flush=True)
    return record


if __name__ == '__main__':
    sys.exit(main())
