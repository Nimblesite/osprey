#!/usr/bin/env python3
"""Measure a bounded Hunt corpus; complete runs qualify, never merely exit 0 [SYMEX-CORPUS-QUICK]."""
import argparse
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


def qualifies(record):
    report = record.get('report') or {}
    explored = report.get('explored') or {}
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


def execute(command, options, folder):
    start = time.monotonic()
    with (folder/'stdout.log').open('w') as stdout, (folder/'stderr.log').open('w') as stderr:
        child = subprocess.Popen(command, cwd=options.root, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            code = child.wait(timeout=max(0, options.wall_time-(time.monotonic()-start)))
            timed_out = False
        except subprocess.TimeoutExpired:
            stop_group(child)
            code, timed_out = 124, True
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
    parser.add_argument('programs', nargs='*')
    options = parser.parse_args()
    if not 0 < options.wall_time <= 10 or options.jobs < 1 or options.max_time < 1:
        parser.error('wall time must be positive and at most 10 seconds; jobs and engine budget must be positive')
    options.root, options.out = Path.cwd(), options.out.resolve()
    options.osprey = options.osprey.resolve()
    return options


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
    main()
