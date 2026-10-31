"""Project discovery for the Osprey source coverage gate [PKG-COVERAGE-GATE].

A gated project is a (kind, name) pair whose kind directory is `packages` or
`apps`: sources live at `<kind>/<name>/src`, the suite at `<kind>/<name>/tests`
and reports are named `<name>` for packages and `<kind>-<name>` for apps so no
app report can collide with a package report.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

PACKAGES_DIR = "packages"
APPS_DIR = "apps"
GATED_KINDS = (PACKAGES_DIR, APPS_DIR)
SOURCE_SUFFIXES = frozenset({".py", ".osp", ".ospml"})
DEFAULT_DEF_RE = re.compile(r"(?m)^[ \t]*(?:export[ \t]+)?fn[ \t]+\w+")
ML_DEF_RE = re.compile(
    r"(?m)^[ \t]*(?:export[ \t]+)?(?P<name>[A-Za-z][\w']*)"
    r"(?:[ \t]*<[^>\n]*>)?[ \t]*(?:\([^)\n]*\))?"
    r"(?:[ \t]+[a-z][\w']*)*[ \t]*=[ \t]*\S"
)
ML_KEYWORDS = frozenset(
    {"type", "module", "namespace", "import", "let", "and", "match", "where"}
)


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


def gate_entries(config: object) -> dict[str, dict]:
    projects = config.get("projects") if isinstance(config, dict) else None
    source = projects if isinstance(projects, dict) else {}
    return {kind: source[kind] for kind in GATED_KINDS if isinstance(source.get(kind), dict)}


def kind_map_problems(config: object) -> list[str]:
    projects = config.get("projects") if isinstance(config, dict) else None
    if not isinstance(projects, dict):
        return []
    return [f"projects.{kind} must be an object" for kind in GATED_KINDS
            if kind in projects and not isinstance(projects[kind], dict)]


def kind_noun(kind: str) -> str:
    return "app" if kind == APPS_DIR else "package"


def project_label(kind: str, name: str) -> str:
    return name if kind == PACKAGES_DIR else f"{kind}/{name}"


def report_name(kind: str, name: str) -> str:
    return name if kind == PACKAGES_DIR else f"{kind}-{name}"


def project_dirs(root: Path, kind: str) -> set[str]:
    base = root / kind
    if not base.is_dir():
        return set()
    return {path.name for path in base.iterdir() if path.is_dir() and (path / "osprey.toml").is_file()}


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


def own_source_relative(raw: object, kind: str, name: str) -> tuple[str, ...] | None:
    parts = normalize_components(raw)
    if parts is None:
        return None
    marker = [kind, name, "src"]
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
        return python_coverable(text)
    if path.suffix == ".ospml":
        return any(match.group("name") not in ML_KEYWORDS for match in ML_DEF_RE.finditer(text))
    if path.suffix == ".osp":
        return DEFAULT_DEF_RE.search(text) is not None
    return False


def required_source_files(root: Path, kind: str, name: str) -> set[str]:
    base = root / kind / name / "src"
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
