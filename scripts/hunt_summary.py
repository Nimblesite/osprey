#!/usr/bin/env python3
"""Summarises a scripts/hunt_corpus.sh run as a Markdown table [SYMEX-CORPUS].

    scripts/hunt_summary.py target/hunt-corpus [--findings-only]

Each program is one row: exit code, paths explored, findings (class at
Osprey file:line with the replay verdict) and the first stderr line when the
hunt itself failed (a program that does not compile alone, a toolchain gap).
"""
import json
import pathlib
import sys


def load(out):
    rows = []
    for code_file in sorted((out / "code").iterdir()):
        slug = code_file.name
        code = code_file.read_text().strip()
        report = read_json(out / "json" / f"{slug}.json")
        log = (out / "log" / f"{slug}.log").read_text().strip().splitlines()
        rows.append((slug, code, report, log[0] if log else ""))
    return rows


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def finding_cell(report):
    if not report:
        return ""
    cells = []
    for finding in report["findings"]:
        place = f"{finding['file']}:{finding['line']}" if finding.get("file") else finding.get("runtimeFrame") or finding["test"]
        cells.append(f"{finding['class']} {place} ({finding['replay']})")
    return "<br>".join(cells)


def row(slug, code, report, first_log):
    program = report["program"] if report else slug
    explored = report["explored"] if report else {}
    paths = f"{explored.get('paths', '')}/{explored.get('completed', '')}" if report else ""
    incomplete = len(report["incomplete"]) if report else ""
    note = "" if report else first_log.replace("|", "\\|")[:120]
    return f"| {program} | {code} | {paths} | {incomplete} | {finding_cell(report)} | {note} |"


def main(argv):
    out = pathlib.Path(argv[1])
    findings_only = "--findings-only" in argv
    rows = load(out)
    total = len(rows)
    with_findings = [r for r in rows if r[2] and r[2]["findings"]]
    failed = [r for r in rows if r[1] == "2" or (r[1] == "1" and not r[2])]
    print(f"# Hunt corpus: {total} programs, {len(with_findings)} with findings, {len(failed)} did not hunt\n")
    print("| program | exit | paths/complete | incomplete | findings | note |")
    print("|---|---|---|---|---|---|")
    for slug, code, report, first_log in rows:
        if findings_only and not (report and report["findings"]) and code == "0":
            continue
        print(row(slug, code, report, first_log))


if __name__ == "__main__":
    main(sys.argv)
