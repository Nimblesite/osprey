# Local coding model [LLM]

**Status:** planned; not implemented. Nothing here exists yet: no GGUF reader, no `qwen3_5` model, no tokenizer, no tool calling, no server. [Plan 0049](../../plans/0049-local-coding-model.md) owns delivery; every rule below is a delivery target.

## Model and weights [LLM-WEIGHTS]

The model is [`Qwen/Qwen3.8-27B`](https://huggingface.co/Qwen/Qwen3.8-27B), revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`, with weights from [`unsloth/Qwen3.8-27B-GGUF`](https://huggingface.co/unsloth/Qwen3.8-27B-GGUF), revision `4ca720788d1e01f1bff70c033e0d0028fd02e502`.

The fallback is `Qwen/Qwen3.6-27B`, which has the same `qwen3_5` architecture and the identical parameter count (27,781,427,952): a weights swap that leaves the rest of this specification unchanged.

## Serving and baseline [LLM-SERVING]

Osprey serves the Ollama wire protocol itself. A real Ollama can't host this port: `ollama create` runs a GGUF on Ollama's own engines, so no Osprey code would execute. Any harness that speaks Ollama or OpenAI points at the Osprey server instead.

The port is 11435, so it runs beside a real Ollama on 11434, which the baseline and the evaluation need. It listens on loopback only.

Real Ollama stays useful as the baseline, loaded with the same GGUF so that any quality gap belongs to the port and not the weights. The `qwen3.8:27b` library tag doesn't state its quantization, so it isn't a fair baseline.

## Architecture [LLM-ARCHITECTURE]

Everything above the runtime is Python-flavor Osprey under [PORT-SCOPE](../ecosystem/0043-PythonFlavorPorts.md#what-a-port-is--port-scope). Binding llama.cpp through `extern` would not be a port, so it is ruled out.

Text-only path, 64 layers of `16 × (3 × (Gated DeltaNet → FFN) → 1 × (Gated Attention → FFN))`.

- **Attention:** 24 query heads over 4 key/value heads, an output gate, and rotary dimension 64 from `partial_rotary_factor` 0.25.
- **Text rotary embedding:** should reduce to plain 1-D because all three position axes are equal. The oracle has to confirm this before it's relied on.
- **Gated DeltaNet:** a depthwise causal convolution of kernel 4 over 10,240 channels, then the delta rule, always in `float32`, then a gated RMS norm.
- **One delta rule, not two:** Transformers has `torch_recurrent_gated_delta_rule` and a chunked form with a triangular solve, documented as computing the same thing. Port only the recurrent one, batch every matrix product, and run the state update as one sequential kernel. That removes the triangular solve. The oracle checks agreement with the chunked form.

## Memory budget [LLM-MEMORY-BUDGET]

Default: **`UD-Q4_K_XL` at 64K context**. Harness overhead alone can be 5–30K tokens, so 32K is too tight for agent work.

| Item | Size | Basis |
| --- | --- | --- |
| Weights | 17.6 GB | File size, mapped read-only |
| KV cache at 64K, `float16` | 4.3 GB | 16 attention layers × 4 heads × 256 × 2 = 32,768 values per token |
| Recurrent and convolution state | 0.16 GB | 48 layers × 48 × 128 × 128, plus 3 × 10,240 per layer, in `float32` |
| 4 prompt checkpoints | 0.63 GB | One copy of the state above each |
| Working memory | 0.5 GB | Estimate, measured when real weights run |
| **Total** | **23.2 GB** | Peak RSS gate at 24 GB |

More accuracy: `UD-Q5_K_M` (19.8 GB) at 32K also totals about 23 GB. 16 GB machines: `UD-IQ3_XXS` (10.9 GB) at 32K totals about 14.4 GB; treat this as degraded. Skipped: the vision projector (0.9 GB) and MTP (1.4 GB).

## Requirements [LLM-REQUIREMENTS]

- **[LLM-BASELINE]:** the model passes enough of the tasks to be worth porting, and the report fixes the default quant and sampling. If 3.8 fails here, run 3.6. If both fail, stop: no port can beat its own weights.
- **[LLM-WEIGHTS-MAPPED]:** a read-only buffer over a memory-mapped file region, shared with the GPU without a copy on unified memory.
- **[LLM-WEIGHTS-QUANT]:** block decoders for each tensor type the chosen GGUF actually uses, listed from the file. Compare against `gguf-py` on every block of a generated fixture.
- **[LLM-STATE-OWNED]:** one mutable, single-owner state buffer for KV and recurrent state. This relaxes [GPU-BUFFER](../runtime/0034-GPUComputation.md#buffers--gpu-buffer)'s immutability for that type only, so it needs its own checks: no aliasing, and a compile error on capture by an escaping closure.
- **[LLM-ARITH-IEEE]:** an `Arith` policy with IEEE semantics in kernels and a finite check on each token's logits, which faults the request instead of emitting garbage.
- **[LLM-MATMUL-QUANT]:** a product of a quantized matrix and a `float32` matrix.
- **[LLM-DISPATCH-BATCH]:** the Metal adapter encodes consecutive dispatches into one command buffer and submits it only where the host reads a result (a write-back, a download, or the batch cap), so a token's kernels are one submission. Bytes match the host and a failed batch is reported by its first kernel.
- **[LLM-SPEED]:** at least 5 tokens per second on the M4 Max, and at least 25% of the Ollama baseline rate on the same quant.
- **[LLM-MODEL-DELTANET] / [LLM-MODEL-ATTENTION] / [LLM-MODEL-ROPE] / [LLM-MODEL-CACHE]:** the `qwen3_5` text model split across `src/osprey/packages/transformers/src/qwen3_5_*.py`.
- **[LLM-MODEL-ORACLE]:** all activations within tolerance, on host and GPU handlers. This is what CI runs, and CI never downloads real weights.
- **[LLM-MEMORY]:** peak RSS at or under 24 GB at full context, measured by the test. A local gate, since hosted runners can't hold this model.
- **[LLM-TOKENIZER]:** NFC, the split pattern and ByteLevel in `@osprey/tokenizers`, using `@osprey/tiktoken`'s merge code so it isn't duplicated. Read the vocabulary and merges from GGUF metadata if the file is confirmed to hold them, so one file is enough.
- **[LLM-TOOL-PARSE]:** a streaming parser for the template's `<tool_call><function=…><parameter=…>` format, coercing values by the tool's JSON Schema. Separate `<think>` content from replies.
- **[LLM-TOOL-CONSTRAIN]:** once the model opens `<tool_call>`, mask tokens so that only declared function names, declared parameter names, and values valid for enum, number and boolean types can be sampled. Free text stays free. Implement as a logits processor.
- **[LLM-TOOL-REPAIR]:** a call that still fails its schema goes back to the harness as a tool error it can recover from, never as made-up arguments.
- **[LLM-API-OLLAMA]:** `/api/version`, `/api/tags`, `/api/show` (reporting capabilities and context length), `/api/ps`, `/api/chat`, `/api/generate`, with NDJSON streaming and `tool_calls` arguments as JSON objects.
- **[LLM-API-OPENAI]:** `/v1/models` and `/v1/chat/completions` with SSE.
- **[LLM-API-QUEUE]:** one generation at a time with a FIFO queue, cancelled when the client disconnects.
- **[LLM-API-LIMIT]:** a prompt over `num_ctx` is an error naming both numbers. Never truncate silently; a harness that loses its instructions gives wrong answers with no warning.
- **[LLM-CACHE-CHECKPOINT]:** the KV cache can be cut back but recurrent state can't, so keep rolling checkpoints of the state and reuse the longest matching prefix.
- **[LLM-ACCEPT]:** on the same GGUF, task pass rate within 1 of the 30 against the Ollama baseline; no schema-invalid tool call reaches a harness; the 24 GB and speed gates still hold.

## Scope [LLM-SCOPE]

Vision and video, MTP, training, producing our own quantizations, YaRN beyond 262K, CUDA, batching for several users, and Windows are out of scope. Linux on OpenCL follows once Metal passes, since [GPU-BACKEND-PORTABILITY](../runtime/0034-GPUComputation.md#vendor-independence--gpu-backend-portability) requires vendor neutrality.
