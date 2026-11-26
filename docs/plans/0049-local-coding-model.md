# Plan 0049: A local coding model on Osprey, and the agent that uses it

Goal: Qwen3.8-27B runs from the pinned GGUF through Osprey at the spec's speed, prompt-rate and memory gates, is served on port 11435 behind the Ollama and OpenAI routes, and `src/osprey/examples/agent` completes real coding and MCP tasks on it. Contract: [LocalCodingModel](../specs/applications/LocalCodingModel.md) (every rule, design and gate below), [OspreyAgent](../specs/applications/OspreyAgent.md), [PythonFlavorPorts](../specs/ecosystem/0043-PythonFlavorPorts.md), [GPUComputation](../specs/runtime/0034-GPUComputation.md) and [HTTP](../specs/runtime/0014-HTTP.md). Depends on [plan 0050](0050-sized-numeric-types.md) (its Phase 5 gate before storage and speed work), [plan 0055](0055-targets.md) (kernel fusion, a speed lever) and [plan 0045](0045-compile-time-ownership.md#runtime-hardening) (the `GpuState` fix is a compiler change under it).

Each item closes with a named test or a dated receipt in `target/local-coding-model/runs/`. `OSPREY_GPU` defaults to the host, so a device claim needs `OSPREY_GPU=metal` on the command and a receipt showing completed dispatches. CI runs the tiny checkpoint only (`test-qwen-gc`, `test-qwen-arc`) and never downloads real weights; real weights, the comparisons and the evaluation run locally.

## Baseline `[LLM-BASELINE]`

- [ ] Free 60 GB for `UD-Q5_K_M` and `UD-IQ3_XXS` beside the present `UD-Q4_K_XL`, cleaning inactive caches without deleting live build outputs and rechecking free space before each download.
- [ ] Run `src/osprey/examples/llms/qwen/eval/` (30 coding tasks with pass/fail scripts, 200 tool-call cases) on real Ollama with each of the three GGUFs at temperatures 0.6 and 1.0, through `eval/baseline.py` in one-hour `timeout` chunks that resume from the journal. Today only the Q4 temperature 1.0 journal exists. The Ollama baseline and the real-weights lane never share the 36 GB machine.
- [ ] Pass rate, tool-call validity, generated and prompt tokens/s and peak RSS for all six configurations, recorded in the dated report, fixing the default quant and sampling. If 3.8 fails here, run 3.6; if both fail, stop.

## Storage and arithmetic

- [ ] [LLM-STATE-OWNED] `appended` in `src/osprey/examples/llms/qwen/model/src/attention.py` copies every cached key and value row per token (12.0 tokens/s at positions 12–132, 7.4 at 132–516). Attribute the slope with `OSPREY_GPU_TIMING=2` at 512 positions (copy kernels, score kernel or allocation), then build the `GpuState` design or the fixed-capacity-block stopgap the spec describes. Check: token time measured at 1K and 32K cached positions before and after is flat, [LLM-SPEED] holds at 32K, and `failscompilation` cases reject capture by an escaping closure.
- [ ] [LLM-ARITH-IEEE] The kernel-side `Arith` policy with IEEE semantics; the finite-logits check is done (`model_decode_nan_logits`). Check: a device kernel reaches the handler, as [ARITH-DEVICE-RECOVERY] in [plan 0055](0055-targets.md) requires.

## Speed `[LLM-SPEED]` `[LLM-PROMPT-RATE]`

- [ ] Re-measure [LLM-SPEED] through the package on a quiet machine (load stated) against Ollama on the same quant, with an `OSPREY_GPU_TIMING=2` profile. Every figure so far was taken under load 4.4 or more.
- [ ] Close the `gpuMatMul` gap against the standalone Metal kernel at 64 rows (36–49 against 64–97 Gop/s) on a quiet machine. The prefill-shape product reached 1.1 TFLOP/s with 8-row by 4-column register tiles; the host order contract caps that design near 2 TFLOP/s, and a simdgroup-matrix kernel beyond it would end bit-exactness with the host oracle, a decision to record here.
- [ ] A synthetic workload with the real tensor shapes and 26G operations per token on random weights in `src/osprey/examples/llms/qwen/speed/`: today only the products are probed (`token_builtin.osp`). Attention, DeltaNet and norms join so the gate is measured without the 17.6 GB file.
- [ ] [LLM-PROMPT-RATE] Cold 1K, 8K and 32K prompts at or above Ollama's rate on the same quant and prompts (Ollama: 169, 154 and 146 tok/s cold; the package: 44, 44 and 25 tok/s of device time under load 3–7; the 8K and 32K rates are unmeasured). Next: name the kernels in the 63-dispatch batch and the host work between batches that both grow with context position, then remove the per-chunk O(context) work and the waits between batches.
- [ ] Peak RSS at or under 24 GB at full context, measured by a local test [LLM-MEMORY].
- [ ] A long-context run past the fixture's 8 positions: rotary angles and attention output at 1K and 32K against llama.cpp, with token time at each.
- [ ] Logits against llama.cpp on the same file over 50 prompts up to 32K tokens: the same top-1 token every time, the top-20 within a tolerance fixed from the model oracle.
- [ ] `float16` KV drift at 64K against `float32`; if it changes top-1 tokens, keep `float32` and cut the default context to fit. Check: a `float16` KV run on the tiny checkpoint matches the oracle within tolerance under host GC, host ARC and Metal ARC.

## Model checks and CI `[LLM-MODEL-ORACLE]`

- [ ] A required CI lane that runs the tiny checkpoint with `OSPREY_GPU=metal` and fails if no dispatch completes. It needs a self-hosted or paid macOS GPU runner, an owner's spend decision; the local lane is `make -C src/toolchain/targets/macos/tests qwen-headless`.
- [ ] The remaining permutation of the tiny-checkpoint checks in `model/src/*.py` ([LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE]) is `float16` KV, measured by the 64K drift item above.

## Platform kernels `[LLM-KERNELS-STATIC]`

- [ ] One static effect over the products and attention with three compile-time handlers: reference (today's kernels, no speed gate), mac (MLX through mlx-c, MPSGraph as the fallback, at least as fast as Ollama and llama.cpp on the M4 Max) and cuda (cuBLASLt and cuDNN on the Windows machine, at least as fast as llama.cpp and Ollama on that card). Each fast build stays within the oracle tolerance of the reference, every 1K/4K/10K/32K receipt names its build beside the baseline on the same machine, and CI builds all three.
  - [ ] Step 0, before any mac handler code: under the GPU-METAL lock, time `mlx-lm` with an MLX 4-bit Qwen3.8-27B cold against `llama-bench` and Ollama `qwen38-q4` on the 1K/4K/10K prompts. Settle first that MLX's affine per-group quantization is not GGUF's Q4_K and Q6_K blocks (convert exactly, or requantize and give up "same quant as the baseline") and that its fused attention reads only fp16, bf16 and fp32, so the KV cache stays unquantized. If MLX is slower than llama.cpp at any length, that operation moves to MPSGraph or adapted kernels.
  - [ ] Order: the effect and reference handler with no behaviour change; mac products; mac attention; re-measure under the GPU-METAL lock; the cuda handler with a receipt from the Windows machine (no NVIDIA library multiplies GGUF blocks directly: dequantize each tile to fp16 before cuBLASLt, or use llama.cpp's int8 tensor-core MMQ as the measured fallback).

## Sampling

- [ ] Sampling defaults from the baseline through `@osprey/transformers`, with deterministic seeds. Check: byte-identical prompts and token IDs against the oracle.

## Server `[LLM-API-*]` `[LLM-TEMPLATE]`

- [ ] [LLM-API-OLLAMA] The remaining template choices, stated in [LLM-TEMPLATE]: a history turn's `<think>` block and the generation prompt's `<think>` trailer. Ollama still rejects string tool-call arguments. Check: the integrated `real` lane on the weights passes with those cases.
- [ ] [LLM-API-QUEUE] Verify disconnect cancellation on the real weights; it is proven against the table (`a_client_that_disconnects_does_not_wedge_the_server`).

## Runtime `[FAILURE-EXPLICIT]`

- [ ] A stack overflow in a request handler, or in a per-character recursion on the runner's 8 MB main-thread stack, kills the process with SIGBUS or SIGSEGV and no message (the known recursions in `TableModel.tokenize`, the JSON writer, `byte_char_map` and `norm_of` are folds now). The runtime owes a guard-page handler that turns the overflow into a typed error for that request, or a documented stack size with a compile-time depth warning. Check: a test drives an overflowing handler or program and gets the error.

## Evaluation and release `[LLM-ACCEPT]`

- [ ] Run `eval/` through pi and the `@osprey/pydantic-ai` coding agent against Osprey and the baseline: `make -C src/toolchain/targets/llms qwen-pydantic` (local only, no CI registration) and the Agent-window lane. pi completed a tool loop against a local HTTP fixture; the real-weight conversations and the full corpus runs remain open, against both servers.
- [ ] (verify) The 2026-11-08 local lane receipts under `target/logs/plan0049/qwen-ui/` (26 preflight, 74 live and 33 replay UI steps; the six Pydantic AI cases on real Qwen) are no longer on disk, and the direct Qwen judge timed out at 300 s on the 3,776-token prompt. Re-run both lanes on real Qwen and put every number in the dated report.
- [ ] Multi-hour soak: no leak, stable RSS, clean ARC exit.
- [ ] README, the spec's status line and a dated report with every measurement: baseline, speed, prompt rate, memory, and the 30-task pass rate within 1 of Ollama's with no schema-invalid tool call reaching a harness.

## Agent `[AGENT-PROOF]`

The agent app's providers, settings, keys, MCP, self-tests and goldens are done (`src/osprey/examples/agent/self-test/{settings,mcp,code-blocks}.json`, `test-app/main.ospml`, `test/main.py`). One item remains:

- [ ] `src/osprey/examples/agent/self-test.sh --live [dotenv] [provider/model]`: on a named model with credit, run a real coding task (`self-test/coding.json` and `tasks/`) and an MCP task, each with acceptance checks the model never sees. The script is ready and waits on a key in `.env`.
