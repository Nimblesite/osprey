#!/usr/bin/env python3
"""Publish validated measurements while retaining every unreplaced value."""

import json
import math
import os
import sys
import tempfile
from pathlib import Path

TIMING = ("mean", "stddev", "min", "max")
PUBLISHABLE = {"ok", "wasm_incompatible"}


def valid_timing(result: dict) -> bool:
    return all(isinstance(result.get(k), (int, float)) and not isinstance(result[k], bool)
               and math.isfinite(result[k]) and (result[k] >= 0 if k == "stddev" else result[k] > 0)
               for k in TIMING)


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as temporary:
            temporary.write(content)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.chmod(name, path.stat().st_mode & 0o777 if path.exists() else 0o644)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line] if path.exists() else []


def failures(rows: list[dict], languages: set[str]) -> list[str]:
    return [f"{r['case']}/{r['lang']}={r['status']}" for r in rows
            if r["lang"] in languages and r["status"] not in PUBLISHABLE]


def hyperfine(path: Path) -> dict:
    return json.loads(path.read_text()) if path.exists() else {"results": []}


def timing_update(destination: Path, update: Path, case: str, langs: set[str]) -> tuple[Path, str]:
    path = destination / "hf" / f"{case}.json"
    old, new = hyperfine(path), hyperfine(update / "hf" / f"{case}.json")
    replacements = {r.get("command"): r for r in new["results"] if valid_timing(r)}
    missing = langs - replacements.keys()
    if missing:
        raise ValueError(f"missing valid timing for {case}: {', '.join(sorted(missing))}; published results left untouched")
    old["results"] = [r for r in old["results"] if r.get("command") not in langs]
    old["results"].extend(replacements[lang] for lang in sorted(langs))
    return path, json.dumps(old, indent=2, allow_nan=False) + "\n"


def raw_update(old: list[dict], new: list[dict]) -> str:
    cells = {(r["case"], r["lang"]): r for r in old}
    for row in new:
        key = row["case"], row["lang"]
        previous = cells.get(key, {})
        cells[key] = {**row, "rss": row["rss"] or previous.get("rss", 0)}
    return "".join(json.dumps(row, separators=(",", ":"), allow_nan=False) + "\n" for row in cells.values())


def merge(destination: Path, update: Path, languages: set[str]) -> None:
    old, new = read_jsonl(destination / "raw.jsonl"), read_jsonl(update / "raw.jsonl")
    broken = failures(new, languages)
    if broken:
        raise ValueError("partial benchmark failed; published results left untouched: " + ", ".join(broken))
    measured = [r for r in new if r["lang"] in languages and r["status"] == "ok"]
    if not measured:
        return
    cases = {r["case"] for r in measured}
    # Validate and serialize EVERY replacement before touching any published file.
    writes = [timing_update(destination, update, case, {r["lang"] for r in measured if r["case"] == case})
              for case in sorted(cases)]
    writes.append((destination / "raw.jsonl", raw_update(old, measured)))
    for path, content in writes:
        atomic_write(path, content)


if __name__ == "__main__":
    if len(sys.argv) < 4:
        raise SystemExit("usage: merge_results.py DESTINATION UPDATE LANGUAGE...")
    merge(Path(sys.argv[1]), Path(sys.argv[2]), set(sys.argv[3:]))
