# GPT-2 in Osprey

Run OpenAI's 124M-parameter GPT-2 in Osprey's **Python flavor**: familiar `def`, `lambda` and f-strings, compiled to native code, with one model running on CPU or GPU. Algebraic effects let the caller choose where the arithmetic runs; inferred types and compiler-checked handlers keep that choice out of the transformer.

**Keep Python in your workflow.** Osprey also supports [calling native Osprey functions from Python and Python libraries from Osprey](../../docs/specs/platforms/0042-PythonInterop.md). You can retain Python tooling while moving computation into a compiled language with GPU kernels. The Python flavor is Osprey source, marked `# osprey: flavor=python`; ordinary Python packages are reached through the explicit interop bridge. Osprey is actively evolving, and shared tensor storage across that bridge remains planned.

This example loads [Hugging Face's published weights](https://huggingface.co/openai-community/gpt2) directly from `model.safetensors`, rebuilds the byte-level BPE tokenizer from `merges.txt`, and implements the transformer, KV-cached generation and greedy sampling in Osprey. No retraining or checkpoint conversion is needed.

- **Start with Python flavor:** [`python/`](python/README.md) contains the model, tokenizer, sampler and both device handlers as `.py` modules. The [flavor guide](https://www.ospreylang.dev/docs/python-flavor/) covers the supported Python constructs and Osprey additions.
- **Explore Default flavor:** [`model/`](model), [`generate/`](generate) and [`train/`](train) include generation and a training step. Training is currently Default-only.
- **Compare with Python:** [`reference.py`](reference.py) is an independent implementation run by CPython. The Default and Python-flavor generators print identical bytes for the same prompt on each tested backend.

## CPU or GPU is an algebraic effect

The model never names a device or a buffer type. Every tensor operation it needs (build, read, combine, reduce, matrix product, load) is an operation of one static effect, `Tensor<B>`. The model *performs* those operations, and whichever handler the program installs around it decides what `B` is and where the arithmetic runs:

| Handler | Default | Python | Runs on | `B` |
| --- | --- | --- | --- | --- |
| `onLists` / `on_lists` | [`model/cpu.osp`](model/cpu.osp) | [`python/cpu.py`](python/cpu.py) | the CPU | `List<float>` |
| `onGpu` / `on_gpu` | [`model/gpu.osp`](model/gpu.osp) | [`python/gpu.py`](python/gpu.py) | Metal or OpenCL | `GpuBuffer<float>` |

`BACKEND=lists` or `BACKEND=gpu` picks the handler. The model source is identical in both runs, and on the CPU both handlers print the same tokens, the same loss and the same 148 gradient sums; on a GPU the text matches and numeric differences are bounded as described below.

The caller installs one handler around the computation; device placement never enters the transformer. A backend implements ten operations without changing the model. Missing handlers are compile errors, and the `static` effect is selected at compilation without runtime effect dispatch. The Python flavor gets the same checks through `@static_effect` and `@static_handler`.

## Run it

```
make gpt2 FLAVOR=python PROMPT="The capital of France is" TOKENS=20 BACKEND=gpu
make gpt2 FLAVOR=python PROMPT="The capital of France is" TOKENS=20 BACKEND=lists
make gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=gpu
make gpt2-train PROMPT="The capital of France is" BACKEND=gpu
make gpt2-train PROMPT="The capital of France is" BACKEND=lists
```

Run these commands from the repository root. The first run downloads about 550 MB into `examples/gpt2/weights/`, ignored by git. `BACKEND` defaults to `gpu`; `GPU=auto` selects the adapter. Generation prints the device, token IDs and continuation:

```
gpt2 (124M) on host — 50000 merges, 50257 vocabulary rows, 12 layers
prompt: The capital of France is
tokens: 464 3139 286 4881 318
generated 20 tokens
The capital of France is the capital of the French Republic, and the capital of the French Republic is the capital of the French
```

This banner uses `GPU=host`; a GPU run names its adapter, such as `metal:0 (Apple M4 Max)`, and `BACKEND=lists` names `lists`.

`make gpt2-train` computes next-token cross-entropy, gradients through all twelve blocks, one SGD update, then the loss again. It prints 148 gradient-tensor sums in checkpoint order, accumulating vector elements and matrix rows sequentially:

```
gpt2 (124M) on host — one training step over 5 tokens, learning rate 0.0001
prompt: The capital of France is
tokens: 464 3139 286 4881 318
loss 4.503189215447316
grad wte -6.369071936518367e-13 wpe -1.4510961876545991e-15 ln_f 0.20932690619905386 0.512850562773751
h.0 -3.3628631026293108 -7.1375868853910704 3.8554208861589614 …
…
loss after one step 3.8539085307403895
```

## Layout

| Path | What it is |
| --- | --- |
| [`python/`](python/README.md) | Python-flavor generation: effect, handlers, tokenizer, loader, model and sampler |
| `model/backends.osp` | `Tensor<B>` effect, arithmetic policy `strictly` and handler selector `under` |
| `model/cpu.osp`, `model/gpu.osp` | CPU-list and Metal/OpenCL-buffer handlers |
| `model/tokenizer.osp` | Byte-level BPE tokenizer from `merges.txt` |
| `model/model.osp` | Weights, transformer, saved activations and KV-cached step |
| `generate/`, `train/` | Default-flavor generation and training entry points |
| `reference.py` | Independent CPython oracle for forward and backward passes |

Each Default program is a [project](../../docs/specs/language/0025-ModulesAndNamespaces.md) with `model/` as a second source root. `osprey examples/gpt2/generate --run` runs the project; Makefile targets add `--memory=arc`.

## One model, two handlers

The model's records are generic in their tensor representation: `Layer<B>`, `Weights<B>`, `Acts<B>`. Here is LayerNorm in the Python flavor, from [`python/model.py`](python/model.py):

```python
# osprey: flavor=python
def layer_norm(x, t, g, b):
    means = row_means(x, t)
    variances = row_variances(x, t, means)
    return perform(Tensor.build(t * WIDTH, lambda i: normalised(x, means, variances, g, b, i)))
```

`perform` requests an operation; the installed handler supplies its implementation. The CPU handler builds lists with `range`, `map` and `toList`. The GPU handler builds buffers with `gpuIota` and `gpuMap`, forwarding the model's callbacks to kernels. Matrix products similarly use a list `fold` or `gpuMatMul`.

The Default entry point installs the selected handler around the whole run:

```osprey
Backends::under(input(), fn() => run(prompt, count))
```

Each branch is compiled with its handler specialised into the model. [One model, two backends](../../website/src/docs/one-model-two-backends.md) explains the effect and what the compiler proves.

## Working alongside Python

Python interoperability is separate from the Python flavor. With the repository's `osprey-interop` package installed, real Python can build and call a native Osprey export:

```python
import osprey

native = osprey.build("examples/python/interop.py")
print(native.greeting("Python"))  # hi Python
```

The [runnable interop example](../python/interop.py) also calls Python from Osprey. The bridge supports scalar calls, explicit exception results and opaque handles to Python objects, including library-owned tensors. This lets Python retain its libraries while Osprey supplies checked native functions. It does not expose GPT-2's generic tensor functions as Python exports or turn Python tensor storage into an Osprey GPU buffer; see the [interop contract](../../docs/specs/platforms/0042-PythonInterop.md).

## What runs where

`BACKEND=gpu` installs `onGpu`; `GPU=auto` selects Metal on macOS or an OpenCL GPU elsewhere. `GPU=metal` and `GPU=opencl` request an adapter explicitly; `GPU=host` runs those same kernels and matrix products on the CPU. `BACKEND=lists` uses host lists regardless of `GPU`. Both flavors follow this split: the list handler never touches the GPU surface, and the device handler executes every model kernel and product on the selected GPU, without CUDA. Adapter failures abort with an error.

```sh
make gpt2 GPU=host      # every kernel and product on the CPU
make gpt2 GPU=metal     # every kernel and product on the Apple GPU
make gpt2 BACKEND=lists # host lists, CPU only
```

To run the compiler interactively from the repository root:

```sh
OSPREY_GPU=auto osprey examples/gpt2/generate --run --memory=arc
```

Enter the prompt, token count and backend (`gpu` or `lists`) on three lines. Set `OSPREY_GPU_TRACE=1` for completed-dispatch reports.

On a selected device, embeddings, LayerNorm, attention, softmax, GELU, residuals, reductions, token selection, training loss and all matrix products execute there ([device contract](../../docs/specs/runtime/0034-GPUComputation.md#device-backends--gpu-backend-device)). Both adapters use software binary64 arithmetic with the host's accumulation order. Tokenization, buffer construction/loading and readbacks stay on the host. Buffers are [device-resident](../../docs/specs/runtime/0034-GPUComputation.md#device-resident-buffers--gpu-buffer-resident) from first launch: weights upload once and layer intermediates stay on device. Unsupported kernels abort rather than falling back to CPU; there is no kernel fusion yet.

Device `exp`, `ln` and `tanh` are correctly rounded where host libm can differ in the last digit. Generated text matches across tested paths; training loss is `4.503189215447316` on host and `4.503189215447317` on Metal. Across the step's 150 numbers, the largest host/device difference is 2.4 × 10⁻¹³. Near-zero gradient sums can differ in every digit. Metal and OpenCL print identical bytes apart from the device name.

| Measured 2026-10-13, AMD Ryzen 9 3900X, Linux 6.18 (WSL2), release build, `--memory=arc`, `GPU=host` | `BACKEND=gpu` | `BACKEND=lists` |
| --- | --- | --- |
| 20 tokens from the 5-token prompt: load, one full pass, 19 KV-cached steps | 19.6 s, 1.4 GB peak | 7 min 18 s, 1.7 GB peak |
| One training step over the 5-token prompt: load, forward, loss, backward, SGD, forward again | 52 s, 3.6 GB peak | 9 min, 4.6 GB peak |
| `reference.py`, the same 20 tokens in pure Python | 16 min 25 s | |

| Measured 2026-10-16, Apple M4 Max, macOS, release build, `--memory=arc`, `BACKEND=gpu`, median of three | `GPU=host` | `GPU=metal` | `GPU=opencl` |
| --- | --- | --- | --- |
| 20 tokens from the 5-token prompt, every kernel and product on the selected device, buffers resident on it | 6.5 s, 1.7 GB peak | 6.9 s, 3.1 GB peak | 6.8 s, 1.7 GB peak |
| One training step over the 5-token prompt | 25.2 s, 4.0 GB peak | 7.6 s, 5.9 GB peak | 7.8 s, 3.1 GB peak |
| `reference.py --train`, the same step in pure Python (2026-10-15) | 5 min 49 s, 16.2 GB peak | | |

| Measured 2026-10-17, Apple M4 Max, macOS, release build, `--memory=arc`; `BACKEND=gpu` median of three, `BACKEND=lists` one run | `BACKEND=lists`, `OSPREY_GPU=invalid` | `BACKEND=gpu`, `GPU=host` | `BACKEND=gpu`, `GPU=metal` |
| --- | --- | --- | --- |
| 20 tokens from the 5-token prompt, Default flavor | 2 min 23 s, 1.8 GB peak | 8.2 s, 1.6 GB peak | 6.1 s, 2.8 GB peak |
| The same 20 tokens, Python flavor | 2 min 19 s, 1.8 GB peak | | 7.3 s, 2.8 GB peak |
| One training step over the 5-token prompt | 3 min 19 s, 3.9 GB peak | | 6.7 s, 3.9 GB peak |

The list handler also ran with `OSPREY_GPU=invalid`: any GPU call would have aborted, yet both flavors matched Metal's text. Dense buffers and flat kernel functions explain much of the CPU improvement over boxed lists and closure calls.

Training benefits more from the GPU's parallel work: half its 1,499 dispatches cover at least 3,072 threads. Generation's KV-cached steps are smaller: 4,338 of 5,919 dispatches cover 768 threads or fewer, each requiring a separate launch. These measurements compare this implementation with its pure-Python oracle, rather than optimised Python frameworks.

## Checking the output

[`reference.py`](reference.py) uses pure Python without NumPy, reading the same checkpoint and matching the forward/backward passes' left-to-right accumulation. It adds sequentially from `0.0` because Python 3.12+ uses compensated float summation in `sum`:

```
python3 examples/gpt2/reference.py examples/gpt2/weights "The capital of France is" 20
python3 examples/gpt2/reference.py examples/gpt2/weights --train "The capital of France is"
```

Tokenizer checks include the default prompt, `Hello, I'm a language model,`, repeated spaces, punctuation and `naïve café — 日本`. Compared continuations match: 20 tokens for the default prompt and `not a` for the second. On the host, both handlers match Python's loss, all 148 gradient sums and the updated loss exactly; device comparisons use the precision limits above.

[`tensor_backends.test.osp`](../../tests/effects/staged/tensor_backends.test.osp) pins a width-4 model under both handlers against Python values; its ML twin must emit identical LLVM IR. [`transformer.test.osp`](../../tests/core/gpu/transformer.test.osp) pins the kernels. The differential harness covers memory backends, kernel lowerings and wasm32 on `make test`.

The [Python-flavor checks](python/README.md) additionally compare 24 deterministic tensors against Python at the real embedding width. `python_gpt2_generator_output_is_byte_identical_to_the_default_generator` compares the generators on each tested device; it requires downloaded weights and runs in the ignored-test suite.

## How it is written

- **Weights** come through the `load` operation: `tensor("h.3.ln_2.bias")` is a `B`. A failed load aborts with its error instead of substituting an empty tensor. Under `onGpu`, `load` uses `gpuLoadTensor`; under `onLists`, it uses `loadTensor`, which reads the same file with the same reader straight into a list, so the list handler never touches the GPU surface. Shapes are stated at each `matMul`, because tensors are rank-1.
- **Tokenizer** — GPT-2 encodes bytes, and merge *i* of `merges.txt` is token `256 + i`, so the merge list alone is the vocabulary; `vocab.json` is never read. Pre-tokenization approximates GPT-2's regular expression for ASCII text and treats every non-ASCII byte as a letter.
- **Attention, LayerNorm, GELU and softmax** are `build` operations over flat index arithmetic, with `exp`, `sqrt` and `tanh` called inside the index function and `fold` over `range` for the inner reductions.
- **Products** are `matMul`. Both handlers accumulate each element in k order from 0.0 — the reference order a device backend must match.
- **The KV cache** — the prompt takes one full forward pass; every later token runs `forwardStep`, one position through the blocks over each block's cached qkv rows, so a token costs one position rather than the whole sequence.
- **Sampling** is greedy: a `reduce` for the peak logit and an `indexOf` for its first index, both kernels.
- **The backward pass** is the same kinds of operation. A `linear`'s gradients are two products against transposed operands plus a column sum; LayerNorm's recomputes the normalised rows from its input; attention's is the softmax Jacobian per row and then the query, key and value columns of each head. The forward pass keeps each block's activations in an `Acts<B>` record so the backward pass reads each exactly once.

## Known limits

- Three stdin lines for `generate` (prompt, token count, backend) and two for `train` (prompt, backend); an unknown backend name prints an error and runs nothing.
- The pre-tokenizer is an ASCII approximation of GPT-2's Unicode regular expression; prompts in other scripts tokenize correctly only where the letter/number/punctuation classes agree with the ASCII rules.
- Computation is double precision throughout (the fp32 weights widen exactly), so logits differ from an fp32 implementation in the last digits; the greedy choice has agreed on every token compared so far.
- Training is one plain SGD step at a fixed learning rate: no optimiser state, no batching, no weight decay, and the updated weights are not written back.
