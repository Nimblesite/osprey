# Python-flavor GPT-2 port notes

The authored sources live in [`examples/gpt2/python`](../../examples/gpt2/python/README.md). They are self-contained Python-flavor modules: the `Tensor` static effect and its two static handlers, the arithmetic policy, the tokenizer, the checkpoint loader, the records, the forward pass, the KV-cached step and the sampler. The project imports no `.osp` module and its source root holds none. [Plan 0031](0031-python-flavor.md) tracks the complete flavor. Training is not being ported at the moment; the Osprey trainer in `examples/gpt2/train` stays Default-only.

| Reference Python | Osprey Python flavor |
| --- | --- |
| `class Tokenizer`, stdlib string handling | `tokenizer.py`: byte stand-ins, pre-tokenizer scan and BPE merges as folds and recursion over `byteAt`, `substring`, `codePointAt` and `Map` |
| `class Safetensors`, stdlib parsing/loading | `Tensor.load`, answered by each handler through the GPU surface's safetensors reader |
| Runtime dictionaries for activations/parameters | `Layer`, `Weights`, `Acts`, `Attn`, `Trace`, `BlockStep` and `Stepped` record classes |
| Tensor lists, comprehensions and indexed mutation | Explicit `Tensor.build`, `get`, `zip_with` and `mat_mul`; the handler selects representation/execution |
| `result or default`, `try`/`except` | `unwrap_or(result, fallback)`, the spelling of Default `result ?: fallback`; faults are a `Fault` effect that `strictly` turns into `Error` |
| Mutable trace and sampling loops | Immutable record accumulators, `fold` and tail recursion |
| `math.sqrt`, `math.exp`, `math.tanh` | Existing `sqrt`, `exp`, `tanh` |
| Dictionary keys | Named record fields, preserving checkpoint tensor ordering |
| Host token IDs used in embedding | `embed` performs host orchestration and passes scalar IDs to kernels |
| Slicing the final row and dotting each vocabulary row | Build the last-row tensor and multiply by the transposed tied embedding |
| Recomputing the whole sequence per token | The Default example's KV-cached `forward_step` over each block's cached qkv rows |
| `sys.argv`/`__main__` | Three-line stdin contract and Osprey `main` |

`Tensor.build` appears directly at each kernel construction site so static specialization receives the actual lambda. A handler class lowers to a named handler function, the module idiom `fn on_gpu(work) = { handle static Tensor<GpuBuffer<float>> { … }\n work() }`, because a module `let` must be a compile-time constant. `strictly` nests two `with` suites; each suite is a closure, so its Result type is pinned by the one return annotation in the port.

Native compiler validation compares every element of 24 tensors to the unchanged reference: three sequence lengths for LayerNorm and causal attention, small matrix/transpose/linear/residual/GELU cases, and every intermediate activation in one block. Both handlers pass under default, GC and ARC; ARC exits retain no live objects. The generator comparison runs both the Default and the Python projects with the same stdin and asserts identical stdout bytes on host, Metal and OpenCL, and over host lists; it needs the downloaded checkpoint, so it is an `--ignored` test, while the checkpoint-free message comparison and the no-`.osp` check run in every suite.
