"""Bounded JSONL readers, strict JSON and source provenance."""

import gzip
import hashlib
import json
from osprey._json import finite_float


def _pairs(items):
    result = {}
    for key, value in items:
        key.encode("utf-8")
        if key in result:
            raise ValueError("duplicate_json_key")
        result[key] = value
    return result


def _constant(_value):
    raise ValueError("non_finite_json")


def parse(raw):
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_pairs, parse_constant=_constant, parse_float=finite_float), ""
    except (UnicodeError, ValueError, RecursionError):
        return None, "invalid_json"


def lines(path, limit):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rb") as stream:
        number = 0
        while raw := stream.readline(limit + 1):
            number += 1
            oversized = len(raw) > limit
            fingerprint = hashlib.sha256(raw)
            if oversized:
                _drain(stream, raw, limit, fingerprint)
            record, issue = (None, "line_too_large") if oversized else parse(raw)
            yield number, record, issue, fingerprint.hexdigest()


def _drain(stream, raw, limit, fingerprint):
    while raw and not raw.endswith(b"\n"):
        raw = stream.readline(limit + 1)
        fingerprint.update(raw)


def file_digest(path):
    fingerprint = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            fingerprint.update(block)
    return fingerprint.hexdigest()


def snapshot(path):
    value = path.stat()
    return value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns
