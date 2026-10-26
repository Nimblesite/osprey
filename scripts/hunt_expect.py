#!/usr/bin/env python3
"""Checks one `osprey hunt --json` report against a `.expected` file: the
deterministic main-mode programs under tests/hunt/ [SYMEX-CLI]
(docs/specs/tooling/SymbolicExecution.md, delivery checklist).

    scripts/hunt_expect.py <report.json> <program.expected> [process-status]

The expected file lists one finding per line as `<class> <file>:<line>`, or
holds the single word `none`. Blank lines and `#` comments are ignored. The
replay verdict is not compared: it depends on the host, the findings do not.
Exit 0 only for a complete exploration whose findings and optional no-replay
process status match the expected set.
"""
import json
import pathlib
import sys


def expected_findings(path):
    lines = [line.split("#", 1)[0].strip() for line in path.read_text().splitlines()]
    lines = [line for line in lines if line]
    if lines == ["none"]:
        return set()
    return set(lines)


def actual_findings(report):
    found = set()
    for finding in report["findings"]:
        if finding.get("file") is None:
            found.add(f"{finding['class']} {finding.get('runtimeFrame') or finding['test']}")
        else:
            found.add(f"{finding['class']} {pathlib.Path(finding['file']).name}:{finding['line']}")
    return found


def validate(report):
    paths, completed = report["explored"]["paths"], report["explored"]["completed"]
    if type(paths) is not int or type(completed) is not int or not 0 <= completed <= paths or paths == 0:
        raise ValueError("invalid or empty exploration")
    if not isinstance(report["findings"], list) or not isinstance(report["incomplete"], list):
        raise ValueError("invalid findings or incompleteness")
    if report["incomplete"] or (not report["findings"] and completed != paths):
        raise ValueError("incomplete exploration")


def main(argv):
    report_path, expected_path = pathlib.Path(argv[1]), pathlib.Path(argv[2])
    try:
        report = json.loads(report_path.read_text())
        validate(report)
        actual = actual_findings(report)
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"{expected_path.stem}: no report ({error})")
        return 1
    expected = expected_findings(expected_path)
    if len(argv) > 3 and argv[3] != str(int(bool(report["findings"]))):
        print(f"{expected_path.stem}: unexpected process status {argv[3]}")
        return 1
    if expected == actual:
        print(f"{expected_path.stem}: ok ({len(actual)} finding(s), {report['explored']['paths']} paths)")
        return 0
    for missing in sorted(expected - actual):
        print(f"{expected_path.stem}: missing  {missing}")
    for extra in sorted(actual - expected):
        print(f"{expected_path.stem}: unexpected  {extra}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
