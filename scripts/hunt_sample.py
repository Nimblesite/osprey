#!/usr/bin/env python3
"""Measure a bounded Hunt corpus; complete runs qualify, never merely exit 0 [SYMEX-CORPUS-QUICK]."""
import argparse
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

from hunt_programs import ROOTS, discover
from hunt_expect import validate


def qualifies(record):
    report = record.get('report') or {}
    try:
        validate(report)
    except (KeyError, ValueError, TypeError, AttributeError):
        return False
    explored = report['explored']
    paths, completed = explored.get('paths', 0), explored.get('completed', 0)
    return (record.get('exitCode') == 0 and not record.get('timedOut')
            and record.get('wallSeconds', 11) <= 10 and isinstance(paths, int) and paths > 0
            and completed == paths and report.get('findings') == [] and report.get('incomplete') == [])


def stop_group(child):
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass  # The child can finish between the deadline and killpg.
    child.wait()


def wait(child, options, start):
    try:
        code = child.wait(timeout=max(0, options.wall_time-(time.monotonic()-start)))
        return code, False
    except subprocess.TimeoutExpired:
        stop_group(child)
        return 124, True


def execute(command, options, folder, stdin=None):
    start = time.monotonic()
    with ExitStack() as stack:
        stdout = stack.enter_context((folder/'stdout.log').open('w'))
        stderr = stack.enter_context((folder/'stderr.log').open('w'))
        source = stack.enter_context(stdin.open('rb')) if stdin else subprocess.DEVNULL
        child = subprocess.Popen(command, cwd=options.root, stdin=source,
                                 stdout=stdout, stderr=stderr, start_new_session=True)
        code, timed_out = wait(child, options, start)
    return code, timed_out, round(time.monotonic()-start, 3)


def read_report(folder):
    path = folder/'engine/report.json'
    try:
        report = json.loads(path.read_text())
        return {key: report[key] for key in ('program', 'findings', 'incomplete', 'explored')}
    except (OSError, ValueError, KeyError, TypeError) as error:
        return {'unavailable': str(error)}


def run_one(program, options):
    slug = program.replace('/', '_')[:120]+'-'+hashlib.sha256(program.encode()).hexdigest()[:8]
    folder = options.out/slug
    folder.mkdir(parents=True)
    command = [str(options.osprey), 'hunt', program, '--max-time', str(options.max_time),
               '--input-bytes', str(options.input_bytes), '--out', str(folder/'engine')]
    (folder/'command.json').write_text(json.dumps(command, indent=2)+'\n')
    code, timed_out, elapsed = execute(command, options, folder)
    record = {'program': program, 'directory': str(folder), 'exitCode': code,
              'timedOut': timed_out, 'wallSeconds': elapsed, 'report': read_report(folder)}
    record['qualifies'] = qualifies(record)
    record['planted'] = program in getattr(options, 'expected_crashes', ())
    (folder/'run.json').write_text(json.dumps(record, indent=2)+'\n')
    return record


def parse():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, default=Path('target/hunt-sample'))
    parser.add_argument('--osprey', type=Path, default=Path('target/release/osprey'))
    parser.add_argument('--wall-time', type=float, default=10)
    parser.add_argument('--max-time', type=int, default=8)
    parser.add_argument('--input-bytes', type=int, default=2)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--manifest', type=Path, help='rerun a safe-runs.json; fail if any program no longer qualifies')
    parser.add_argument('programs', nargs='*')
    options = parser.parse_args()
    apply_manifest(options, parser)
    if not 0 < options.wall_time <= 10 or options.jobs < 1 or options.max_time < 1:
        parser.error('wall time must be positive and at most 10 seconds; jobs and engine budget must be positive')
    options.root, options.out = Path.cwd(), options.out.resolve()
    options.osprey = options.osprey.resolve()
    return options


def apply_manifest(options, parser):
    if not options.manifest:
        return
    try:
        manifest = json.loads(options.manifest.read_text())
        programs, limits = manifest['programs'], manifest['limits']
        if type(manifest['version']) is not int or manifest['version'] != 1 or not isinstance(programs, list) or not programs or options.programs:
            raise ValueError('expected version 1, a nonempty manifest, and no extra programs')
        if not all(isinstance(p, str) and p for p in programs) or len(set(programs)) != len(programs):
            raise ValueError('manifest programs must be unique nonempty paths')
        options.programs = programs
        options.wall_time, options.max_time, options.input_bytes = (limits['wallTimeSeconds'], limits['engineTimeSeconds'], limits['inputBytesPerStream'])
        if type(options.wall_time) not in (int, float) or not 0 < options.wall_time <= 10:
            raise ValueError('wall deadline must be positive and at most 10 seconds')
        if any(type(v) is not int for v in (options.max_time, options.input_bytes)) or not 0 <= options.input_bytes <= 4096:
            raise ValueError('invalid engine budget or input bound')
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(f'invalid safe-run manifest: {error}')


def snapshot(options):
    folder = options.out/'toolchain'
    folder.mkdir(parents=True)
    shutil.copy2(options.osprey, folder/'osprey')
    options.osprey = folder/'osprey'
    runtime = options.root/'compiler/bin/libosprey_runtime_klee.bc'
    shutil.copy2(runtime, folder/runtime.name)
    os.environ['OSPREY_KLEE_RUNTIME'] = str(folder/runtime.name)
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in folder.iterdir()}


def main():
    options = parse()
    programs = options.programs or discover(options.root)
    cases = json.loads((options.root/'tests/hunt/crash-cases.json').read_text())
    options.expected_crashes = {case['program'] for case in cases}
    options.out.mkdir(parents=True, exist_ok=True)
    fingerprints = snapshot(options)
    metadata = {'programs': programs, 'roots': ROOTS, 'wallTime': options.wall_time,
                'engineBudget': options.max_time, 'inputBytes': options.input_bytes,
                'jobs': options.jobs, 'fingerprints': fingerprints}
    (options.out/'selection.json').write_text(json.dumps(metadata, indent=2)+'\n')
    records = collect(programs, options)
    (options.out/'results.json').write_text(json.dumps({'metadata': metadata, 'runs': records}, indent=2)+'\n')
    from hunt_sample_report import write
    write(options.out, records)
    return int(bool(options.manifest) and not all(record['qualifies'] for record in records))


def collect(programs, options):
    records = []
    with ThreadPoolExecutor(max_workers=options.jobs) as pool:
        futures = [pool.submit(run_one, program, options) for program in programs]
        for future in as_completed(futures):
            record = future.result()
            records.append(record)
            print(f"{len(records)}/{len(programs)} {record['program']}: "
                  f"{'quick candidate' if record['qualifies'] else 'excluded'} ({record['wallSeconds']}s)", flush=True)
    return sorted(records, key=lambda record: record['program'])


if __name__ == '__main__':
    raise SystemExit(main())
