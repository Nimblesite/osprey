"""Optional real Hugging Face integration; installed extras, offline, no model weights."""

import csv
import json
import os
from pathlib import Path
import tempfile
import unittest

from tokenizers import Tokenizer, models, pre_tokenizers
from transformers import PreTrainedTokenizerFast

from osprey.dataset_audit.config import Config
from osprey.dataset_audit.runner import run_audit

TEMPLATE = "{% for message in messages %}{{ message['role'] + ': ' + message['content'] + '\\n' }}{% endfor %}"


def local_tokenizer(directory, template):
    backend = Tokenizer(models.WordLevel({"[UNK]": 0, "user": 1, "assistant": 2, ":": 3, "word": 4}, unk_token="[UNK]"))
    backend.pre_tokenizer = pre_tokenizers.Whitespace()
    tokenizer = PreTrainedTokenizerFast(tokenizer_object=backend, unk_token="[UNK]", chat_template=template)
    tokenizer.save_pretrained(directory)
    return tokenizer


class TokenizerAcceptance(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="osprey-hf-acceptance-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.messages = [{"role": "user", "content": "word"}, {"role": "assistant", "content": "word"}]
        self.records = [{"messages": self.messages}, {"messages": [
            {"role": "user", "content": "word " * 100}, {"role": "assistant", "content": "word"}]}]
        self.source = self.root / "train.jsonl"
        self.source.write_text("".join(json.dumps(record) + "\n" for record in self.records))

    def audit(self):
        return run_audit(Config(train=(self.source,), output=self.root / "output", compiler=os.environ["OSPREY"],
                                tokenizer_dir=self.root / "tokenizer", max_tokens=16))

    def test_chat_budget_counts_tokens_instead_of_batch_encoding_fields(self):
        tokenizer = local_tokenizer(self.root / "tokenizer", TEMPLATE)
        expected = [len(tokenizer.encode(tokenizer.apply_chat_template(record["messages"], tokenize=False),
                                         add_special_tokens=False)) for record in self.records]
        report = self.audit()
        with (self.root / "output/records.csv").open() as stream:
            actual = [int(row["tokens"]) for row in csv.DictReader(stream)]
        self.assertEqual(actual, expected)
        self.assertGreater(actual[1], 16)
        self.assertEqual(report["summary"], {"records": 2, "kept": 1, "quarantined": 1})

    def test_missing_chat_template_fails_without_publishing(self):
        local_tokenizer(self.root / "tokenizer", None)
        with self.assertRaisesRegex(ValueError, "chat template"):
            self.audit()
        self.assertFalse((self.root / "output").exists())


if __name__ == "__main__":
    unittest.main()
