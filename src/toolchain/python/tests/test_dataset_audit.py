"""Implements [PYTHON-DATASET-AUDIT]: real files, native policy and worker processes."""

import gzip
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import osprey
from osprey.dataset_audit import store
from osprey.dataset_audit.config import Config
from osprey.dataset_audit.runner import run_audit
from osprey.dataset_audit.session import Session


def pair(prompt, completion="A useful, verified answer.", **metadata):
    return {"prompt": prompt, "completion": completion, **metadata}


class DatasetAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="osprey-dataset-audit-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def source(self, name, records):
        path = self.root / name
        text = "".join(json.dumps(record, ensure_ascii=False) + "\n" for record in records)
        if name.endswith(".gz"):
            with gzip.open(path, "wt", encoding="utf-8") as stream:
                stream.write(text)
        else:
            path.write_text(text, encoding="utf-8")
        return path

    def audit(self, train, evaluation=(), **options):
        config = Config(train=tuple(train), evaluation=tuple(evaluation), output=self.root / "output",
                        compiler=os.environ["OSPREY"], batch_size=2, **options)
        return run_audit(config)

    def output(self, name):
        return self.root / "output" / name

    def rows(self, name):
        return [json.loads(line) for line in self.output(name).read_text().splitlines()]

    def test_real_pipeline_emits_clean_and_quarantined_splits(self):
        evaluation = self.source("eval.jsonl", [pair("What causes the aurora?")])
        train = self.source("train.jsonl", [pair("Explain coral reef bleaching", id="keep"),
            pair(" Explain   coral reef bleaching "), pair("Explain coral reef bleaching", "Different target"),
            pair("What causes the aurora?"), pair("Email person@example.invalid for access"),
            pair("AWS credential AKIAABCDEFGHIJKLMNOP"), pair("Empty target", ""),
            pair("x" * 300)])
        report = self.audit([train], [evaluation], max_chars=200)
        self.assertEqual(report["summary"], {"records": 9, "kept": 2, "quarantined": 7})
        self.assertEqual([row["id"] for row in self.rows("train.clean.jsonl")], ["keep"])
        reasons = {row["reason"] for row in self.rows("quarantine.jsonl")}
        self.assertTrue({"duplicate", "conflicting_target", "evaluation_leakage", "personal_data",
                         "exposed_secret", "empty_completion", "context_limit"}.issubset(reasons))

    def test_reports_do_not_republish_sensitive_content(self):
        sensitive = "person@example.invalid"
        secret = "AKIAABCDEFGHIJKLMNOP"
        source = self.source("private.jsonl", [pair(sensitive), pair(secret)])
        self.audit([source])
        for name in ["report.json", "quarantine.jsonl", "records.csv"]:
            text = self.output(name).read_text()
            self.assertNotIn(sensitive, text)
            self.assertNotIn(secret, text)
        self.assertEqual(self.rows("train.clean.jsonl"), [])

    def test_conflicting_targets_and_duplicates_persist_across_worker_batches(self):
        source = self.source("train.jsonl", [pair("Unique question"), pair("Unique question"),
                                             pair("Unique question", "A conflicting answer")])
        config = Config(train=(source,), output=self.root / "output", compiler=os.environ["OSPREY"], batch_size=1)
        report = run_audit(config)
        self.assertEqual(report["summary"]["kept"], 1)
        self.assertEqual([row["reason"] for row in self.rows("quarantine.jsonl")],
                         ["duplicate", "conflicting_target"])
        self.assertEqual(report["execution"]["worker_processes"], 3)

    def test_chat_instruction_dolly_and_gzip_are_real_inputs(self):
        records = [{"messages": [{"role": "user", "content": "Explain gravity"},
                                  {"role": "assistant", "content": "Mass curves spacetime."}]},
                   {"instruction": "Summarize", "input": "Corals are animals", "output": "Corals live."},
                   {"instruction": "Extract a city", "context": "Paris is in France", "response": "Paris"}]
        source = self.source("train.jsonl.gz", records)
        report = self.audit([source])
        self.assertEqual(report["summary"]["kept"], 3)
        self.assertEqual(self.rows("train.clean.jsonl"), records)
        self.assertIsNone(report["tokenizer"])

    def test_near_evaluation_leakage_is_checked_against_the_reference_index(self):
        prompt = " ".join(f"concept{index}" for index in range(60))
        changed = prompt.replace("concept30", "replacement")
        train = self.source("train.jsonl", [pair(changed)])
        evaluation = self.source("eval.jsonl", [pair(prompt)])
        self.audit([train], [evaluation], near_threshold=0.8)
        row = self.rows("quarantine.jsonl")[0]
        self.assertEqual(row["reason"], "near_evaluation_leakage")
        self.assertGreaterEqual(row["reference"]["similarity"], 0.8)
        self.assertEqual(row["reference"]["split"], "eval")

    def test_invalid_and_oversized_lines_do_not_hide_the_following_valid_record(self):
        source = self.root / "invalid.jsonl"
        good = json.dumps(pair("p", "a")).encode() + b"\n"
        source.write_bytes(b'\xff\n{"prompt":"p","prompt":"q"}\n{"n":NaN}\n[\n' + b"x" * 200 + b"\n" + good)
        report = self.audit([source], max_line_bytes=100)
        self.assertEqual(report["summary"], {"records": 6, "kept": 1, "quarantined": 5})
        self.assertEqual(self.rows("train.clean.jsonl"), [pair("p", "a")])

    def test_configuration_errors_leave_no_published_result(self):
        source = self.source("train.jsonl", [pair("Valid input")])
        for options in [{"max_tokens": 1024}, {"near_threshold": 1.2}, {"batch_size": 0}]:
            with self.subTest(options=options), self.assertRaises(ValueError):
                config = Config(train=(source,), output=self.root / "output", compiler=os.environ["OSPREY"], **options)
                run_audit(config)
            self.assertFalse(self.output("report.json").exists())

    def test_compiler_failure_removes_staging_without_publishing(self):
        source = self.source("train.jsonl", [pair("Valid input")])
        config = Config(train=(source,), output=self.root / "output", compiler=str(self.root / "missing-compiler"))
        with self.assertRaises(FileNotFoundError):
            run_audit(config)
        self.assertFalse((self.root / "output").exists())
        self.assertEqual(list(self.root.glob(".output-*")), [])

    def test_inputs_cannot_overlap_or_be_overwritten(self):
        source = self.source("train.jsonl", [pair("Valid input")])
        with self.assertRaises(ValueError):
            self.audit([source], [source])
        self.audit([source])
        before = self.output("report.json").read_bytes()
        with self.assertRaises(FileExistsError):
            self.audit([source])
        self.assertEqual(self.output("report.json").read_bytes(), before)

    def test_hardlink_aliases_cannot_put_one_input_in_both_splits(self):
        source = self.source("train.jsonl", [pair("Valid input")])
        alias = self.root / "eval.jsonl"
        os.link(source, alias)
        with self.assertRaises(ValueError):
            self.audit([source], [alias])
        self.assertFalse(self.output("report.json").exists())

    def test_cli_is_a_data_quality_gate_and_operates_outside_the_checkout(self):
        source = self.source("train.jsonl", [pair("Contact person@example.invalid")])
        result = subprocess.run([sys.executable, "-m", "osprey.dataset_audit", "--train", str(source),
            "--output", str(self.root / "output"), "--compiler", os.environ["OSPREY"]],
            cwd=self.root, capture_output=True, text=True,
            env=dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parents[1])))
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("quarantined", result.stdout)
        self.assertNotIn("person@example.invalid", result.stdout + result.stderr)

    def test_callback_failure_rolls_back_the_entire_native_batch(self):
        source = self.source("train.jsonl", [pair("First"), pair("Second")])
        connection = store.create(self.root / "audit.sqlite3", Config(train=(source,), output=self.root))
        self.addCleanup(connection.close)
        store.ingest(connection, Config(train=(source,), output=self.root))
        library = osprey.build(Path(store.__file__).with_name("policy.py"), compiler=os.environ["OSPREY"])
        session = Session(connection, library, 1, 2)
        handle = library.keep(session)
        self.addCleanup(library.release, handle)
        library.audit_batch(handle, 2, 2)  # Row two succeeds; nonexistent row three fails inside Python.
        with self.assertRaisesRegex(RuntimeError, "row 3"):
            session.finish()
        decided = connection.execute("SELECT COUNT(*) FROM records WHERE decision IS NOT NULL").fetchone()[0]
        self.assertEqual(decided, 0)

    def test_exact_identity_and_personal_data_override_preserve_original_records(self):
        records = [pair("Question with spaces"), pair(" Question  with spaces "),
                   pair("Contact person@example.invalid")]
        source = self.source("train.jsonl", records)
        self.audit([source], normalization="exact", allow_personal_data=True)
        self.assertEqual(self.rows("train.clean.jsonl"), records)
        self.assertEqual(self.output("report.json").stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.output("report.json").parent.stat().st_mode & 0o777, 0o700)
        self.assertFalse(self.output("audit.sqlite3").exists())

    def test_invalid_unicode_and_numeric_overflow_are_quarantined(self):
        source = self.root / "invalid.jsonl"
        source.write_bytes(b'{"prompt":"p","completion":"\\ud800"}\n'
                           b'{"prompt":"p","completion":"a","weight":1e999}\n')
        result = self.audit([source])
        self.assertEqual(result["summary"], {"records": 2, "kept": 0, "quarantined": 2})
        self.assertEqual({row["reason"] for row in self.rows("quarantine.jsonl")}, {"invalid_record"})


if __name__ == "__main__":
    unittest.main()
