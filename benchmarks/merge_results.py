#!/usr/bin/env python3
"""Replace selected language measurements without disturbing other results."""

import json
import sys
from pathlib import Path

# Statuses a partial run may publish. `wasm_incompatible` is a legitimate skip
# (the case uses a non-portable builtin), so it carries no measurement and must
# not veto the merge. Anything else means the re-run produced no trustworthy
# number for that cell — publishing it would overwrite a good published record
# with a hole, so the whole merge is refused and the destination left untouched.
PUBLISHABLE = {"ok", "wasm_incompatible"}


def read_jsonl(path: Path) -> list[dict[str, object]]:
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def failures(rows: list[dict[str, object]], languages: set[str]) -> list[str]:
    return [
        f"{row['case']}/{row['lang']}={row['status']}"
        for row in rows
        if str(row["lang"]) in languages and str(row["status"]) not in PUBLISHABLE
    ]


def merge(destination: Path, update: Path, languages: set[str]) -> None:
    old_raw = read_jsonl(destination / "raw.jsonl")
    new_raw = read_jsonl(update / "raw.jsonl")
    broken = failures(new_raw, languages)
    if broken:
        raise ValueError(
            "partial benchmark failed; published results left untouched: " + ", ".join(broken)
        )
    # Replacement is keyed on the (case, language) pairs the re-run ACTUALLY
    # measured, never on the language list it was asked to measure. A language
    # whose toolchain is absent writes no record, and keying on the list alone
    # deleted its published row for every re-run case.
    measured = {(str(r["case"]), str(r["lang"])) for r in new_raw if str(r["lang"]) in languages}

    kept = [row for row in old_raw if (str(row["case"]), str(row["lang"])) not in measured]
    merged = kept + [row for row in new_raw if str(row["lang"]) in languages]
    (destination / "raw.jsonl").write_text(
        "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in merged)
    )

    destination_hf = destination / "hf"
    destination_hf.mkdir(parents=True, exist_ok=True)
    for case in {c for c, _ in measured}:
        replaced = {lang for c, lang in measured if c == case}
        update_file = update / "hf" / f"{case}.json"
        destination_file = destination_hf / f"{case}.json"
        old = json.loads(destination_file.read_text()) if destination_file.exists() else {"results": []}
        new = json.loads(update_file.read_text()) if update_file.exists() else {"results": []}
        results = [r for r in old.get("results", []) if r.get("command") not in replaced]
        results.extend(r for r in new.get("results", []) if r.get("command") in replaced)
        old["results"] = results
        destination_file.write_text(json.dumps(old, indent=2) + "\n")


if __name__ == "__main__":
    if len(sys.argv) < 4:
        raise SystemExit("usage: merge_results.py DESTINATION UPDATE LANGUAGE...")
    merge(Path(sys.argv[1]), Path(sys.argv[2]), set(sys.argv[3:]))
