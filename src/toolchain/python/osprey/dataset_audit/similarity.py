"""Bounded candidate search with exact Jaccard verification; not exhaustive recall."""

import hashlib
import re

SKETCH_SIZE = 16
CANDIDATE_LIMIT = 50
SHINGLE_WIDTH = 5


def shingles(text):
    words = re.findall(r"\w+", text.casefold())
    if not words:
        return set()
    fragments = [" ".join(words[index:index + SHINGLE_WIDTH])
                 for index in range(max(1, len(words) - SHINGLE_WIDTH + 1))]
    return {hashlib.blake2b(fragment.encode("utf-8"), digest_size=8).digest() for fragment in fragments}


def index_record(connection, row):
    values = sorted(shingles(row["prompt"]))[:SKETCH_SIZE]
    connection.executemany("INSERT OR IGNORE INTO sketches VALUES (?, ?)", ((value, row["id"]) for value in values))


def nearest(connection, row):
    values = shingles(row["prompt"])
    sketch = sorted(values)[:SKETCH_SIZE]
    if not sketch:
        return {"train": None, "eval": None}
    candidates = _candidates(connection, row, sketch)
    matches = {"train": None, "eval": None}
    for candidate in candidates:
        other = shingles(candidate["prompt"])
        score = len(values & other) / len(values | other)
        previous = matches[candidate["split"]]
        if previous is None or score > previous["similarity"]:
            matches[candidate["split"]] = reference(candidate, score)
    return matches


def _candidates(connection, row, sketch):
    placeholders = ",".join("?" for _ in sketch)
    query = f"""SELECT r.*, COUNT(*) AS shared FROM sketches s JOIN records r ON r.id=s.record_id
        WHERE s.value IN ({placeholders}) AND r.id<>?
          AND ((?='train' AND r.split='eval') OR (r.split=? AND r.decision=0))
        GROUP BY r.id ORDER BY shared DESC, r.id ASC LIMIT ?"""
    return connection.execute(query, (*sketch, row["id"], row["split"], row["split"], CANDIDATE_LIMIT)).fetchall()


def reference(row, similarity=1.0):
    return {"source": row["source"], "line": row["line"], "split": row["split"], "similarity": similarity}
