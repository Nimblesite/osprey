# Dataset-audit acceptance

The [installed application](../../../toolchain/python/osprey/dataset_audit/README.md) prepares actual LLM fine-tuning data. Its native Python-flavor effect driver calls Python services, which call native policy back for every record. The small `interop.osp`/`interop.py` files in this directory remain quick bridge checks; the dataset audit is the substantial application.

## Public-data run

On 2026-10-16, an installed wheel audited the complete Databricks Dolly JSONL with the first 100 records held out as evaluation, GPT-2 tokenization, a 1,024-token budget, near threshold `0.85` and default privacy/character policy. No model weights, GPU or PyTorch were used. The optional environment used Python 3.12.14, Transformers 5.20.0 and Tokenizers 0.23.3.

| Artifact | SHA-256 |
| --- | --- |
| Complete Dolly JSONL (15,011 records) | `2df9083338b4abd6bceb5635764dab5d833b393b55759dffb0959b6fcbf794ec` |
| Evaluation JSONL (first 100 records) | `71b9d81379c47c768ff6fcb338253cf54f5006622a0e312b457cf2d407cca47b` |
| Training JSONL (remaining 14,911 records) | `84ec800f60d5ef498daf6e0668d20af1d292eb340c13b0c0cb21d16b1414fa00` |
| GPT-2 `tokenizer.json` | `8414cab924d8b9b33013f0d221c5862f365ee9be39c5c2bfae8a5a9e970478a6` |

The run kept **14,342** records and quarantined **669**. Training retained 14,245 records / 2,180,270 tokens; evaluation retained 97 / 14,853 tokens. A separate check reconstructed the tokenizer inputs from every kept original record, recomputed every token count, checked the 1,024-token cap and confirmed those totals. The CSV accounts for all 15,011 unique `(source, line)` pairs exactly once. Reason counts and reproduction commands are in the application guide. Findings have not been manually adjudicated.

The application wrote clean train/evaluation JSONL, a metadata-only quarantine, a per-record CSV and a provenance report through eight fresh native workers. A nonzero quality-gate exit (`2`) correctly signaled quarantined records while preserving the complete result for review. No input dataset, model files or generated output is vendored in this repository.

Source: [Databricks Dolly dataset/card](https://huggingface.co/datasets/databricks/databricks-dolly-15k), [Databricks Dolly project](https://github.com/databrickslabs/dolly), [GPT-2 tokenizer](https://huggingface.co/openai-community/gpt2). Dolly is CC BY-SA 3.0; follow its attribution and share-alike requirements when redistributing data or adaptations.

## Repeatable checks

The normal integration gate uses the compiler Cargo builds and no optional dependencies/downloads:

```sh
cargo test -p osprey-cli --test python_interop
```

It covers both directions in Default/ML/Python, checked scalars and JSON, object lifetimes, loading/thread restrictions, and the real application with disk-backed inputs/workers. Application regressions include callback failure after an earlier row succeeded (entire batch rolls back), cross-batch conflicts, sensitive-data handling, malformed/oversized records, Unicode/numeric overflow, output permissions, input aliases and failed-publication cleanup.

For actual Hugging Face chat formatting, install the extra and run the additional offline acceptance suite:

```sh
python3 -m pip install './python[dataset-audit]'
OSPREY="$PWD/target/debug/osprey" PYTHONPATH="$PWD/src/toolchain/python" \
  python3 -m unittest discover -s src/toolchain/python/acceptance -v
```

This uses a real local tokenizer and saved chat template, with no network. It independently renders and tokenizes short/long chats, verifies the recorded counts and context quarantine, and requires a missing template to fail without publishing. The regression caught Transformers 5 returning `BatchEncoding` by default: counting its fields produced `2` for both examples rather than `6` and `105` tokens. Explicit `return_dict=False` fixes the count across the supported API. The extra includes Jinja2, which current Transformers requires for chat templates.

Opaque references also passed an installed NumPy 2.5.4 CPU matrix-method call, preserving the original input object/storage identity. That is Python-object interoperability evidence. Neither it nor the audit claims Osprey GPU dispatch, tensor storage interchange or a measured speedup. Those have separate acceptance requirements in [plan 0031](../../../../docs/plans/0031-python.md).
