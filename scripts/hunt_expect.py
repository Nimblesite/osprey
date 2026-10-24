#!/usr/bin/env python3
"""Checks one `osprey hunt --json` report against a `.expected` file: the
deterministic main-mode programs under tests/hunt/ [SYMEX-CLI]
(docs/specs/tooling/SymbolicExecution.md, delivery checklist).

    scripts/hunt_expect.py <report.json> <program.expected>

The expected file lists one finding per line as `<class> <file>:<line>`, or
holds the single word `none`. Blank lines and `#` comments are ignored. The
replay verdict is not compared: it depends on the host, the findings do not.
Exit 0 when the report's findings are exactly the expected set.
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


def main(argv):
    report_path, expected_path = pathlib.Path(argv[1]), pathlib.Path(argv[2])
    try:
        report = json.loads(report_path.read_text())
    except (OSError, ValueError) as error:
        print(f"{expected_path.stem}: no report ({error})")
        return 1
    expected = expected_findings(expected_path)
    actual = actual_findings(report)
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
