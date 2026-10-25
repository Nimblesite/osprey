#!/usr/bin/env python3
"""Per-package source coverage gates for the Osprey package bundles.
Implements [PKG-COVERAGE-GATE].

One report per package under `projects.packages`, never averaged: a package
passes only when its OWN `packages/<name>/src` lines meet its own threshold.
Test fixtures, helpers and imported libraries (`packages/testing/src/check.py`,
`tests/*.py`) appear in the compiler's JSON but must never pad the numerator or
the denominator — a package whose tests are perfect and whose source is not has
to fail.

    python3 scripts/package_coverage.py [--collect] [--compiler PATH]
        [--reports PATH] [--thresholds PATH] [--root PATH] [--package NAME]

`--collect` removes the package's stale report and log, reruns
`<compiler> test packages/<name>/tests --coverage-json <report> --quiet`, and
then gates the report. A nonzero exit, a stdout oracle that differs from
`tests/expected_output.txt`, or an unwritable report invalidates the evidence,
because the suite can print failures and still exit zero. Without `--collect`
the existing `<reports>/<name>.json` files are gated and nothing is re-run.
`--package` narrows the work but the whole package inventory is still checked.

The runner appends `# coverage total:` and `# suites:` lines to the suite's own
stdout, so exactly those two documented footer lines are removed before the
oracle comparison; every other line must match the golden byte for byte.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from package_coverage_evidence import SOURCE_SUFFIXES, evidence_path, evidence_problems, golden_problems, seal_evidence, source_inputs

EXIT_OK = 0
EXIT_GATE = 1
EXIT_USAGE = 2
PACKAGES_DIR = "packages"
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
PY_DEF_RE = re.compile(r"(?m)^[ \t]*def[ \t]+\w+")
DEFAULT_DEF_RE = re.compile(r"(?m)^[ \t]*fn[ \t]+\w+")
ML_DEF_RE = re.compile(
    r"(?m)^[ \t]*(?:export[ \t]+)?(?P<name>[A-Za-z][\w']*)"
    r"(?:[ \t]*<[^>\n]*>)?[ \t]*(?:\([^)\n]*\))?"
    r"(?:[ \t]+[a-z][\w']*)*[ \t]*=[ \t]*\S"
)
ML_KEYWORDS = frozenset(
    {"type", "module", "namespace", "import", "let", "and", "match", "where"}
)
LINE_KEY_RE = re.compile(r"^[1-9][0-9]*$")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--collect", action="store_true", help="rerun suites")
    parser.add_argument("--compiler", help="compiler binary for --collect")
    parser.add_argument("--reports", help="per-package evidence directory")
    parser.add_argument("--thresholds", help="coverage-thresholds.json path")
    parser.add_argument("--root", help="checkout root (default: repo root)")
    parser.add_argument("--package", help="gate one package (inventory still checked)")
    return parser


def resolve_paths(args: argparse.Namespace) -> tuple[Path, Path, Path, Path]:
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parents[1]
    thresholds = Path(args.thresholds) if args.thresholds else root / "coverage-thresholds.json"
    reports = Path(args.reports) if args.reports else root / "target" / "coverage" / "packages"
    compiler = Path(args.compiler) if args.compiler else root / "target" / "release" / "osprey"
    return root, thresholds, reports, compiler


def package_dirs(root: Path) -> set[str]:
    base = root / PACKAGES_DIR
    if not base.is_dir():
        return set()
    return {path.name for path in base.iterdir() if path.is_dir() and (path / "osprey.toml").is_file()}


def suite_problems(tests: Path, verify_golden: bool) -> list[str]:
    problems = [f"missing tests/{name}" for name in ("osprey.toml", "expected_output.txt")
                if not (tests / name).is_file()]
    if verify_golden and (tests / "expected_output.txt").is_file():
        problems += golden_problems(tests / "expected_output.txt")
    return problems


def validate_entry(root: Path, name: str, entry: object, verify_golden: bool) -> list[str]:
    if not isinstance(entry, dict):
        return [f"{name}: gate entry must be an object"]
    threshold = entry.get("threshold")
    problems = []
    if isinstance(threshold, bool) or not isinstance(threshold, int):
        problems.append(f"{name}: threshold must be an integer")
    elif not MIN_THRESHOLD <= threshold <= MAX_THRESHOLD:
        problems.append(f"{name}: threshold {threshold} must be {MIN_THRESHOLD}..{MAX_THRESHOLD}")
    if entry.get("language") != DEFAULT_LANGUAGE:
        problems.append(f"{name}: language must be {DEFAULT_LANGUAGE!r}")
    tests = root / PACKAGES_DIR / name / "tests"
    problems += [f"{name}: {problem}" for problem in suite_problems(tests, verify_golden)]
    return problems


def validate_inventory(root: Path, config: object, verify_golden: bool) -> tuple[dict, list[str]]:
    projects = config.get("projects") if isinstance(config, dict) else None
    packages = projects.get("packages") if isinstance(projects, dict) else None
    if not isinstance(packages, dict) or not packages:
        return {}, ["coverage-thresholds.json has no projects.packages map"]
    shipped = package_dirs(root)
    problems = []
    problems += [f"{name}: package has no coverage gate entry" for name in sorted(shipped - set(packages))]
    problems += [f"{name}: gate entry has no packages/{name}/osprey.toml" for name in sorted(set(packages) - shipped)]
    for name in sorted(shipped & set(packages)):
        problems += validate_entry(root, name, packages[name], verify_golden)
    return packages, problems


def normalize_components(raw: object) -> list[str] | None:
    if not isinstance(raw, str) or not raw.strip():
        return None
    parts: list[str] = []
    for part in raw.split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            if not parts:
                return None
            parts.pop()
            continue
        parts.append(part)
    return parts or None


def own_source_relative(raw: object, name: str) -> tuple[str, ...] | None:
    parts = normalize_components(raw)
    if parts is None:
        return None
    marker = [PACKAGES_DIR, name, "src"]
    for start in range(len(parts) - 4, -1, -1):
        if parts[start : start + 3] == marker:
            return tuple(parts[start + 3 :])
    return None


def has_definitions(path: Path) -> bool:
    try:
        text = path.read_text()
    except OSError:
        return False
    if path.suffix == ".py":
        return PY_DEF_RE.search(text) is not None
    if path.suffix == ".ospml":
        return any(match.group("name") not in ML_KEYWORDS for match in ML_DEF_RE.finditer(text))
    if path.suffix == ".osp":
        return DEFAULT_DEF_RE.search(text) is not None
    return False


def required_source_files(root: Path, name: str) -> set[str]:
    base = root / PACKAGES_DIR / name / "src"
    if not base.is_dir():
        return set()
    files = []
    for path in base.rglob("*"):
        relative = path.relative_to(base)
        if any(part.startswith(".") for part in relative.parts):
            continue
        if path.is_file() and path.suffix in SOURCE_SUFFIXES and has_definitions(path):
            files.append(relative.as_posix())
    return set(files)


def read_report(path: Path) -> tuple[dict | None, list[str]]:
    try:
        data = json.loads(path.read_text())
    except OSError:
        return None, [f"missing coverage report: {path}"]
    except json.JSONDecodeError as error:
        return None, [f"malformed coverage report: {error.msg} at line {error.lineno}"]
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


def validate_own_file(root: Path, name: str, key: str, rel: tuple[str, ...], body: object) -> tuple[dict | None, list[str]]:
    local = root / PACKAGES_DIR / name / "src" / Path(*rel)
    if not local.is_file():
        return None, [f"missing executable source file: {key}"]
    lines = body.get("lines") if isinstance(body, dict) else None
    if not isinstance(lines, dict) or not lines:
        return None, [f"no line counters for {key}"]
    hits, problems = validated_hits(lines, len(local.read_text().splitlines()), key)
    if problems:
        return None, problems
    covered = sum(1 for count in hits.values() if count > 0)
    return {"path": key, "covered": covered, "total": len(hits), "percentage": percentage(covered, len(hits))}, []


def own_entries(files: dict, root: Path, name: str) -> tuple[list[dict], list[str], set[str]]:
    entries, problems, present, seen = [], [], set(), set()
    for raw, body in files.items():
        rel = own_source_relative(raw, name)
        if rel is None:
            continue  # test fixtures, helpers and imported libraries are not ours
        key = "/".join(rel)
        if key in seen:
            problems.append(f"duplicate source path after normalization: {key}")
            continue
        seen.add(key)
        present.add(key)
        entry, issues = validate_own_file(root, name, key, rel, body)
        problems += issues
        if entry is not None:
            entries.append(entry)
    return entries, problems, present


def percentage(covered: int, total: int) -> float | None:
    return None if total == 0 else round(covered / total * HUNDRED, 1)


def result_template(name: str, configured: dict) -> dict:
    return {
        "name": name,
        "threshold": configured["threshold"],
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
    if result["covered"] * HUNDRED < result["threshold"] * result["total"]:
        return [f"{result['percentage']:.1f}% is below the {result['threshold']}% threshold"]
    return []


def gate_package(root: Path, reports: Path, name: str, configured: dict) -> dict:
    result = result_template(name, configured)
    files, problems = read_report(reports / f"{name}{REPORT_SUFFIX}")
    problems += golden_problems(root / PACKAGES_DIR / name / "tests" / "expected_output.txt")
    if files is not None:
        entries, issues, present = own_entries(files, root, name)
        problems += issues
        missing = required_source_files(root, name) - present
        problems += [f"missing source evidence: src/{rel}" for rel in sorted(missing)]
        result["files"] = entries
        result["covered"] = sum(entry["covered"] for entry in entries)
        result["total"] = sum(entry["total"] for entry in entries)
        result["percentage"] = percentage(result["covered"], result["total"])
        problems += evidence_problems(root, name, reports / f"{name}{REPORT_SUFFIX}")
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


def run_collector(root: Path, compiler: Path, report: Path, suite: Path):
    command = [str(compiler), "test", str(suite), "--coverage-json", str(report), "--quiet"]
    try:
        completed = subprocess.run(command, capture_output=True, cwd=root, check=False)
        completed.stdout = completed.stdout.decode("utf-8")
        completed.stderr = completed.stderr.decode("utf-8")
        return command, completed, None
    except OSError as error:
        return command, None, error


def collect_package(root: Path, compiler: Path, reports: Path, name: str, configured: dict) -> dict:
    report = (reports / f"{name}{REPORT_SUFFIX}").resolve()
    log = reports / f"{name}{LOG_SUFFIX}"
    suite = root / PACKAGES_DIR / name / "tests"
    if not clear_stale(report, log):
        return collection_failure(name, configured, [f"cannot remove stale evidence in {reports}"])
    before = source_inputs(root, name)
    command, completed, error = run_collector(root, compiler, report, suite)
    if error is not None:
        write_log(log, command, None, f"cannot run collector: {error}")
        return collection_failure(name, configured, [f"cannot run collector: {error}"])
    if not write_log(log, command, completed, None):
        remove_if_exists(report)
        return collection_failure(name, configured, [f"cannot write log {log}"])
    return collected_evidence(root, reports, name, configured, report, log, completed, suite, before)


def collected_evidence(root: Path, reports: Path, name: str, configured: dict,
                       report: Path, log: Path, completed, suite: Path, before: dict) -> dict:
    if completed.returncode != 0:
        remove_if_exists(report)
        return collection_failure(name, configured, [f"collection failed with exit {completed.returncode}; log {log}"])
    problems = compare_oracle(completed.stdout, suite / "expected_output.txt")
    if problems:
        remove_if_exists(report)
        return collection_failure(name, configured, [f"{problem}; log {log}" for problem in problems])
    try:
        seal_evidence(root, name, report, before)
    except (OSError, ValueError) as error:
        remove_if_exists(report)
        return collection_failure(name, configured, [f"cannot verify collection: {error}"])
    return gate_package(root, reports, name, configured)


def print_result(result: dict) -> None:
    shown = "n/a" if result["percentage"] is None else f"{result['percentage']:.1f}%"
    status = "OK  " if result["pass"] else "FAIL"
    print(f"{status} {result['name']}: {result['covered']}/{result['total']} own source lines, {shown} (threshold {result['threshold']}%)")
    for problem in result["missing_evidence"]:
        print(f"     - {problem}")


def write_summary(reports: Path, results: list[dict], mode: str) -> str | None:
    failed = sum(0 if result["pass"] else 1 for result in results)
    payload = {"mode": mode, "packages": results, "passed": len(results) - failed, "failed": failed}
    try:
        reports.mkdir(parents=True, exist_ok=True)
        (reports / SUMMARY_NAME).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    except OSError as error:
        return f"cannot write {reports / SUMMARY_NAME}: {error}"
    return None


def load_config(path: Path) -> tuple[object, str | None]:
    try:
        return json.loads(path.read_text()), None
    except (OSError, json.JSONDecodeError) as error:
        return None, f"cannot read {path}: {error}"


def fail(message: str, code: int = EXIT_USAGE) -> int:
    print(f"package coverage: {message}", file=sys.stderr)
    return code


def selected_names(configured: dict, requested: str | None) -> list[str] | None:
    if requested is None:
        return sorted(configured)
    return [requested] if requested in configured else None


def evaluate(root: Path, reports: Path, compiler: Path, configured: dict, names: list[str], collect: bool) -> list[dict]:
    if collect:
        reports.mkdir(parents=True, exist_ok=True)
        remove_if_exists(reports / SUMMARY_NAME)
    return [
        collect_package(root, compiler, reports, name, configured[name])
        if collect
        else gate_package(root, reports, name, configured[name])
        for name in names
    ]


def finish(results: list[dict], reports: Path, mode: str, thresholds_path: Path) -> int:
    error = write_summary(reports, results, mode)
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
    names = selected_names(configured, args.package)
    if names is None:
        return fail(f"unknown package {args.package!r}")
    results = evaluate(root, reports, compiler, configured, names, args.collect)
    for result in results:
        print_result(result)
    return finish(results, reports, "collect" if args.collect else "check", thresholds_path)


if __name__ == "__main__":
    sys.exit(main())
