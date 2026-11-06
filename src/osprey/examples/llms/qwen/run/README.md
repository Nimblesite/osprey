# Qwen3.8-27B on Metal

The real model on the real weights, greedy, through the package: `Qwen3_5Gguf` maps the
GGUF, the tokenizer is built from the file's own `tokenizer.ggml.*` tables, and every
token prints as `position token -> next`, so a run is its own receipt.

```
printf '%s\n%s\n%s\n' ~/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf 'The capital of France is' 61 \
  | OSPREY_GPU=metal osprey src/osprey/examples/llms/qwen/run --run --memory=arc
```

Three lines on stdin: the GGUF path, the prompt (no chat template: the raw text), and
how many tokens to add. The output ends with the added ids and the text they spell.

It needs about 18 GB of the machine's memory for the wired weights. Receipts live in
`target/local-coding-model/runs/`: on 2026-11-07 (`2026-11-07_080211-package-real-weights-france.txt`)
"The capital of France is" continued " Paris.\nThe capital of Germany is", token for
token the ids the earlier one-file program produced (11751 13 198 760 6511 314 9564 369),
and 61 tokens went on through Berlin, Rome, Madrid, Lisbon, Athens, London and Dublin.
