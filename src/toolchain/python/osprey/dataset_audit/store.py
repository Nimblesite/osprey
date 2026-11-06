"""A disk index makes evaluation leakage and cross-batch decisions deterministic."""

import json
import os
import sqlite3

from . import privacy, records, schema, similarity

SCHEMA = """
CREATE TABLE records (
 id INTEGER PRIMARY KEY, split TEXT NOT NULL, source TEXT NOT NULL, line INTEGER NOT NULL,
 raw_hash TEXT NOT NULL, original TEXT, kind TEXT, prompt TEXT, prompt_hash TEXT, target_hash TEXT,
 chars INTEGER NOT NULL, completion_chars INTEGER NOT NULL, findings TEXT NOT NULL, issue TEXT NOT NULL,
 token_count INTEGER, decision INTEGER, reason TEXT, reference TEXT);
CREATE INDEX prompt_lookup ON records(split, prompt_hash, decision, id);
CREATE TABLE sketches (value BLOB NOT NULL, record_id INTEGER NOT NULL, PRIMARY KEY(value, record_id));
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"""


def connect(path):
    connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys=ON")
    return connection


def create(path, config):
    connection = connect(path)
    os.chmod(path, 0o600)
    connection.executescript(SCHEMA)
    settings = {name: getattr(config, name) for name in ["normalization", "max_chars", "min_completion_chars",
                "max_tokens", "near_threshold", "separator", "allow_personal_data"]}
    settings["tokenizer_dir"] = str(config.tokenizer_dir) if config.tokenizer_dir else None
    connection.execute("INSERT INTO settings VALUES ('config', ?)", (json.dumps(settings),))
    connection.commit()
    return connection


def ingest(connection, config):
    sources = []
    for split, paths in [("eval", config.evaluation), ("train", config.train)]:
        for path in paths:
            sources.append(_ingest_file(connection, path, split, config))
    connection.commit()
    return sources


def _ingest_file(connection, path, split, config):
    before = records.snapshot(path)
    fingerprint = records.file_digest(path)
    count = 0
    for line, record, issue, raw_hash in records.lines(path, config.max_line_bytes):
        _insert(connection, split, path, line, record, issue, raw_hash, config)
        count += 1
    if records.snapshot(path) != before:
        raise RuntimeError(f"Input changed while auditing: {path}")
    return {"path": str(path), "split": split, "records": count, "sha256": fingerprint, "bytes": before[2]}


def _properties(record, issue, mode):
    if issue:
        return None, "", "", "", 0, 0, [], issue
    try:
        findings = privacy.findings(record)
        value = schema.document(record)
        prompt, prompt_hash, target_hash = schema.identities(value, mode)
        return value.kind, prompt, prompt_hash, target_hash, value.chars, len(value.target.strip()), findings, ""
    except (ValueError, UnicodeError, RecursionError) as error:
        return None, "", "", "", 0, 0, [], str(error) if type(error) is ValueError else "invalid_text"


def _insert(connection, split, path, line, record, issue, raw_hash, config):
    properties = _properties(record, issue, config.normalization)
    original = json.dumps(record, ensure_ascii=True, allow_nan=False) if record is not None else None
    connection.execute("""INSERT INTO records
        (split,source,line,raw_hash,original,kind,prompt,prompt_hash,target_hash,chars,completion_chars,findings,issue)
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
        (split, str(path), line, raw_hash, original, *properties[:6], json.dumps(properties[6]), properties[7]))
    if split == "eval" and config.near_threshold is not None and not properties[7]:
        row = connection.execute("SELECT * FROM records WHERE id=last_insert_rowid()").fetchone()
        similarity.index_record(connection, row)
