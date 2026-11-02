#!/usr/bin/env python3
"""Fail when Osprey's instruction count rises against checked Rust.

Wall-clock time on hosted runners is too noisy to gate on. Cachegrind's instruction
count depends only on the binary and its input, so the Osprey / `rust-checked` ratio of
each case is compared with instruction-ratios.json, which only goes down. See plan 0033.

Usage: instruction_gate.py [--record]
  --record  lower the recorded ratios to today's and add any missing case.
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

BENCHMARKS = Path(__file__).resolve().parent
OSPREY = BENCHMARKS.parent / "target" / "release" / "osprey"
RATIOS = BENCHMARKS / "instruction-ratios.json"
CASES = ("collatz", "coprime", "digitsum", "fib", "gcdsum", "hanoi", "isqrt", "tak")
# Cachegrind differs by well under 1% between runs and between runners.
TOLERANCE = 1.01
INSTRUCTIONS = re.compile(r"I\s+refs:\s+([\d,]+)")


def parse_instructions(report: str) -> int:
    """The instruction count in a cachegrind report."""
    found = INSTRUCTIONS.search(report)
    if found is None:
        raise ValueError(f"no instruction count in cachegrind output:\n{report}")
    return int(found.group(1).replace(",", ""))


def regressions(measured: dict[str, float], recorded: dict[str, float]) -> list[str]:
    """Each case with no recorded ratio or above it."""
    problems = []
    for case, ratio in sorted(measured.items()):
        limit = recorded.get(case)
        if limit is None:
            problems.append(f"{case}: {ratio:.3f} has no recorded ratio; run --record")
        elif ratio > limit * TOLERANCE:
            problems.append(f"{case}: {ratio:.3f} is above the recorded {limit:.3f}")
    return problems


def lowered(measured: dict[str, float], recorded: dict[str, float]) -> dict[str, float]:
    """The ratios to record: never higher than a ratio already recorded."""
    return {
        case: round(min(ratio, recorded.get(case, ratio)), 3)
        for case, ratio in sorted(measured.items())
    }


def count_instructions(binary: Path) -> int:
    """Run `binary` under cachegrind without simulating caches."""
    result = subprocess.run(
        ["valgrind", "--tool=cachegrind", "--cache-sim=no", "--cachegrind-out-file=/dev/null", str(binary)],
        capture_output=True,
        text=True,
        check=True,
    )
    return parse_instructions(result.stderr)


def measure(case: str, build: Path) -> float:
    """The Osprey / rust-checked instruction ratio of one benchmark case."""
    source = BENCHMARKS / "cases" / case
    osprey = build / f"{case}-osprey"
    rust = build / f"{case}-rust"
    subprocess.run(
        [str(OSPREY), str(source / f"{case}.osp"), "--compile", "--quiet", "--no-warnings", "-o", str(osprey)],
        check=True,
    )
    subprocess.run(
        ["rustc", "-C", "opt-level=3", "-C", "overflow-checks=on", "-o", str(rust), str(source / f"{case}.rs")],
        check=True,
    )
    return count_instructions(osprey) / count_instructions(rust)


def main() -> int:
    recorded = json.loads(RATIOS.read_text()) if RATIOS.exists() else {}
    with tempfile.TemporaryDirectory() as build:
        measured = {case: measure(case, Path(build)) for case in CASES}
    for case, ratio in sorted(measured.items()):
        print(f"{case}: {ratio:.3f} (recorded {recorded.get(case, 'none')})")
    if "--record" in sys.argv:
        RATIOS.write_text(json.dumps(lowered(measured, recorded), indent=2) + "\n")
        return 0
    problems = regressions(measured, recorded)
    print("\n".join(problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
