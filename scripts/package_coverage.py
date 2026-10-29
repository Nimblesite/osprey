#!/usr/bin/env python3
"""Per-package source coverage gates for the Osprey package bundles [PKG-COVERAGE-GATE].

One report per project under `projects.packages` or `projects.apps`, never averaged:
a project passes only when its OWN `<kind>/<name>/src` lines meet 90% or its higher
threshold. App reports are named apps-<name> so they never collide with a package.

    python3 scripts/package_coverage.py [--collect] [--compiler PATH] [--reports PATH]
        [--thresholds PATH] [--root PATH] [--package NAME] [--project KIND/NAME]
        [--memory gc|arc]

`--collect` reruns each suite (one compiler per CPU, or OSPREY_PACKAGE_JOBS) and gates
its report; a nonzero exit, stdout differing from the golden, or an unwritable report
invalidates the evidence. Without it the existing reports are gated and nothing re-runs.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from decimal import Decimal
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from package_coverage_evidence import (
    evidence_path, evidence_problems, golden_problems,
    definition_problems, record_measurements, seal_evidence, source_inputs, unique_object,
)
from package_coverage_projects import (
    GATED_KINDS, PACKAGES_DIR, gate_entries, kind_map_problems, kind_noun, project_dirs,
    project_label, report_name, required_source_files, own_source_relative,
)

EXIT_OK = 0
EXIT_GATE = 1
EXIT_USAGE = 2
REPORT_SUFFIX = ".json"
LOG_SUFFIX = ".log"
SUMMARY_NAME = "summary.json"
MIN_THRESHOLD = 90
MAX_THRESHOLD = 100
HUNDRED = 100
DEFAULT_LANGUAGE = "osprey"

COVERAGE_TOTAL_RE = re.compile(
    r"^# coverage total: (?:\d+\.\d%|n/a) \(\d+/\d+ lines\)$"
)
SUITES_RE = re.compile(r"^# suites: 1 passed, 0 failed$")
LINE_KEY_RE = re.compile(r"^[1-9][0-9]*$")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    selection = parser.add_mutually_exclusive_group()
    parser.add_argument("--collect", action="store_true", help="rerun suites")
    selection.add_argument("--record-measurements", action="store_true", help="save verified measurements separately from required thresholds")
    parser.add_argument("--compiler", help="compiler binary for --collect")
    parser.add_argument("--memory", choices=("gc", "arc"), default="gc", help="collection backend (default: gc)")
    parser.add_argument("--reports", help="per-package evidence directory")
    parser.add_argument("--thresholds", help="coverage-thresholds.json path")
    parser.add_argument("--root", help="checkout root (default: repo root)")
    selection.add_argument("--package", help="gate one package (inventory still checked)")
    selection.add_argument("--project", help="gate one project as <kind>/<name>, e.g. apps/workbench")
    return parser


def resolve_paths(args: argparse.Namespace) -> tuple[Path, Path, Path, Path]:
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parents[1]
    thresholds = Path(args.thresholds) if args.thresholds else root / "coverage-thresholds.json"
    reports = Path(args.reports) if args.reports else root / "target" / "coverage" / "packages"
    compiler = Path(args.compiler) if args.compiler else root / "target" / "release" / "osprey"
    return root, thresholds, reports, compiler


def suite_problems(tests: Path, verify_golden: bool) -> list[str]:
    problems = [f"missing tests/{name}" for name in ("osprey.toml", "expected_output.txt")
                if not (tests / name).is_file()]
    if verify_golden and (tests / "expected_output.txt").is_file():
        problems += golden_problems(tests / "expected_output.txt")
    return problems


def validate_entry(root: Path, kind: str, name: str, entry: object, verify_golden: bool) -> list[str]:
    label = project_label(kind, name)
    if not isinstance(entry, dict):
        return [f"{label}: gate entry must be an object"]
    threshold = entry.get("threshold", MIN_THRESHOLD)
    problems = []
    if isinstance(threshold, bool) or not isinstance(threshold, (int, float)):
        problems.append(f"{label}: threshold must be numeric")
    elif not MIN_THRESHOLD <= threshold <= MAX_THRESHOLD:
        problems.append(f"{label}: threshold {threshold} must be {MIN_THRESHOLD}..{MAX_THRESHOLD}")
    if entry.get("language") != DEFAULT_LANGUAGE:
        problems.append(f"{label}: language must be {DEFAULT_LANGUAGE!r}")
    problems += [f"{label}: {problem}" for problem in suite_problems(root / kind / name / "tests", verify_golden)]
    return problems


def inventory_problems(root: Path, entries: dict, verify_golden: bool) -> list[str]:
    problems = []
    for kind in GATED_KINDS:
        group = entries.get(kind, {})
        shipped = project_dirs(root, kind)
        problems += [f"{project_label(kind, name)}: {kind_noun(kind)} has no coverage gate entry"
                     for name in sorted(shipped - set(group))]
        problems += [f"{project_label(kind, name)}: gate entry has no {kind}/{name}/osprey.toml"
                     for name in sorted(set(group) - shipped)]
        for name in sorted(shipped & set(group)):
            problems += validate_entry(root, kind, name, group[name], verify_golden)
    return problems


def validate_inventory(root: Path, config: object, verify_golden: bool) -> tuple[dict, list[str]]:
    entries = gate_entries(config)
    if not entries.get(PACKAGES_DIR):
        return {}, ["coverage-thresholds.json has no projects.packages map"]
    return entries, kind_map_problems(config) + inventory_problems(root, entries, verify_golden)


def read_report(path: Path) -> tuple[dict | None, list[str]]:
    try:
        data = json.loads(path.read_text(), object_pairs_hook=unique_object)
    except OSError:
        return None, [f"missing coverage report: {path}"]
    except ValueError as error:
        return None, [f"malformed coverage report: {error}"]
    files = data.get("files") if isinstance(data, dict) else None
    if not isinstance(files, dict):
        return None, ["malformed coverage report: no files object"]
    return files, []


def validated_hits(lines: dict, line_count: int, key: str) -> tuple[dict, list[str]]:
    hits, problems = {}, []
    for raw_line, count in lines.items():
        if not isinstance(raw_line, str) or LINE_KEY_RE.match(raw_line) is None:
            problems.append(f"bad line number {raw_line!r} in {key}")
            continue
        line = int(raw_line)
        if line > line_count:
            problems.append(f"line {line} is past the end of {key} ({line_count} lines)")
            continue
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            problems.append(f"bad hit count {count!r} at {key}:{line}")
            continue
        hits[line] = count
    return hits, problems


def validate_own_file(root: Path, kind: str, name: str, key: str, rel: tuple[str, ...], body: object) -> tuple[dict | None, list[str]]:
    local = root / kind / name / "src" / Path(*rel)
    if not local.is_file():
        return None, [f"missing executable source file: {key}"]
    lines = body.get("lines") if isinstance(body, dict) else None
    if not isinstance(lines, dict) or not lines:
        return None, [f"no line counters for {key}"]
    hits, problems = validated_hits(lines, len(local.read_text().splitlines()), key)
    problems += definition_problems(local, hits)
    if problems:
        return None, problems
    covered = sum(1 for count in hits.values() if count > 0)
    entry = {"path": key, "covered": covered, "total": len(hits), "percentage": percentage(covered, len(hits))}
    return {**entry, "uncovered_lines": sorted(line for line, count in hits.items() if count == 0)}, []


def own_entries(files: dict, root: Path, kind: str, name: str) -> tuple[list[dict], list[str], set[str]]:
    entries, problems, present, seen = [], [], set(), set()
    for raw, body in files.items():
        rel = own_source_relative(raw, kind, name)
        if rel is None:
            continue  # test fixtures, helpers and imported libraries are not ours
        key = "/".join(rel)
        if key in seen:
            problems.append(f"duplicate source path after normalization: {key}")
            continue
        seen.add(key)
        present.add(key)
        entry, issues = validate_own_file(root, kind, name, key, rel, body)
        problems += issues
        if entry is not None:
            entries.append(entry)
    return entries, problems, present


def percentage(covered: int, total: int) -> float | None:
    return None if total == 0 else round(covered / total * HUNDRED, 1)


def result_template(name: str, configured: dict) -> dict:
    return {
        "name": name,
        "threshold": configured.get("threshold", MIN_THRESHOLD),
        "language": configured["language"],
        "covered": 0,
        "total": 0,
        "percentage": None,
        "pass": False,
        "files": [],
        "missing_evidence": [],
    }


def threshold_problems(result: dict) -> list[str]:
    if result["total"] == 0:
        return ["no own source lines in report"]
    if result["covered"] * HUNDRED < Decimal(str(result["threshold"])) * result["total"]:
        return [f"{result['percentage']:.1f}% is below the {result['threshold']}% threshold"]
    return []


def own_gate_problems(root: Path, kind: str, name: str, present: set[str], report: Path) -> list[str]:
    missing = required_source_files(root, kind, name) - present
    return [f"missing source evidence: src/{rel}" for rel in sorted(missing)] + evidence_problems(root, name, report, kind)


def gate_package(root: Path, reports: Path, kind: str, name: str, configured: dict) -> dict:
    report = reports / f"{report_name(kind, name)}{REPORT_SUFFIX}"
    result = result_template(project_label(kind, name), configured)
    files, problems = read_report(report)
    problems += golden_problems(root / kind / name / "tests" / "expected_output.txt")
    if files is not None:
        entries, issues, present = own_entries(files, root, kind, name)
        problems += issues + own_gate_problems(root, kind, name, present, report)
        result["files"] = entries
        result["covered"] = sum(entry["covered"] for entry in entries)
        result["total"] = sum(entry["total"] for entry in entries)
        problems += evidence_problems(root, name, report, kind)
        result["percentage"] = None if problems else percentage(result["covered"], result["total"])
    if not problems:
        problems = threshold_problems(result)
    result["missing_evidence"] = problems
    result["pass"] = not problems
    return result


def strip_runner_footer(stdout: str) -> str | None:
    lines = stdout.splitlines(keepends=True)
    if len(lines) < 2:
        return None
    total, suites = lines[-2].rstrip("\r\n"), lines[-1].rstrip("\r\n")
    if COVERAGE_TOTAL_RE.match(total) is None or SUITES_RE.match(suites) is None:
        return None
    return "".join(lines[:-2])


def first_mismatch(actual: str, expected: str) -> str:
    got, want = actual.splitlines(), expected.splitlines()
    for index in range(max(len(got), len(want))):
        left = got[index] if index < len(got) else "<missing>"
        right = want[index] if index < len(want) else "<unexpected>"
        if left != right:
            return f"suite output differs from expected_output.txt at line {index + 1}: got {left!r}, expected {right!r}"
    return "suite output differs from expected_output.txt (trailing whitespace)"


def compare_oracle(stdout: str, expected_path: Path) -> list[str]:
    raw = strip_runner_footer(stdout)
    if raw is None:
        return ["missing runner summary lines; cannot verify the suite oracle"]
    try:
        expected = expected_path.read_bytes().decode("utf-8")
    except OSError as error:
        return [f"cannot read expected output: {error}"]
    return [] if raw == expected else [first_mismatch(raw, expected)]


def write_log(path: Path, command: list[str], completed: subprocess.CompletedProcess | None, error: str | None) -> bool:
    parts = ["$ " + " ".join(command) + "\n"]
    if completed is not None:
        parts += ["--- stdout ---\n", completed.stdout, "--- stderr ---\n", completed.stderr]
    if error is not None:
        parts.append(error + "\n")
    try:
        path.write_text("".join(parts))
    except OSError:
        return False
    return True


def remove_if_exists(path: Path) -> bool:
    try:
        path.unlink()
    except FileNotFoundError:
        return True
    except OSError:
        return False
    return True


def clear_stale(report: Path, log: Path) -> bool:
    return all(remove_if_exists(path) for path in (report, log, evidence_path(report)))


def collection_failure(name: str, configured: dict, problems: list[str]) -> dict:
    result = result_template(name, configured)
    result["missing_evidence"] = problems
    return result


def run_collector(root: Path, compiler: Path, report: Path, suite: Path, memory="gc"):
    command = [str(compiler), "test", str(suite), "--coverage-json", str(report), "--quiet", f"--memory={memory}"]
    try:
        completed = subprocess.run(command, capture_output=True, cwd=root, check=False)
        completed.stdout = completed.stdout.decode("utf-8")
        completed.stderr = completed.stderr.decode("utf-8")
        return command, completed, None
    except OSError as error:
        return command, None, error


def collect_package(root: Path, compiler: Path, reports: Path, kind: str, name: str, configured: dict, memory="gc") -> dict:
    label = project_label(kind, name)
    report = (reports / f"{report_name(kind, name)}{REPORT_SUFFIX}").resolve()
    log = reports / f"{report_name(kind, name)}{LOG_SUFFIX}"
    suite = root / kind / name / "tests"
    if not clear_stale(report, log):
        return collection_failure(label, configured, [f"cannot remove stale evidence in {reports}"])
    before = source_inputs(root, name, kind)
    command, completed, error = run_collector(root, compiler, report, suite, memory)
    if error is not None:
        write_log(log, command, None, f"cannot run collector: {error}")
        return collection_failure(label, configured, [f"cannot run collector: {error}"])
    if not write_log(log, command, completed, None):
        remove_if_exists(report)
        return collection_failure(label, configured, [f"cannot write log {log}"])
    return collected_evidence(root, reports, kind, name, configured, report, log, completed, suite, before)


def collected_evidence(root: Path, reports: Path, kind: str, name: str, configured: dict,
                       report: Path, log: Path, completed, suite: Path, before: dict) -> dict:
    label = project_label(kind, name)
    if completed.returncode != 0:
        remove_if_exists(report)
        return collection_failure(label, configured, [f"collection failed with exit {completed.returncode}; log {log}"])
    problems = compare_oracle(completed.stdout, suite / "expected_output.txt")
    if problems:
        remove_if_exists(report)
        return collection_failure(label, configured, [f"{problem}; log {log}" for problem in problems])
    try:
        seal_evidence(root, name, report, before, kind)
    except (OSError, ValueError) as error:
        remove_if_exists(report)
        return collection_failure(label, configured, [f"cannot verify collection: {error}"])
    return gate_package(root, reports, kind, name, configured)


def print_result(result: dict) -> None:
    shown = "n/a" if result["percentage"] is None else f"{result['percentage']:.1f}%"
    status = "OK  " if result["pass"] else "FAIL"
    override = f" (threshold {result['threshold']}%)" if result["threshold"] > MIN_THRESHOLD else ""
    print(f"{status} {result['name']}: {result['covered']}/{result['total']} own source lines, {shown}{override}")
    for problem in result["missing_evidence"]:
        print(f"     - {problem}")


def write_summary(reports: Path, results: list[dict], mode: str) -> str | None:
    failed = sum(0 if result["pass"] else 1 for result in results)
    reported = [{key: value for key, value in result.items() if key != "threshold" or value > MIN_THRESHOLD} for result in results]
    payload = {"mode": mode, "metric": "executed statement lines", "packages": reported, "passed": len(results) - failed, "failed": failed}
    try:
        reports.mkdir(parents=True, exist_ok=True)
        (reports / SUMMARY_NAME).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    except OSError as error:
        return f"cannot write {reports / SUMMARY_NAME}: {error}"
    return None


def load_config(path: Path) -> tuple[object, str | None]:
    try:
        return json.loads(path.read_text(), object_pairs_hook=unique_object), None
    except (OSError, ValueError) as error:
        return None, f"cannot read {path}: {error}"


def fail(message: str, code: int = EXIT_USAGE) -> int:
    print(f"package coverage: {message}", file=sys.stderr)
    return code


def selected_projects(entries: dict, package: str | None, project: str | None) -> list[tuple[str, str]] | None:
    if project is not None:
        kind, _, name = project.partition("/")
        return [(kind, name)] if name in entries.get(kind, {}) else None
    if package is not None:
        return [(PACKAGES_DIR, package)] if package in entries.get(PACKAGES_DIR, {}) else None
    return sorted((kind, name) for kind, group in entries.items() for name in group)


def unknown_selection(args: argparse.Namespace) -> str:
    if args.project is not None:
        return f"unknown project {args.project!r}"
    return f"unknown package {args.package!r}"


JOBS_ENV = "OSPREY_PACKAGE_JOBS"


def worker_count(names: list[str], cpus: int | None = None) -> int:
    """Bound collection by CPU count or OSPREY_PACKAGE_JOBS, and by package count.
    Fewer workers keep every suite and threshold while limiting concurrent compilers.
    """
    configured = os.environ.get(JOBS_ENV)
    if configured is not None:
        if not configured.isdecimal() or int(configured) < 1:
            raise SystemExit(fail(f"{JOBS_ENV} must be a positive integer, got {configured!r}"))
        cpus = int(configured)
    return max(1, min(len(names), cpus if cpus is not None else os.cpu_count() or 1))


def evaluate(root: Path, reports: Path, compiler: Path, entries: dict, projects: list[tuple[str, str]], collect: bool, memory="gc") -> list[dict]:
    if collect:
        reports.mkdir(parents=True, exist_ok=True)
        remove_if_exists(reports / SUMMARY_NAME)

    def gate(pair: tuple[str, str]) -> dict:
        kind, name = pair
        if collect:
            return collect_package(root, compiler, reports, kind, name, entries[kind][name], memory)
        return gate_package(root, reports, kind, name, entries[kind][name])

    with ThreadPoolExecutor(max_workers=worker_count(projects)) as pool:
        return list(pool.map(gate, projects))


def finish(results: list[dict], reports: Path, mode: str, thresholds_path: Path, record=None) -> int:
    error = write_summary(reports, results, mode)
    if error is None and record is not None:
        error = record()
    if error is not None:
        return fail(error, EXIT_GATE)
    failed = sum(0 if result["pass"] else 1 for result in results)
    print(f"package coverage: {len(results) - failed} passed, {failed} failed ({thresholds_path})")
    return EXIT_OK if failed == 0 else EXIT_GATE


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root, thresholds_path, reports, compiler = resolve_paths(args)
    config, error = load_config(thresholds_path)
    if error is not None:
        return fail(error)
    configured, problems = validate_inventory(root, config, args.collect)
    if problems:
        for problem in problems:
            fail(problem)
        return EXIT_USAGE
    projects = selected_projects(configured, args.package, args.project)
    if projects is None:
        return fail(unknown_selection(args))
    results = evaluate(root, reports, compiler, configured, projects, args.collect, args.memory)
    for result in results:
        print_result(result)
    return finish(results, reports, "collect" if args.collect else "check", thresholds_path,
                  (lambda: record_measurements(root, thresholds_path, reports, config, gate_package)) if args.record_measurements else None)


if __name__ == "__main__":
    sys.exit(main())
