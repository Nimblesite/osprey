# Audit LLM fine-tuning data before a GPU training run

`osprey-dataset-audit` turns local training and evaluation JSONL into cleaned datasets and actionable quarantine reports. It catches evaluation leakage, repeated prompts with conflicting answers, duplicates, sensitive-data patterns, malformed examples and examples that exceed the model's context budget. This is a usable preparation and quality gate for supervised fine-tuning jobs.

Osprey's Python flavor owns the decision policy and an `Audit` effect. Its handler calls Python services for each record; those services call native Osprey policy back before writing a decision. Python provides streaming gzip/JSONL, SQLite indexing and the actual Hugging Face tokenizer. Changing acceptance policy means editing [policy.py](policy.py), rather than changing data ingestion. Osprey compiles this file; CPython does not import it.

## Run on your data

**Compiler migration:** the current Python build helper omits the `--memory=none` option now required by the Python-library target. The command below depends on that helper and is blocked with the GC-default compiler until the helper is updated. The [interop guide](../../README.md) gives an explicit compile-and-load workflow for scalar libraries.

Build the Osprey compiler/runtime, install the package, then run:

```sh
python3 -m pip install './python[dataset-audit]'
export OSPREY=/absolute/path/to/Osprey/target/debug/osprey
osprey-dataset-audit \
  --train /data/train.jsonl.gz \
  --eval /data/evaluation.jsonl \
  --tokenizer-dir /models/your-model-tokenizer \
  --max-tokens 4096 \
  --near-threshold 0.85 \
  --output /data/audited-v1
```

Repeat `--train` and `--eval` for multiple files. The output directory must be new. Inputs are read-only, fingerprinted and checked for changes during ingestion. The tokenizer directory must already exist locally: the application never downloads model weights, tokenizers or remote code. Without token counting, install `./python` and omit both tokenizer options; the rest of the application uses the standard library.

Exit status is `0` for a clean audit, `2` when records were quarantined, and `1` for an operational/configuration failure. `--allow-quarantine` changes the quality-gate exit status to `0` after producing cleaned data; it does not keep rejected records. A training pipeline can require review of the report before proceeding.

### Accepted records

Each physical line is one JSON object, in one of these text-only formats:

```json
{"prompt":"Explain gradient clipping","completion":"Limit the gradient norm before updating weights."}
{"instruction":"Summarize","input":"The input document","output":"The summary"}
{"instruction":"Extract the answer","context":"A source passage","response":"The answer"}
{"messages":[{"role":"user","content":"Explain overfitting"},{"role":"assistant","content":"Fitting training-specific noise."}]}
```

The third form supports Databricks Dolly directly. Chat requires a final assistant answer and nonempty user context. Multimodal content, ambiguous schemas and missing/non-text required fields are rejected. Additional metadata and the original JSON values are preserved in clean output. Invalid UTF-8, duplicate JSON keys, non-finite numbers and oversized lines are quarantined; a bad line does not consume the next example. Blank lines are invalid records.

### Outputs

| File | Use |
| --- | --- |
| `train.clean.jsonl` | Accepted training examples, in input order |
| `eval.clean.jsonl` | Accepted evaluation examples |
| `quarantine.jsonl` | Rejected source locations, reasons, fingerprints and overlap references |
| `records.csv` | Decision and token-count inventory for every record |
| `report.json` | Totals by split/reason, input and tokenizer fingerprints, settings and policy version/hash |

Reports and quarantine contain record metadata, not rejected prompt/answer text. Source paths remain visible. Find an original example using its source and line; rejected secrets are not copied into diagnostic artifacts. Output directories are private (`0700`), files private (`0600`). `--keep-database` explicitly retains the working SQLite database **including original input content**; otherwise it is removed. Keep the original inputs under your own retention policy.

The application publishes the output directory only after every worker and export succeeds. A failed callback, tokenizer error, crash or timeout leaves no completed output directory. A per-output file lock prevents concurrent cooperating writers. Do not modify inputs or tokenizer files during an audit.

## Policy and interpretation

Evaluation files are indexed before training. Any exact evaluation-prompt match quarantines the training example, even if that evaluation example is itself quarantined. Among accepted records of a split, a repeated prompt is a duplicate when its answer matches, and a conflicting target otherwise. The first accepted example wins; no records are silently merged.

Default prompt/answer identity uses Unicode NFKC and collapsed whitespace, preserves case and includes chat roles/history. `--normalization exact` preserves whitespace and Unicode distinctions for identity. It does not alter the original records in either mode. Near matching separately uses case-folded word shingles: `--near-threshold` enables an indexed candidate search and exact Jaccard scoring of candidates. It has bounded candidate counts and **does not guarantee exhaustive recall or semantic equivalence**. Inspect references before treating near matches as annotation errors.

Privacy checks cover named patterns for AWS/GitHub/API credentials, private keys, email addresses, international telephone numbers and populated secret-named metadata fields. They are heuristics with false positives and false negatives, not a compliance or comprehensive secret-detection service. `--allow-personal-data` allows PII-pattern matches and keeps their findings in the inventory; it never allows recognized secrets. Pattern matching covers nested string metadata as well as training text.

For prompt/completion, instruction and Dolly records, token counts use the actual local tokenizer on `prompt + separator + completion`, with special tokens enabled and no truncation. Instruction context is joined with two newlines; the default separator is also two newlines, configurable with `--separator`. Match this to your training formatter. Chat uses the tokenizer's own chat template, including all messages, with `add_generation_prompt=False`; a missing template is an error. A requested token budget without a local tokenizer is an error. `--max-chars` defaults to 32,000; set it to `0` to disable this independent character cap. Over-character-limit records do not need tokenization.

One reason is assigned by priority: invalid structure, secret/PII, evaluation leakage, conflicting target/duplicate, then context budget/empty completion. Findings remain available alongside that primary reason. The kept dataset is ready for its existing training formatter; the tool does not manufacture labels, repair answers or launch training.

## Actual Dolly/GPT-2 acceptance

A full run on the public [Databricks Dolly 15k dataset](https://huggingface.co/datasets/databricks/databricks-dolly-15k) exercised **15,011 actual records**, holding the first 100 out as evaluation. It used the local [GPT-2 tokenizer](https://huggingface.co/openai-community/gpt2), a 1,024-token budget, the default character/privacy policy and near threshold `0.85`. No model weights or GPU were required.

| Result | Records |
| --- | ---: |
| Kept | 14,342 |
| Quarantined | 669 |
| Conflicting target | 169 |
| Exact duplicate | 13 |
| Exact evaluation overlap | 4 |
| Candidate near duplicate | 251 |
| Candidate near evaluation overlap | 41 |
| Context limit | 184 |
| Personal-data pattern | 7 |

These are policy findings, not manually adjudicated dataset defects. The run tokenized all 15,011 records through Transformers 5.20.0, with 8 fresh native workers. It retained 14,245 training and 97 evaluation records. Every row crossed Python → Osprey → Python → Osprey; the normal test suite also forces a callback failure and verifies transaction rollback.

Reproduce with an installed package. Download the dataset and tokenizer separately (the dataset is CC BY-SA 3.0; follow its attribution/share-alike requirements when redistributing data or adaptations):

```python
from pathlib import Path
from urllib.request import urlretrieve
from huggingface_hub import snapshot_download

root = Path("dolly-audit-inputs")
root.mkdir(exist_ok=True)
urlretrieve("https://huggingface.co/datasets/databricks/databricks-dolly-15k/resolve/main/databricks-dolly-15k.jsonl", root / "dolly.jsonl")
lines = (root / "dolly.jsonl").read_bytes().splitlines(keepends=True)
(root / "eval.jsonl").write_bytes(b"".join(lines[:100]))
(root / "train.jsonl").write_bytes(b"".join(lines[100:]))
snapshot_download("openai-community/gpt2", local_dir=root / "tokenizer",
                  allow_patterns=["config.json", "tokenizer.json", "tokenizer_config.json", "vocab.json", "merges.txt"])
```

Run the earlier command using these files, `--max-tokens 1024` and `--near-threshold 0.85`. The report records exact content hashes; upstream revisions or dependency changes can change results. The checked run's full-source SHA-256 is recorded in [the acceptance notes](../../../examples/python/dataset-audit-acceptance.md).

## Runtime boundaries

This initial native host supports Linux/macOS, one loading thread and the explicit unmanaged `none` allocator. The driver uses ordinary value effects; it does not use fibers or dynamic continuations across Python. Native general allocations are retained, so each configurable batch (default 2,000 records) runs in a fresh process, commits its SQLite transaction and exits. This bounds native lifetime per batch, not a fixed byte memory ceiling; individual line size is separately bounded. SQLite holds cross-batch indexes on disk. `--worker-timeout` defaults to 300 seconds per batch.

No performance speedup, GPU computation or tensor-storage interchange is claimed by this audit. Its practical role is preparing data before costly training. [Spec 0042](../../../docs/specs/platforms/0042-PythonInterop.md) defines the bridge; [plan 0032](../../../docs/plans/0032-python-interop.md) tracks typed direct calls, memory reclamation and tensor storage.
