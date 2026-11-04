"""Python services called by native Osprey; decisions call native policy back."""

import json

from . import schema, similarity, tokenizer


class Session:
    def __init__(self, connection, library, start, count):
        self.connection, self.library = connection, library
        self.start, self.count = start, count
        self.settings = json.loads(connection.execute("SELECT value FROM settings WHERE key='config'").fetchone()[0])
        self.tokenizer = tokenizer.load(self.settings["tokenizer_dir"])
        self.failure = None

    def inspect(self, row_id):
        if self.failure:
            raise RuntimeError("An earlier audit callback failed")
        try:
            _inspect(self, row_id)
        except Exception as error:
            self.failure = f"row {row_id}: {type(error).__name__}"
            raise RuntimeError(self.failure) from error

    def finish(self):
        completed = self.connection.execute("SELECT COUNT(*) FROM records WHERE id>=? AND id<? AND decision IS NOT NULL",
                                            (self.start, self.start + self.count)).fetchone()[0]
        if self.failure or completed != self.count:
            self.connection.rollback()
            raise RuntimeError(self.failure or "The native batch did not complete every requested record")
        self.connection.commit()
        return completed


def _inspect(session, row_id):
    row = session.connection.execute("SELECT * FROM records WHERE id=?", (row_id,)).fetchone()
    if row is None or not session.start <= row_id < session.start + session.count or row["decision"] is not None:
        raise RuntimeError("The native batch requested an invalid or repeated record")
    overlaps = _overlaps(session, row)
    count = _tokens(session, row)
    decision = session.library.decide(**_features(row, count, overlaps, session.settings))
    if not session.library.valid_decision(decision):
        raise RuntimeError("The native policy returned an invalid decision")
    reason = session.library.reason_name(decision)
    reference = _reference(decision, row["split"], overlaps)
    session.connection.execute("UPDATE records SET decision=?, reason=?, reference=?, token_count=? WHERE id=?",
                               (decision, reason, json.dumps(reference), count, row_id))
    if decision == 0 and row["split"] == "train" and session.settings["near_threshold"] is not None:
        similarity.index_record(session.connection, row)


def _overlaps(session, row):
    connection = session.connection
    evaluation = connection.execute("SELECT * FROM records WHERE split='eval' AND prompt_hash=? ORDER BY id LIMIT 1",
                                    (row["prompt_hash"],)).fetchone() if row["split"] == "train" and not row["issue"] else None
    previous = connection.execute("SELECT * FROM records WHERE split=? AND prompt_hash=? AND decision=0 ORDER BY id LIMIT 1",
                                  (row["split"], row["prompt_hash"])).fetchone() if not row["issue"] else None
    near = similarity.nearest(connection, row) if session.settings["near_threshold"] is not None and not row["issue"] else {"train": None, "eval": None}
    return {"evaluation": evaluation, "previous": previous, "near": near}


def _tokens(session, row):
    if row["issue"] or session.tokenizer is None:
        return None
    if session.settings["max_chars"] and row["chars"] > session.settings["max_chars"]:
        return None
    value = schema.document(json.loads(row["original"]))
    return tokenizer.count(session.tokenizer, value, session.settings["separator"])


def _score(reference):
    return float(reference["similarity"]) if reference else 0.0


def _features(row, tokens, overlaps, config):
    findings = json.loads(row["findings"])
    previous = overlaps["previous"]
    return {"valid": not row["issue"], "secret": any(name.startswith("secret.") for name in findings),
            "personal": any(name.startswith("pii.") for name in findings), "allow_personal": config["allow_personal_data"],
            "is_train": row["split"] == "train", "evaluation_match": overlaps["evaluation"] is not None,
            "evaluation_similarity": _score(overlaps["near"]["eval"]), "duplicate": previous is not None,
            "conflict": previous is not None and previous["target_hash"] != row["target_hash"],
            "local_similarity": _score(overlaps["near"][row["split"]]), "threshold": float(config["near_threshold"] or 0.0),
            "chars": row["chars"], "max_chars": config["max_chars"], "tokens": tokens or 0,
            "max_tokens": config["max_tokens"] or 0, "completion_chars": row["completion_chars"],
            "minimum_completion": config["min_completion_chars"]}


def _reference(code, split, overlaps):
    if code == 4:
        return similarity.reference(overlaps["evaluation"])
    if code in {5, 6}:
        return similarity.reference(overlaps["previous"])
    if code in {7, 8}:
        return overlaps["near"]["eval" if code == 7 else split]
    return None
