# Plan 0049 — A local coding model on Osprey: Qwen3.8-27B

Specs: [0034 GPU computation](../specs/runtime/0034-GPUComputation.md), [0043 Python-flavor ports](../specs/ecosystem/0043-PythonFlavorPorts.md), [0014 HTTP](../specs/runtime/0014-HTTP.md). New: `docs/specs/applications/0053-LocalCodingModel.md`, written in phase 1, owns the `LLM-*` IDs below.

## Goal

An Osprey program, with no CPython, llama.cpp or Ollama engine behind it, loads Qwen3.8-27B from one file in at most 24 GB of RAM and does real coding tasks through tool calls in an unmodified harness.

Accuracy and tool use come first. Speed only has to be good enough for an agent to finish a task.

## The model

**Pick: [`Qwen/Qwen3.8-27B`](https://huggingface.co/Qwen/Qwen3.8-27B)**, revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`, with weights from [`unsloth/Qwen3.8-27B-GGUF`](https://huggingface.co/unsloth/Qwen3.8-27B-GGUF), revision `4ca720788d1e01f1bff70c033e0d0028fd02e502`.

| Requirement | Finding |
| --- | --- |
| Free weights | Apache-2.0, not gated, downloaded 7.9M times |
| Source available | The architecture is Python in Transformers, [`models/qwen3_5`](https://github.com/huggingface/transformers/tree/main/src/transformers/models/qwen3_5): 702 lines of modular source, 1,969 generated. This is inference code. Qwen publishes no training code or data, and the port doesn't need them |
| Fits 16–24 GB | Yes at 4-bit, see the budget below |
| Tool use | Native tool format in the chat template; Ollama's own build advertises `tools` and `thinking` |
| Careful over fast | Thinking is on by default, with effort up to `xhigh`, the default in Unsloth's guide. Independent testing found it uses about 3× the tokens of 3.6 and is slower, which suits this goal |

From the model card, on the same scale as its predecessor:

| Benchmark | Qwen3.8-27B | Qwen3.6-27B | Opus 4.6 Max |
| --- | --- | --- | --- |
| SWE-bench Pro | 61.7 | 53.5 | 53.4 |
| Terminal-Bench 2.1 | 73.0 | 63.4 | 78.2 |
| DeepSWE 1.1 | 42.2 | 13.3 | — |
| NL2Repo-Bench | 42.3 | 36.2 | 47.6 |

**Every one of these numbers is vendor-reported.** Field reports are good but anecdotal, and they disagree: one session had no failed tool calls, while another saw about 1 in 10 calls with slightly malformed arguments. Phase 0 measures this on our own tasks before any port work starts, and phase 6 answers the malformed-call report directly.

Rejected:

| Model | Why not |
| --- | --- |
| Qwen3.8-Flash-Next | 180B parameters and a non-standard licence |
| GLM-5.3-Flash | 321B parameters |
| Qwen3-Coder-30B-A3B | July 2025; superseded |
| Devstral Small 2 (24B) | Older, and Mistral's 68% is on SWE-bench Verified, so it can't be compared with the card above |
| Gemma 4 31B | Aggregators contradict each other on its coding rank, and it has no agentic numbers to compare |

Fallback: `Qwen/Qwen3.6-27B` has the same `qwen3_5` architecture and the identical parameter count (27,781,427,952). If 3.8 disappoints in phase 0, it is a weights swap and the rest of this plan still holds.

## How it reaches harnesses

**Osprey serves the Ollama wire protocol itself.** A real Ollama can't host this port: `ollama create` runs a GGUF on Ollama's own engines, so no Osprey code would execute. Any harness that speaks Ollama or OpenAI points at the Osprey server instead:

```sh
osprey-llm serve --weights Qwen3.8-27B-UD-Q4_K_XL.gguf --port 11435
OLLAMA_HOST=127.0.0.1:11435 ollama run qwen3.8-osprey
```

Port 11435 lets it run beside a real Ollama on 11434, which phase 0 and the evaluation need. It listens on loopback only.

Real Ollama stays useful as the baseline, loaded with the same GGUF so that any quality gap belongs to the port and not the weights. The `qwen3.8:27b` library tag doesn't state its quantization, so it isn't a fair baseline.

## What stands in the way

Audited against this repository on 2026-11-03.

| Need | Today | Gap |
| --- | --- | --- |
| Hold the weights | `GpuBuffer<float>` stores binary64 | About 220 GB for 27.4B text parameters. **Blocking** |
| Compute at speed | Metal and OpenCL kernels run software binary64 | About a minute per token, see below. **Blocking** |
| KV cache and recurrent state | A buffer is immutable at the language surface | Copying 4 GB of KV per token is impossible. **Blocking** |
| Read quantized weights | `gpuLoad` and `gpuLoadTensor` copy and widen | Needs a mapped, block-quantized, read-only weight type |
| Read GGUF | Nothing | Port the `gguf` package |
| Model | GPT-2 only. `@osprey/transformers` holds the generation layer and defers architectures | Port the `qwen3_5` text model |
| Tokenizer | `@osprey/tiktoken` has byte-level BPE; `@osprey/tokenizers` has no BPE or NFC | NFC, the `\p{L}` split pattern and 248,044 entries with fast startup |
| Chat template | `@osprey/jinja2` parses macros, but rendering one is an error | The template's `render_content` macro must render |
| Sampling | `@osprey/transformers` has top-k and top-p | Reuse as is; add presence penalty only if the evaluation asks for it |
| Serving | `HttpResponse` has `streamFd`, and the HTTP plan is mid-repair ([0048](0048-runtime-memory-safety.md)) | Prove incremental streaming and disconnect detection |
| Agent to test with | `@osprey/pydantic-ai` has an `OllamaProvider` and a coding agent | None; it is one of the evaluation harnesses |

**The speed gap (my estimate).** One token costs about 26 billion multiply-adds at 32K context: 11.4G in the feed-forward layers, 5.6G in Gated DeltaNet, 7.6G in attention and 1.3G in the output head. That is roughly 200× GPT-2's 125M. GPT-2 takes about 0.3 s per token today, which gives about a minute per token. Native `float32` on the GPU is therefore the first real work, and phase 2 can stop the project.

## Memory budget at 24 GB

Default: **`UD-Q4_K_XL` at 64K context**. Harness overhead alone can be 5–30K tokens, so 32K is too tight for agent work.

| Item | Size | Basis |
| --- | --- | --- |
| Weights | 17.6 GB | File size, mapped read-only |
| KV cache at 64K, `float16` | 4.3 GB | 16 attention layers × 4 heads × 256 × 2 = 32,768 values per token |
| Recurrent and convolution state | 0.16 GB | 48 layers × 48 × 128 × 128, plus 3 × 10,240 per layer, in `float32` |
| 4 prompt checkpoints | 0.63 GB | One copy of the state above each |
| Working memory | 0.5 GB | Estimate, measured in phase 4 |
| **Total** | **23.2 GB** | Peak RSS gate at 24 GB |

Other tiers, each only if phase 0 passes it:

- **More accuracy:** `UD-Q5_K_M` (19.8 GB) at 32K also totals about 23 GB.
- **16 GB machines:** `UD-IQ3_XXS` (10.9 GB) at 32K totals about 14.4 GB. Treat this as degraded: one community config reports weaker tool-call discipline at low bit depths.

Skipped: the vision projector (0.9 GB) and MTP (1.4 GB). MTP trades memory for speed, the opposite of this goal.

## Architecture

```text
harness ── Ollama /api/chat, OpenAI /v1/chat/completions
  apps/llm                   server, CLI, tool-call parser, request queue, prompt cache
  packages/jinja2            renders the template from the GGUF
  packages/tokenizers        BPE, NFC, ByteLevel, special tokens
  packages/transformers      Qwen3_5ForCausalLM, sampling, tool-call constraints
  packages/gguf              port of gguf-py's reader
  static Tensor effect       host handler and GPU handler, [PORT-DEVICE]
  compiler/runtime           float32 and float16 buffers, in-place state, quantized products
```

Everything above the runtime is Python-flavor Osprey under [PORT-SCOPE]. Binding llama.cpp through `extern` would not be a port, so this plan rules it out.

Text-only path, 64 layers of `16 × (3 × (Gated DeltaNet → FFN) → 1 × (Gated Attention → FFN))`:

- **Attention:** 24 query heads over 4 key/value heads, an output gate, and rotary dimension 64 from `partial_rotary_factor` 0.25.
- **Text rotary embedding:** should reduce to plain 1-D because all three position axes are equal. The oracle has to confirm this before it's relied on.
- **Gated DeltaNet:** a depthwise causal convolution of kernel 4 over 10,240 channels, then the delta rule, always in `float32`, then a gated RMS norm.
- **One delta rule, not two:** Transformers has `torch_recurrent_gated_delta_rule` and a chunked form with a triangular solve, documented as computing the same thing. Port only the recurrent one, batch every matrix product, and run the state update as one sequential kernel. That removes the triangular solve. The oracle checks agreement with the chunked form.

## Phases

Each phase ends with its gate met, its tests in the suite, its spec IDs written, and a commit. Work stops at a failed gate and reports the measurement.

### Phase 0 — Baseline, before any port code

- [ ] Free disk. There is **17 GB free** at 99% full and two GGUFs won't fit. Needs 60 GB; `target` (10 GB) and `hosts/gpui/target` (14 GB) are shared with other agents, so coordinate through Too Many Cooks first.
- [ ] `apps/llm/eval/`: 30 coding tasks with deterministic pass/fail scripts, drawn from this repo's closed bugs and small features, plus 200 tool-call cases covering nested objects, multi-line strings, enums and error recovery over 15+ turns.
- [ ] Run them on real Ollama with the unmodified GGUFs: `UD-Q4_K_XL`, `UD-Q5_K_M` and `UD-IQ3_XXS`, and at temperature 1.0 against 0.6.
- [ ] Record pass rate, tool-call validity, tokens per second, prompt processing rate and peak RSS in `docs/reports/`.

**Gate [LLM-BASELINE]:** the model passes enough of the tasks to be worth porting, and the report fixes the default quant and sampling. If 3.8 fails here, run 3.6. If both fail, stop: no port can beat its own weights.

### Phase 1 — Storage

- [ ] Write spec 0053.
- [ ] `[LLM-BUFFER-FLOAT32]` `[LLM-BUFFER-FLOAT16]`: `GpuBuffer` element types with native arithmetic on Metal and OpenCL and on the host. Kernel arithmetic stays binary32 end to end, with no silent widening.
- [ ] `[LLM-WEIGHTS-MAPPED]`: a read-only buffer over a memory-mapped file region, shared with the GPU without a copy on unified memory.
- [ ] `[LLM-WEIGHTS-QUANT]`: block decoders for each tensor type the chosen GGUF actually uses, listed from the file in phase 0. Compare against `gguf-py` on every block of a generated fixture.
- [ ] `[LLM-STATE-OWNED]`: one mutable, single-owner state buffer for KV and recurrent state. This relaxes [GPU-BUFFER]'s immutability for that type only, so it needs its own checks: no aliasing, and a compile error on capture by an escaping closure.
- [ ] `[LLM-ARITH-IEEE]`: an `Arith` policy with IEEE semantics in kernels and a finite check on each token's logits, which faults the request instead of emitting garbage.

**Gate:** all three memory backends pass, with ARC clean.

### Phase 2 — Speed gate

- [ ] `[LLM-MATMUL-QUANT]`: a product of a quantized matrix and a `float32` matrix.
- [ ] Port GPT-2's handlers to `float32` as the first end-to-end proof.
- [ ] A synthetic workload with the real tensor shapes and 26G operations per token, on random weights.

**Gate [LLM-SPEED]:** at least 5 tokens per second on this M4 Max, and at least 25% of the phase 0 Ollama rate on the same quant. The floor means a 2,000-token turn takes about 7 minutes.

**This is the go/no-go.** If it misses, stop and report before porting the model. The next options are kernel fusion, which [plan 0023](0023-gpu-computation.md) still owes, and a multithreaded NEON path. Binding a native engine is not an option.

### Phase 3 — Model on a tiny checkpoint

- [ ] `@osprey/gguf`: the reader from `gguf` 0.19.0, with its oracle in `scripts/oracles/gguf.py`.
- [ ] `packages/transformers/src/qwen3_5_*.py`: `[LLM-MODEL-DELTANET]`, `[LLM-MODEL-ATTENTION]`, `[LLM-MODEL-ROPE]`, `[LLM-MODEL-CACHE]`.
- [ ] A tiny random `Qwen3_5TextConfig` (4 layers, a few MB), written by `scripts/oracles/transformers.py`, with every layer's activations compared to Transformers in `float32` under [PORT-TEST-ORACLE].
- [ ] Permutations: one token at a time against a batch, cached against uncached, recurrent against chunked rule, `float16` KV against `float32`.

**Gate [LLM-MODEL-ORACLE]:** all activations within tolerance, on host and GPU handlers. This is what CI runs, and CI never downloads real weights.

### Phase 4 — Real weights

- [ ] Load the chosen GGUF, with no download at test time.
- [ ] Compare logits with llama.cpp on the same file across 50 prompts of up to 32K tokens: the same top-1 token every time, and top-20 logits within a tolerance fixed from the phase 3 results.
- [ ] Measure `float16` KV drift at 64K against `float32`. If it changes top-1 tokens, use `float32` and reduce the default context to fit.

**Gate [LLM-MEMORY]:** peak RSS at or under 24 GB at full context, measured by the test. A local gate, since hosted runners can't hold this model.

### Phase 5 — Language layer

- [ ] `[LLM-TOKENIZER]`: NFC, the split pattern and ByteLevel in `@osprey/tokenizers`, using `@osprey/tiktoken`'s merge code so it isn't duplicated. Read the vocabulary and merges from GGUF metadata if phase 0 confirms they're there, so one file is enough. Compare with the real package on source code in many languages, emoji, and invalid-looking input.
- [ ] Render jinja2 macros, then the unmodified template against the real one across messages, tools, thinking, `reasoning_effort` and `preserve_thinking`.
- [ ] Sampling defaults from phase 0 through `@osprey/transformers`, with deterministic seeds.

**Gate:** byte-identical prompts and token IDs against the oracle.

### Phase 6 — Tool calls

- [ ] `[LLM-TOOL-PARSE]`: a streaming parser for the template's `<tool_call><function=…><parameter=…>` format, coercing values by the tool's JSON Schema. Separate `<think>` content from replies.
- [ ] `[LLM-TOOL-CONSTRAIN]`: once the model opens `<tool_call>`, mask tokens so that only declared function names, declared parameter names, and values valid for enum, number and boolean types can be sampled. Free text stays free. Implement as a logits processor.
- [ ] `[LLM-TOOL-REPAIR]`: a call that still fails its schema goes back to the harness as a tool error it can recover from, never as made-up arguments.

**Gate:** 100% of the 200 phase 0 cases are schema-valid, and tool-selection accuracy is not below the Ollama baseline.

### Phase 7 — Server and prompt cache

- [ ] `[LLM-API-OLLAMA]`: `/api/version`, `/api/tags`, `/api/show` (reporting capabilities and context length), `/api/ps`, `/api/chat`, `/api/generate`, with NDJSON streaming and `tool_calls` arguments as JSON objects.
- [ ] `[LLM-API-OPENAI]`: `/v1/models` and `/v1/chat/completions` with SSE.
- [ ] `[LLM-API-QUEUE]`: one generation at a time with a FIFO queue, cancelled when the client disconnects.
- [ ] `[LLM-API-LIMIT]`: a prompt over `num_ctx` is an error naming both numbers. Never truncate silently; a harness that loses its instructions gives wrong answers with no warning.
- [ ] `[LLM-CACHE-CHECKPOINT]`: the KV cache can be cut back but recurrent state can't, so keep rolling checkpoints of the state and reuse the longest matching prefix. Without this, every agent turn reprocesses the whole prompt, which is a reported failure of this architecture on llama.cpp. Test with a harness that rewrites its history.
- [ ] `osprey-llm pull`: resumable download with the SHA-256 checked against Hugging Face's metadata.

**Gate:** real `ollama run`, an OpenAI SDK client and `@osprey/pydantic-ai` all complete a tool-using conversation.

### Phase 8 — Evaluation and release

- [ ] Run the phase 0 suite through **pi** and the `@osprey/pydantic-ai` coding agent, against both Osprey and the baseline.
- [ ] Multi-hour soak: no leak, stable RSS, a clean ARC exit.
- [ ] README, spec status and a dated report with every measurement.

**Gate [LLM-ACCEPT]:** on the same GGUF, task pass rate within 1 of the 30 against the Ollama baseline; no schema-invalid tool call reaches a harness; the 24 GB and speed gates still hold.

## Testing and CI

- **In CI:** phases 1, 3, 5, 6 and 7 against the tiny checkpoint, on Linux and macOS, under the existing Build → Test → Coverage phases. Fixtures stay within a few MB.
- **Local only:** real weights, the logit comparison, the memory gate and the evaluation, with results recorded in dated reports. These never run on hosted runners, so they add no Actions spend beyond the small suites.
- **Costs:** all of it runs on this machine, so the only paid use is agents. Repetitive oracle cases and fixtures go to pi agents under review.

## Not in this plan

Vision and video, MTP, training, producing our own quantizations, YaRN beyond 262K, CUDA, batching for several users, and Windows. Linux on OpenCL follows once Metal passes, since [GPU-BACKEND-PORTABILITY] requires vendor neutrality.

## Risks

| Risk | Answer |
| --- | --- |
| Osprey can't reach usable speed | Phase 2 finds out before the model is ported |
| The model isn't good enough locally | Phase 0 finds out before anything is built |
| Quantization hurts tool calls | Phase 0 picks the quant from measurements; phase 6 constrains the output |
| `LLM-STATE-OWNED` undermines immutability elsewhere | One separate type, with compile-time checks, and it can move to [plan 0045](0045-compile-time-ownership.md) if ownership ships |
| Numeric drift between Osprey and the references | Layer-by-layer oracle comparison; a finite check on every token's logits |
| Plan 0048 is still changing the HTTP runtime | Phase 7 starts after its items land |
| A better open model ships during the work | Any `qwen3_5` release is a weights swap. Other architectures reuse phases 1, 2, 5, 6 and 7 |

## Sources

- Model: [Qwen3.8-27B card](https://huggingface.co/Qwen/Qwen3.8-27B), [GGUF files](https://huggingface.co/unsloth/Qwen3.8-27B-GGUF), [Unsloth guide](https://unsloth.ai/docs/models/qwen3.8-27b), [Ollama library](https://ollama.com/library/qwen3.8), [Ollama chat API](https://docs.ollama.com/api/chat).
- Memory and caveats: [RunPod review](https://www.runpod.io/blog/qwen3-8-27b-on-runpod-the-theory-behind-frontier-class-agentic-coding-that-fits-on-a-single-24gb-worker).
- Field reports, anecdotal: [MindStudio test](https://www.mindstudio.ai/blog/qwen3-8-27b-agentic-coding-test), [personal agent write-up](https://medium.com/@axdliu/i-run-qwen3-8-27b-as-my-local-personal-agent-and-it-feels-amazing-ddadf3179c9e).
- Hybrid cache behaviour: [llama.cpp prompt reprocessing write-up](https://zenn.dev/syoyo/articles/188508eb6ab389), [checkpoint log analysis](https://gist.github.com/huytd/3a1dd7a6a76fac3b19503f57b76dbe65).
- Rejected models: [Morph ranking](https://www.morphllm.com/best-open-source-llm), [Zima guide](https://shop.zimaspace.com/blogs/tech-ai-hub/best-local-coding-models-8gb-12gb-16gb-24gb-vram), [BenchLM](https://benchlm.ai/compare/gemma-4-31b-vs-qwen3-6-35b-a3b), [llmbase](https://llmbase.ai/compare/gemma-4-31b,qwen3-6-27b/).
