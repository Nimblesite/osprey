#!/usr/bin/env python3
"""Run the Python-flavor application ports and check their golden transcripts.

Each application under examples/python/ports is a complete Osprey project with
its own manifest and entry. This harness runs it with the compiler under the
requested memory backend, compares stdout byte-for-byte with the app's
expected_output.txt, and requires exit status 0. Under ARC it also requires
exactly one `[osp-arc] exit: 0 live objects` sentinel on stderr.

    python3 scripts/test-python-app-ports.py --compiler target/release/osprey
    python3 scripts/test-python-app-ports.py --memory gc
    OSPREY_ARC_DEBUG=1 python3 scripts/test-python-app-ports.py --memory arc

The expected transcripts are captured from real compiler runs:

    target/release/osprey examples/python/ports/classifier --run > examples/python/ports/classifier/expected_output.txt

Use --write-expected only to refresh a transcript after an intentional output
change; review the diff before committing it.
"""

from __future__ import annotations

import argparse
import difflib
import os
import re
import subprocess
import sys
from pathlib import Path
from package_coverage_evidence import oracle_problems

APPS = ("classifier", "vector-search", "text-evaluation")
MEMORIES = ("default", "gc", "arc")
ARC_PATTERN = re.compile(r"^\[osp-arc\] exit: (\d+) live objects.*$", re.MULTILINE)
TIMEOUT_SECONDS = 300


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--compiler", default=str(Path("target") / "release" / "osprey"), help="compiler binary to run")
    parser.add_argument("--memory", choices=(*MEMORIES, "all"), default="all", help="memory backend, or all three (default)")
    parser.add_argument("--app", choices=APPS, action="append", help="only this app; repeatable (default: all)")
    parser.add_argument("--write-expected", action="store_true", help="refresh expected_output.txt from a real run")
    return parser.parse_args()


def app_dir(root: Path, name: str) -> Path:
    return root / "examples" / "python" / "ports" / name


def run_app(compiler: Path, directory: Path, memory: str) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    if memory == "arc":
        env["OSPREY_ARC_DEBUG"] = "1"
    command = [str(compiler), str(directory), "--run", f"--memory={memory}"]
    result = subprocess.run(command, cwd=directory.parents[3], env=env, capture_output=True, timeout=TIMEOUT_SECONDS)
    result.stdout = result.stdout.decode("utf-8")
    result.stderr = result.stderr.decode("utf-8")
    return result


def arc_live_count(stderr: str) -> int | None:
    matches = ARC_PATTERN.findall(stderr)
    if len(matches) != 1:
        return None
    return int(matches[0])


def diff_text(got: str, want: str, name: str) -> str:
    return "".join(difflib.unified_diff(want.splitlines(keepends=True), got.splitlines(keepends=True), fromfile=f"{name}/expected_output.txt", tofile=f"{name}/actual"))


def result_problems(result, name, memory):
    failures = [f"{name}/{memory}: {problem}" for problem in oracle_problems(result.stdout)]
    if result.returncode != 0:
        failures.append(f"{name}/{memory}: exit {result.returncode}\n{result.stderr.strip()}")
    if memory == "arc":
        live = arc_live_count(result.stderr)
        if live != 0:
            failures.append(f"{name}/{memory}: expected one '[osp-arc] exit: 0 live objects' sentinel, got {live!r}\n{result.stderr.strip()}")
    return failures


def check_one(compiler: Path, root: Path, name: str, memory: str, write_expected: bool) -> list[str]:
    result = run_app(compiler, app_dir(root, name), memory)
    failures = result_problems(result, name, memory)
    expected_path = app_dir(root, name) / "expected_output.txt"
    if write_expected:
        if not failures:
            expected_path.write_bytes(result.stdout.encode("utf-8"))
        return failures
    want = expected_path.read_bytes().decode("utf-8")
    if result.stdout != want:
        failures.append(f"{name}/{memory}: stdout differs\n{diff_text(result.stdout, want, name)}")
    return failures


def check_safely(compiler, root, name, memory, write_expected):
    try:
        failures = check_one(compiler, root, name, memory, write_expected)
        if not failures:
            print(f"PASS {name}/{memory}")
        return failures
    except subprocess.TimeoutExpired:
        return [f"{name}/{memory}: timed out after {TIMEOUT_SECONDS}s"]
    except (OSError, UnicodeError) as error:
        return [f"{name}/{memory}: cannot verify output: {error}"]


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    compiler = Path(args.compiler).resolve()
    if not compiler.is_file():
        print(f"compiler not found: {compiler}", file=sys.stderr)
        return 2
    if args.write_expected and args.memory != "default":
        print("Refresh goldens with --memory default; verify other backends against that golden.", file=sys.stderr)
        return 2
    apps = tuple(args.app) if args.app else APPS
    memories = MEMORIES if args.memory == "all" else (args.memory,)
    failures = [problem for name in apps for memory in memories
                for problem in check_safely(compiler, root, name, memory, args.write_expected)]
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
