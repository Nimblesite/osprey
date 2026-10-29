"""A project's own entry run, measured beside its suite ([PKG-COVERAGE-GATE]).

A project that pins `tests/entry.expectedoutput` declares the exact bytes its
entry prints for the closed-stdin probe. The gate then runs the project itself
— not its suite — through the same collector, refuses any output that is not
that oracle byte for byte, requires a real counter on the entry module, and
merges the measured own-source hits into the suite report. Nothing is
zero-filled and nothing is merged into unvalidated evidence: the suite report
passes the same own-source validation as the gate itself before a single hit is
added, and a missing, stale, refused or counter-less probe fails the project.

The checker module is passed in, never imported: the gate runs as `__main__`, so
importing it from here would build a second, divergent module instance.
"""
import json
import os
import tomllib
from pathlib import Path

ORACLE = "entry.expectedoutput"


def entry_oracle(root, kind, name):
    """The declared oracle, or a fault when the path exists but is not a file."""
    path = root / kind / name / "tests" / ORACLE
    if not os.path.lexists(path):
        return None
    if not path.is_file():
        raise ValueError(f"entry oracle {path} is not a readable file")
    return path


RECEIPTS = (".suite.json", ".entry.json", ".entry.log")


def receipt(module, reports, kind, name, suffix):
    return module.artifact(reports, f"{module.report_name(kind, name)}{suffix}")


def receipt_paths(module, reports, kind, name):
    """Every per-run receipt beside the canonical report, in one naming convention."""
    return tuple(receipt(module, reports, kind, name, suffix) for suffix in RECEIPTS)


def entry_paths(module, reports, kind, name):
    return receipt(module, reports, kind, name, ".entry.json"), receipt(module, reports, kind, name, ".entry.log")


def suite_receipt(module, reports, kind, name):
    """The suite's own raw report, kept beside the merged canonical report."""
    return receipt(module, reports, kind, name, ".suite.json")


def collect_entry(module, root, compiler, reports, kind, name, memory="gc"):
    """Measure the entry of a project that declares an oracle; [] when it declares none."""
    try:
        oracle = entry_oracle(root, kind, name)
    except ValueError as error:
        return [f"entry probe: {error}"]
    if oracle is None:
        return []
    return probe(module, root, compiler, reports, kind, name, memory, oracle)


def probe(module, root, compiler, reports, kind, name, memory, oracle):
    entry, log = entry_paths(module, reports, kind, name)
    suite = suite_receipt(module, reports, kind, name)
    command, done, error = module.run_collector(root, compiler, entry, root / kind / name, memory)
    if error is not None:
        module.write_log(log, command, None, f"cannot run entry collector: {error}")
        return [f"cannot run entry collector: {error}; log {log}"]
    if not module.write_log(log, command, done, None):
        return [f"cannot write entry log {log}"]
    problems = probe_problems(module, root, kind, name, entry, log, done, oracle)
    return problems or merge(module, root, kind, name, entry, log, suite)


def probe_problems(module, root, kind, name, entry, log, done, oracle):
    if done.returncode != 0:
        return [f"entry probe exited {done.returncode}; log {log}"]
    problems = output_problems(module, done.stdout, oracle, log)
    if problems:
        return problems
    files, problems = module.read_report(entry)
    if files is None:
        return [f"entry probe: {problem}; log {log}" for problem in problems]
    return measured_problems(module, root, kind, name, files, log)


def measured_problems(module, root, kind, name, files, log):
    """The probe's own report must be valid *and* prove the entry module ran."""
    entries, issues, _ = module.own_entries(files, root, kind, name)
    if issues:
        return [f"entry probe: {issue}; log {log}" for issue in issues]
    try:
        entry = entry_name(root, kind, name)
    except ValueError as error:
        return [f"entry probe: {error}; log {log}"]
    return counter_problems(entries, entry, log)


def output_problems(module, stdout, oracle, log):
    raw = module.strip_runner_footer(stdout)
    if raw is None:
        return [f"entry probe printed no runner summary lines; log {log}"]
    try:
        expected = oracle.read_bytes().decode("utf-8")
    except (OSError, UnicodeError) as error:
        return [f"entry oracle {oracle} is unreadable: {error}; log {log}"]
    if raw == expected:
        return []
    return [f"entry probe output differs from {ORACLE}: {module.first_mismatch(raw, expected)}; log {log}"]


def entry_name(root, kind, name):
    """The project's own entry as a logical own path; a fault is a ValueError."""
    manifest = root / kind / name / "osprey.toml"
    try:
        project = tomllib.loads(manifest.read_text()).get("project")
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"cannot read {manifest}: {error}") from error
    if not isinstance(project, dict):
        raise ValueError(f"{manifest} has no [project] table")
    entry = project.get("entry")
    if not isinstance(entry, str) or not entry:
        raise ValueError(f"{manifest} project.entry is not a source path: {entry!r}")
    try:
        return str(Path(entry).relative_to("src"))
    except ValueError as error:
        raise ValueError(f"{manifest} project.entry {entry!r} is not under src/") from error


def counter_problems(entries, entry, log):
    measured = next((item for item in entries if item["path"] == entry), None)
    if measured is None:
        return [f"entry probe measured no counters for {entry}; log {log}"]
    if measured["covered"] == 0:
        return [f"entry probe never executed {entry}; log {log}"]
    return []


def merge(module, root, kind, name, entry, log, suite):
    """Add the probe's real own-line hits to the suite report, by logical path."""
    primary = entry.with_suffix("").with_suffix(".json")
    preserved = preserve_suite(primary, suite, log)
    if preserved:
        return preserved
    merged, problems = module.read_report(primary)
    measured, issues = module.read_report(entry)
    if merged is None or measured is None:
        return [f"entry probe report unusable: {problem}; log {log}" for problem in problems + issues]
    invalid = primary_problems(module, root, kind, name, merged, log)
    if invalid:
        return invalid
    apply_entry(module, merged, measured, kind, name)
    return written(primary, merged, log)


def apply_entry(module, merged, measured, kind, name):
    """Sum the probe's own-file hits into the suite report, one logical path each."""
    index = logical_index(module, merged, kind, name)
    for key, body in measured.items():
        rel = module.own_source_relative(key, kind, name)
        if rel is None:
            continue
        target = index.get("/".join(rel), f"{kind}/{name}/src/{'/'.join(rel)}")
        index["/".join(rel)] = target
        merged[target] = merged_body(merged.get(target), body)


def preserve_suite(primary, suite, log):
    """Copy the suite's raw report byte for byte before the merge rewrites it."""
    try:
        suite.write_bytes(primary.read_bytes())
    except OSError as error:
        return [f"cannot preserve the suite report {primary} as {suite.name}: {error}; log {log}"]
    return []


def primary_problems(module, root, kind, name, files, log):
    """Refuse a suite report the gate itself would refuse, before merging into it."""
    _, issues, _ = module.own_entries(files, root, kind, name)
    return [f"suite coverage report invalid: {issue}; log {log}" for issue in issues]


def written(primary, merged, log):
    try:
        primary.write_text(json.dumps({"files": merged}) + "\n")
    except OSError as error:
        return [f"cannot write merged coverage report {primary}: {error}; log {log}"]
    return []


def merged_body(existing, measured):
    if existing is None:
        return measured
    return {**existing, "lines": summed(existing["lines"], measured["lines"])}


def logical_index(module, files, kind, name):
    pairs = ((module.own_source_relative(key, kind, name), key) for key in files)
    return {"/".join(rel): key for rel, key in pairs if rel is not None}


def summed(left, right):
    total = dict(left)
    for line, count in right.items():
        total[line] = total.get(line, 0) + count
    return total
