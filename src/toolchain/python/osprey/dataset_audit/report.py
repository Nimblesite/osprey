"""Useful cleaned datasets, per-record diagnostics and reproducible provenance."""

from collections import Counter
from contextlib import ExitStack
import csv
from dataclasses import asdict
from datetime import datetime, timezone
import json
from pathlib import Path

from .output import private_file
from .records import file_digest

FIELDS = ["source", "line", "split", "reason", "issue", "sha256", "tokens", "findings", "reference_source", "reference_line", "similarity"]


def export(connection, staging, config, sources, tokenizer, processes):
    missing = connection.execute("SELECT COUNT(*) FROM records WHERE decision IS NULL").fetchone()[0]
    if missing:
        raise RuntimeError("The audit has undecided records; refusing to publish")
    counts = _export_records(connection, staging)
    report = _summary(connection, config, sources, tokenizer, processes, counts)
    with private_file(staging / "report.json") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2, sort_keys=True)
        stream.write("\n")
    return report


def _export_records(connection, staging):
    counts = Counter()
    with ExitStack() as stack:
        clean = {split: stack.enter_context(private_file(staging / f"{split}.clean.jsonl")) for split in ["train", "eval"]}
        quarantine = stack.enter_context(private_file(staging / "quarantine.jsonl"))
        csv_file = stack.enter_context(private_file(staging / "records.csv"))
        writer = csv.DictWriter(csv_file, fieldnames=FIELDS)
        writer.writeheader()
        for row in connection.execute("SELECT * FROM records ORDER BY id"):
            counts[row["reason"]] += 1
            _export_row(row, clean, quarantine, writer)
    return counts


def _export_row(row, clean, quarantine, writer):
    detail = _detail(row)
    if row["decision"] == 0:
        clean[row["split"]].write(json.dumps(json.loads(row["original"]), ensure_ascii=False, allow_nan=False) + "\n")
    else:
        quarantine.write(json.dumps(detail, ensure_ascii=False, allow_nan=False) + "\n")
    flat = {name: detail.get(name, "") for name in FIELDS}
    flat["findings"] = ";".join(detail["findings"])
    reference = detail["reference"] or {}
    flat.update(reference_source=reference.get("source", ""), reference_line=reference.get("line", ""), similarity=reference.get("similarity", ""))
    writer.writerow(flat)


def _detail(row):
    return {"source": row["source"], "line": row["line"], "split": row["split"], "reason": row["reason"],
            "issue": row["issue"], "sha256": row["raw_hash"], "tokens": row["token_count"],
            "findings": json.loads(row["findings"]), "reference": json.loads(row["reference"] or "null")}


def _summary(connection, config, sources, tokenizer, processes, counts):
    kept = counts.get("keep", 0)
    total = sum(counts.values())
    return {"summary": {"records": total, "kept": kept, "quarantined": total - kept},
            "reasons": dict(sorted(counts.items())), "splits": _splits(connection), "sources": sources,
            "tokenizer": tokenizer, "configuration": json.loads(json.dumps(asdict(config), default=str)),
            "policy": {"version": 1, "source_sha256": file_digest(Path(__file__).with_name("policy.py"))},
            "execution": {"worker_processes": processes, "native_allocator": "default", "bounded_worker_rows": config.batch_size},
            "created_at": datetime.now(timezone.utc).isoformat(),
            "limits": {"privacy": "named heuristic patterns, not comprehensive PII or secret detection",
                       "near_duplicates": "candidate-based Jaccard verification; recall is not exhaustive"}}


def _splits(connection):
    return {row["split"]: {"records": row["records"], "kept": row["kept"], "quarantined": row["records"] - row["kept"],
                           "tokenized_records": row["tokenized"], "kept_tokens": row["tokens"]}
            for row in connection.execute("""SELECT split, COUNT(*) AS records, SUM(decision=0) AS kept,
                COUNT(token_count) AS tokenized, SUM(CASE WHEN decision=0 THEN token_count END) AS tokens
                FROM records GROUP BY split""")}
