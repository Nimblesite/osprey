#!/usr/bin/env python3
"""Replace selected language measurements without disturbing other results."""

import json
import sys
from pathlib import Path


def read_jsonl(path: Path) -> list[dict[str, object]]:
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def merge(destination: Path, update: Path, languages: set[str]) -> None:
    old_raw = read_jsonl(destination / "raw.jsonl")
    new_raw = read_jsonl(update / "raw.jsonl")
    updated_cases = {str(row["case"]) for row in new_raw}

    kept = [
        row for row in old_raw
        if not (str(row["case"]) in updated_cases and str(row["lang"]) in languages)
    ]
    merged = kept + [row for row in new_raw if str(row["lang"]) in languages]
    (destination / "raw.jsonl").write_text(
        "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in merged)
    )

    destination_hf = destination / "hf"
    destination_hf.mkdir(parents=True, exist_ok=True)
    for case in updated_cases:
        update_file = update / "hf" / f"{case}.json"
        destination_file = destination_hf / f"{case}.json"
        old = json.loads(destination_file.read_text()) if destination_file.exists() else {"results": []}
        new = json.loads(update_file.read_text()) if update_file.exists() else {"results": []}
        results = [r for r in old.get("results", []) if r.get("command") not in languages]
        results.extend(r for r in new.get("results", []) if r.get("command") in languages)
        old["results"] = results
        destination_file.write_text(json.dumps(old, indent=2) + "\n")


if __name__ == "__main__":
    if len(sys.argv) < 4:
        raise SystemExit("usage: merge_results.py DESTINATION UPDATE LANGUAGE...")
    merge(Path(sys.argv[1]), Path(sys.argv[2]), set(sys.argv[3:]))
