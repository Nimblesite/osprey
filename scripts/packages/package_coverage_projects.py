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

PACKAGES_DIR = "src/osprey/packages"
APPS_DIR = "src/osprey"
MIN_THRESHOLD = 90
PROJECT_KEYS = {PACKAGES_DIR: "packages", APPS_DIR: "apps"}
GATED_KINDS = tuple(PROJECT_KEYS)
SOURCE_SUFFIXES = frozenset({".py", ".osp", ".ospml", ".lean", ".ts"})
MATH_STORE = (".osprey", "math")
"""Where a project keeps the modules generated from its `.lean` math sources [MATH-CODE-UNTRACKED]."""
MATH_DEF_RE = re.compile(r"^\s*(?:private\s+)?def\s", re.M)
DEFAULT_DEF_RE = re.compile(r"(?m)^[ \t]*(?:export[ \t]+)?fn[ \t]+\w+")
TYPESCRIPT_DEF_RE = re.compile(r"(?m)^[ \t]*(?:export[ \t]+)?function[ \t]+\w+")
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
    return {kind: source[key] for kind, key in PROJECT_KEYS.items() if isinstance(source.get(key), dict)}


def kind_map_problems(config: object) -> list[str]:
    projects = config.get("projects") if isinstance(config, dict) else None
    if not isinstance(projects, dict):
        return []
    return [f"projects.{key} must be an object" for key in PROJECT_KEYS.values()
            if key in projects and not isinstance(projects[key], dict)]


def kind_noun(kind: str) -> str:
    return "app" if kind == APPS_DIR else "package"


def project_label(kind: str, name: str) -> str:
    return name if kind == PACKAGES_DIR else f"{kind}/{name}"


def report_name(kind: str, name: str) -> str:
    return name if kind == PACKAGES_DIR else f"{PROJECT_KEYS[kind]}-{name}"


def project_dirs(root: Path, kind: str) -> set[str]:
    base = root / kind
    if not base.is_dir():
        return set()
    return {path.name for path in base.iterdir() if path.is_dir() and (path / "osprey.toml").is_file()}


def declared_roots(suite, project):
    """The extra roots a suite manifest declares, type-checked before any traversal.

    A mistyped shape would otherwise be iterated (a string is a sequence of
    characters, one of them "/"), so the whole filesystem could be crawled
    instead of the fixture. Nothing is skipped: a bad shape is a fault.
    """
    if project is None:
        return []
    if not isinstance(project, dict):
        raise ValueError(f"{(suite / 'osprey.toml')} has no [project] table")
    declared = project.get("source_roots", [])
    if not isinstance(declared, list):
        raise ValueError(f"{(suite / 'osprey.toml')} source_roots must be a list, not {type(declared).__name__}")
    for root in declared:
        if not isinstance(root, str) or "\x00" in root:
            raise ValueError(f"{(suite / 'osprey.toml')} source_roots entry is not a path: {root!r}")
    return [(suite / root).resolve() for root in declared]


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
    """A report path as the project's own source: under `src/`, or generated from a math source there."""
    parts = normalize_components(raw)
    if parts is None:
        return None
    project = kind.split("/") + [name]
    for marker in (project + ["src"], project + [*MATH_STORE, "src"]):
        for start in range(len(parts) - len(marker), -1, -1):
            if parts[start : start + len(marker)] == marker:
                return tuple(parts[start + len(marker) :])
    return None


def own_source_file(root: Path, kind: str, name: str, rel: tuple[str, ...]) -> Path:
    """The text of an own source: the file under `src/`, else the module generated from the math source there."""
    written = root / kind / name / "src" / Path(*rel)
    return written if written.is_file() else root / kind / name / Path(*MATH_STORE) / "src" / Path(*rel)


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
    if path.suffix == ".ts":
        return not path.name.endswith(".d.ts") and TYPESCRIPT_DEF_RE.search(text) is not None
    return path.suffix == ".lean" and MATH_DEF_RE.search(text) is not None


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
            files.append(relative.as_posix() + (".ospml" if path.suffix == ".lean" else ""))
    return set(files)


UNFINISHED_ALLOWED = frozenset({(PACKAGES_DIR, "transformers")})
"""The only projects the user approved to drop the floor; any other `unfinished` key is a config error."""


def unfinished_problem(kind: str, name: str, entry: dict) -> list[str]:
    allowed = entry.get("unfinished") is True and (kind, name) in UNFINISHED_ALLOWED
    return [] if entry.get("unfinished", False) is False or allowed else ["unfinished may only be true, and only for transformers"]


def minimum(entry: dict) -> int:
    """An allowlisted `"unfinished": true` project still reports coverage but has no 90% floor."""
    return 0 if entry.get("unfinished") is True else MIN_THRESHOLD
