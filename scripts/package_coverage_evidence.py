"""Bind coverage reports to the source and suite whose golden output passed."""
import ast
import hashlib
import json
import re
import tomllib
import tempfile
from datetime import datetime, timezone
from pathlib import Path

SOURCE_SUFFIXES = frozenset({".py", ".osp", ".ospml"})
GENERATED_DIRS = frozenset({"target", "__pycache__", "node_modules"})
COMMENT_OR_STRING = re.compile(r'''"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|//[^\n]*|/\*|\(\*''')
MATCH_ARM = re.compile(r"^\s+(?![/#])(?:[^=\\:\n]+|[A-Z]\w*\s*\([^\\\n]*\))\s*=>")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def executable_assignment(node):
    if not isinstance(node, (ast.Assign, ast.AnnAssign)) or node.value is None:
        return False
    try:
        ast.literal_eval(node.value)
        return False
    except (ValueError, TypeError, SyntaxError):
        return True


def python_coverable(text):
    try:
        nodes = ast.walk(ast.parse(text))
        return any(isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda,
                                    ast.Call, ast.Await, ast.Raise, ast.Assert))
                   or executable_assignment(node) for node in nodes)
    except SyntaxError:
        return True


def concrete_function(node):
    return isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and not (
        len(node.body) == 1 and isinstance(node.body[0], ast.Expr)
        and isinstance(node.body[0].value, ast.Constant) and node.body[0].value.value is Ellipsis)


def executable_statement(node):
    if isinstance(node, ast.Expr):
        return not (isinstance(node.value, ast.Constant)
                    and (isinstance(node.value.value, str) or node.value.value is Ellipsis))
    if isinstance(node, ast.AnnAssign):
        return node.value is not None
    if isinstance(node, ast.Assign):
        return not all(isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets)
    return isinstance(node, (ast.Return, ast.AugAssign, ast.If, ast.For, ast.While,
                             ast.With, ast.Match, ast.Assert, ast.Raise, ast.Break, ast.Continue))


def definition_problems(path, hits):
    if path.suffix != ".py":
        return match_arm_problems(path, hits)
    try:
        nodes = list(ast.walk(ast.parse(path.read_text())))
        kinds = {node.lineno: "executable statement" for node in nodes if executable_statement(node)}
        kinds.update({node.lineno: "function definition" for node in nodes if concrete_function(node)})
        return [f"missing {kinds[line]} counter: {path.name}:{line}"
                for line in sorted(set(kinds) - set(hits))]
    except (SyntaxError, OSError) as error:
        return [f"cannot verify source counters: {path}: {error}"]


def match_arm_problems(path, hits):
    # Cross-check ordinary arm entries independently of compiler-emitted rows.
    # Bindings, type arrows and lambdas are not match arm declarations.
    lines = {index for index, line in enumerate(without_comments(path.read_text()).splitlines(), 1)
             if MATCH_ARM.match(line)}
    return [f"missing match arm counter: {path.name}:{line}"
            for line in sorted(lines - set(hits))]


def block_comment_end(text, start):
    opening = text[start:start + 2]
    closing = "*)" if opening == "(*" else "*/"
    cursor, depth = start + 2, 1
    while cursor < len(text) and depth:
        pair = text[cursor:cursor + 2]
        depth += int(pair == opening) - int(pair == closing)
        cursor += 2 if pair in (opening, closing) else 1
    return cursor


def without_comments(text):
    cursor, parts = 0, []
    while token := COMMENT_OR_STRING.search(text, cursor):
        if token[0].startswith(('"', "'")):
            parts.append(text[cursor:token.end()])
            cursor = token.end()
        else:
            end = token.end() if token[0].startswith("//") else block_comment_end(text, token.start())
            parts.append(text[cursor:token.start()] + re.sub(r"[^\n]", " ", text[token.start():end]))
            cursor = end
    return "".join(parts) + text[cursor:]


def oracle_problems(text):
    summary = re.search(r"^([1-9][0-9]*) passed, 0 failed\n?\Z", text, re.MULTILINE)
    tap = re.search(r"^# tests=([1-9][0-9]*) passed=([1-9][0-9]*) failed=0 skipped=0\n?\Z", text, re.MULTILINE)
    count = len(re.findall(r"^ok ", text, re.MULTILINE))
    passed = int(summary[1]) if summary else int(tap[1]) if tap and tap[1] == tap[2] else 0
    bad_rows = re.search(r"^(?:FAIL(?: |$)|not ok(?: |$))", text, re.MULTILINE)
    return [] if passed > 0 and passed == count and not bad_rows else ["suite golden must prove positive passing assertions and zero failures"]


def golden_problems(path):
    try:
        return oracle_problems(path.read_bytes().decode("utf-8"))
    except (OSError, UnicodeError):
        return ["cannot read suite golden"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_inputs(root, name):
    root = Path(root).resolve()
    package = root / "packages" / name
    suite = package / "tests"
    manifest = tomllib.loads((suite / "osprey.toml").read_text())
    roots = [package / "src", suite]
    roots += [(suite / path).resolve() for path in manifest.get("project", {}).get("source_roots", [])]
    files = {package / "osprey.toml", suite / "osprey.toml", suite / "expected_output.txt"}
    for base in roots:
        files.update(path.resolve() for path in base.rglob("*") if path.is_file()
                     and not any(part.startswith(".") or part in GENERATED_DIRS
                                 for part in path.relative_to(base).parts))
    return {path.relative_to(root).as_posix(): digest(path) for path in sorted(files)}


def evidence_path(report):
    return report.with_suffix(".evidence.json")


def seal_evidence(root, name, report, before):
    if source_inputs(root, name) != before:
        raise ValueError("source or suite changed during coverage collection")
    evidence = {"package": name, "inputs": before, "report_sha256": digest(report)}
    evidence_path(report).write_text(json.dumps(evidence, sort_keys=True) + "\n")


def evidence_problems(root, name, report):
    try:
        evidence = json.loads(evidence_path(report).read_text(), object_pairs_hook=unique_object)
        valid = (evidence.get("package") == name and evidence.get("inputs") == source_inputs(root, name)
                 and evidence.get("report_sha256") == digest(report))
        return [] if valid else ["stale or altered coverage evidence; recollect this package"]
    except (OSError, ValueError, AttributeError):
        return ["missing verified collection evidence; recollect this package"]


def optional_digest(path):
    return digest(path) if path.is_file() else None


def measurement_basis(root, reports, names):
    return {name: {"inputs": source_inputs(root, name),
                   "report": optional_digest(reports / f"{name}.json"),
                   "evidence": optional_digest(evidence_path(reports / f"{name}.json")),
                   "log": optional_digest(reports / f"{name}.log")}
            for name in sorted(names)}


def measurement_row(root, result):
    valid = result["percentage"] is not None
    counts = {"covered": result["covered"] if valid else None,
              "total": result["total"] if valid else None, "percentage": result["percentage"]}
    problems = "; ".join(result["missing_evidence"]).replace(str(root) + "/", "")
    return counts if valid else {**counts, "error": problems}


def measurement_snapshot(root, reports, basis, results):
    return {"_doc": "Historical verified measurements, not required minimums or CI inputs. Null means unavailable. Refresh after source or test changes.",
            "recorded_at": datetime.now(timezone.utc).isoformat(),
            "metric": "executed statement lines; not branch or path coverage",
            "reports": reports.relative_to(root).as_posix() if reports.is_relative_to(root) else str(reports),
            "basis_sha256": hashlib.sha256(json.dumps(basis, sort_keys=True).encode()).hexdigest(),
            "packages": {name: measurement_row(root, result) for name, result in sorted(results.items())}}


def replace_measurements(path, config, snapshot):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=f".{path.name}.", delete=False) as output:
            temporary = Path(output.name)
            json.dump({**config, "package_measurements": snapshot}, output, indent=2)
            output.write("\n")
        if json.loads(path.read_text(), object_pairs_hook=unique_object) != config:
            raise ValueError("threshold configuration changed during measurement; retry")
        temporary.chmod(path.stat().st_mode)
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def record_measurements(root, path, reports, config, gate):
    try:
        entries = config["projects"]["packages"]
        before = measurement_basis(root, reports, entries)
        results = {name: gate(root, reports, name, entry) for name, entry in entries.items()}
        snapshot = measurement_snapshot(root, reports, before, results)
        if measurement_basis(root, reports, entries) != before:
            raise ValueError("measurement inputs changed during validation; retry")
        replace_measurements(path, config, snapshot)
        return None
    except (OSError, ValueError) as error:
        return f"cannot record measurements: {error}"
