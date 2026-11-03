# Plan 0049 — A local coding model on Osprey: Qwen3.8-27B

Contract: ../specs/runtime/0034-GPUComputation.md, ../specs/ecosystem/0043-PythonFlavorPorts.md, ../specs/runtime/0014-HTTP.md, ../specs/applications/0053-LocalCodingModel.md (written in Phase 1; owns the `LLM-*` IDs below)
Depends on: 0050-sized-numeric-types.md (Phase 5 gate), 0048-runtime-memory-safety.md (its items land before Phase 7)

## Prerequisites in place
- Model pins: `Qwen/Qwen3.8-27B` revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`, weights from `unsloth/Qwen3.8-27B-GGUF` revision `4ca720788d1e01f1bff70c033e0d0028fd02e502`; fallback `Qwen/Qwen3.6-27B` (same `qwen3_5` architecture, 27,781,427,952 params) is a weights swap if 3.8 fails Phase 0.
- Harnesses already ported: `@osprey/pydantic-ai` `OllamaProvider` (packages/pydantic_ai/src/providers_http.py) and the tool-calling `Agent` (packages/pydantic_ai/src/agent.py).
- Reusable: `@osprey/tiktoken` byte-level BPE (packages/tiktoken/src/core.py); `@osprey/transformers` top-k/top-p (packages/transformers/src/logits_process.py) and model-independent `generate` (generation.py); `@osprey/jinja2` parses macros (parser.py `parse_macro`) but rendering errors (render.py:102); `@osprey/tokenizers` has no BPE and no NFC.
- `HttpResponse` has `streamFd` (compiler/runtime/http_shared.h:57); `gpuLoad`/`gpuLoadTensor` copy and widen (compiler/runtime/gpu_host.c), and `GpuBuffer<float>` stores binary64.

## Phase 0 — Baseline, before any port code
- [ ] Free disk: 16 GiB free (99% full) — needs 60 GB for two GGUFs. `target` (13 GB) and `hosts/gpui/target` (14 GB) are shared with other agents, so coordinate through Too Many Cooks first.
- [ ] `apps/llm/eval/`: 30 coding tasks with deterministic pass/fail scripts (this repo's closed bugs and small features) + 200 tool-call cases covering nested objects, multi-line strings, enums and error recovery over 15+ turns.
- [ ] Run them on real Ollama with the unmodified GGUFs `UD-Q4_K_XL`, `UD-Q5_K_M`, `UD-IQ3_XXS`, at temperature 1.0 vs 0.6.
- [ ] Record pass rate, tool-call validity, tokens/s, prompt-processing rate and peak RSS in `docs/reviews/review.md`.

Gate [LLM-BASELINE]: the model passes enough tasks to be worth porting, and the report fixes the default quant and sampling. If 3.8 fails, run 3.6; if both fail, stop.

## Phase 1 — Storage
- [ ] Write `docs/specs/applications/0053-LocalCodingModel.md`, defining every `LLM-*` ID used here.
- [ ] `float32`/`float16` buffers and native device arithmetic come from plan 0050 ([TYPE-NUMERIC-SIZED], [GPU-DEVICE-NATIVE]); wait on its Phase 5 gate; add nothing of its own here.
- [ ] [LLM-WEIGHTS-MAPPED]: a read-only buffer over a memory-mapped file region, shared with the GPU without a copy on unified memory.
- [ ] [LLM-WEIGHTS-QUANT]: block decoders for each tensor type the chosen GGUF uses (listed from the file in Phase 0); compare against `gguf-py` on every block of a generated fixture.
- [ ] [LLM-STATE-OWNED]: one mutable, single-owner state buffer for KV and recurrent state, relaxing [GPU-BUFFER] immutability for this type only; checks: no aliasing, compile error on capture by an escaping closure.
- [ ] [LLM-ARITH-IEEE]: an `Arith` policy with IEEE semantics in kernels and a finite check on each token's logits that faults the request instead of emitting garbage.

Gate: all three memory backends pass, ARC clean.

## Phase 2 — Speed gate
- [ ] [LLM-MATMUL-QUANT]: a product of a quantized matrix and a `float32` matrix.
- [ ] Port GPT-2's handlers to `float32` as the first end-to-end proof.
- [ ] Synthetic workload with the real tensor shapes and 26G operations per token, on random weights.

Gate [LLM-SPEED]: at least 5 tokens/s on this M4 Max, and at least 25% of the Phase 0 Ollama rate on the same quant (a 2,000-token turn ≈ 7 min). Go/no-go: if missed, stop and report before porting the model; next options are kernel fusion (0023-gpu.md) and a multithreaded NEON path — binding a native engine is not an option.

## Phase 3 — Model on a tiny checkpoint
- [ ] `@osprey/gguf`: reader from `gguf` 0.19.0, oracle in `scripts/oracles/gguf.py`.
- [ ] `packages/transformers/src/qwen3_5_*.py`: [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE].
- [ ] Tiny random `Qwen3_5TextConfig` (4 layers, a few MB) written by `scripts/oracles/transformers.py`; every layer's activations compared to Transformers in `float32` under [PORT-TEST-ORACLE].
- [ ] Permutations: one token at a time vs a batch, cached vs uncached, recurrent vs chunked rule, `float16` KV vs `float32`.

Gate [LLM-MODEL-ORACLE]: all activations within tolerance on host and GPU handlers; CI runs this and never downloads real weights.

## Phase 4 — Real weights
- [ ] Load the chosen GGUF, with no download at test time.
- [ ] Compare logits with llama.cpp on the same file over 50 prompts up to 32K tokens: same top-1 token every time, top-20 logits within a tolerance fixed from Phase 3.
- [ ] Measure `float16` KV drift at 64K against `float32`; if it changes top-1 tokens, use `float32` and reduce the default context to fit.

Gate [LLM-MEMORY]: peak RSS at or under 24 GB at full context, measured by the test (local gate).

## Phase 5 — Language layer
- [ ] [LLM-TOKENIZER]: NFC, the split pattern and ByteLevel in `@osprey/tokenizers`, reusing `@osprey/tiktoken`'s merge code; read vocabulary/merges from GGUF metadata if Phase 0 confirms they're there. Compare with the real package on source code in many languages, emoji, invalid-looking input.
- [ ] Render jinja2 macros, then the unmodified template against the real one across messages, tools, thinking, `reasoning_effort`, `preserve_thinking`.
- [ ] Sampling defaults from Phase 0 through `@osprey/transformers`, with deterministic seeds.

Gate: byte-identical prompts and token IDs against the oracle.

## Phase 6 — Tool calls
- [ ] [LLM-TOOL-PARSE]: a streaming parser for the template's `<tool_call><function=…><parameter=…>` format, coercing values by the tool's JSON Schema; separate `<think>` content from replies.
- [ ] [LLM-TOOL-CONSTRAIN]: once the model opens `<tool_call>`, mask tokens so only declared function names, declared parameter names, and enum/number/boolean-valid values can be sampled; free text stays free. Implement as a logits processor.
- [ ] [LLM-TOOL-REPAIR]: a call that still fails its schema goes back to the harness as a recoverable tool error, never as made-up arguments.

Gate: 100% of the 200 Phase 0 cases schema-valid; tool-selection accuracy not below the Ollama baseline.

## Phase 7 — Server and prompt cache
- [ ] [LLM-API-OLLAMA]: `/api/version`, `/api/tags`, `/api/show` (capabilities + context length), `/api/ps`, `/api/chat`, `/api/generate`, NDJSON streaming, `tool_calls` arguments as JSON objects.
- [ ] [LLM-API-OPENAI]: `/v1/models` and `/v1/chat/completions` with SSE.
- [ ] [LLM-API-QUEUE]: one generation at a time, FIFO, cancelled when the client disconnects.
- [ ] [LLM-API-LIMIT]: a prompt over `num_ctx` is an error naming both numbers; never truncate silently.
- [ ] [LLM-CACHE-CHECKPOINT]: the KV cache can be cut back but recurrent state can't, so keep rolling checkpoints of the state and reuse the longest matching prefix (else every agent turn reprocesses the whole prompt). Test with a harness that rewrites its history.
- [ ] `osprey-llm pull`: resumable download, SHA-256 checked against Hugging Face's metadata.

Gate: real `ollama run`, an OpenAI SDK client and `@osprey/pydantic-ai` each complete a tool-using conversation.

## Phase 8 — Evaluation and release
- [ ] Run the Phase 0 suite through **pi** and the `@osprey/pydantic-ai` coding agent, against both Osprey and the baseline.
- [ ] Multi-hour soak: no leak, stable RSS, clean ARC exit.
- [ ] README, spec status, and a dated report with every measurement.

Gate [LLM-ACCEPT]: on the same GGUF, task pass rate within 1 of the 30 against the Ollama baseline; no schema-invalid tool call reaches a harness; the 24 GB and speed gates still hold.

## Constraints
- Text-only path; port only the recurrent delta rule (not the chunked/triangular-solve form), batch every matrix product, run the state update as one sequential kernel.
- Gated DeltaNet always in `float32`.
- Attention: 24 query heads over 4 key/value heads, an output gate, rotary dimension 64 from `partial_rotary_factor` 0.25.
- Text rotary embedding must reduce to plain 1-D; the oracle confirms this before it's relied on.
- The model runs over a static Tensor effect with a host handler and a GPU handler ([PORT-DEVICE]).
- Serve on port 11435, loopback only (beside a real Ollama on 11434, which Phase 0 and the evaluation need).
- Binding llama.cpp through `extern` is ruled out: not a port under [PORT-SCOPE].
- Linux on OpenCL follows once Metal passes ([GPU-BACKEND-PORTABILITY] requires vendor neutrality).

## Testing and CI
- CI (Linux + macOS, existing Build → Test → Coverage phases): Phases 1, 3, 5, 6, 7 against the tiny checkpoint; fixtures stay within a few MB; CI never downloads real weights.
- Local only: real weights, the logit comparison, the memory gate and the evaluation, results in dated reports.
