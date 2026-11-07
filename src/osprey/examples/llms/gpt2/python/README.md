# GPT-2 in the Python flavor

The whole program is Osprey source in the Python flavor: no `.osp` module is imported and the project's one source root holds none. These `.py` files start with `# osprey: flavor=python` and are compiled by `osprey`; they are not Python, and CPython cannot run them. The real Python in this example is [reference.py](../reference.py), the independent oracle. [backends.py](backends.py) declares the `Tensor` static effect and spells the arithmetic policy with a `Fault` effect, two handler classes and `with`. [cpu.py](cpu.py) answers `Tensor` over host lists and [gpu.py](gpu.py) over GPU buffers. [tokenizer.py](tokenizer.py) is the byte-level BPE tokenizer. [model.py](model.py) holds the constants, the records, the checkpoint loader, the forward pass and the KV-cached step. [generate.py](generate/generate.py) is the Default sampler in the Python flavor. The [Python flavor guide](https://www.ospreylang.dev/docs/python/flavor/) explains the flavor and how it differs from Python.

```python
# osprey: flavor=python
# backends.py
@static_effect
class Tensor[B]:
    def build(size: int, kernel: Kernel) -> B: ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> B: ...


# gpu.py
@static_handler(Tensor[GpuBuffer[float]])
class on_gpu:
    def build(n, f):
        match gpuIota(n):
            case Success(value=value):
                return Success(value=gpuMap(value, f))
            case Error(message=message):
                return Error(message=toString(message))

    def mat_mul(a, b, rows, inner, cols):
        return answered(gpuMatMul(a, b, rows, inner, cols))


# model.py
def product(a, b, rows, inner, cols):
    return perform(Tensor.mat_mul(a, b, rows, inner, cols))
```

From the repository root:

```sh
make -C src/toolchain/targets/llms gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
make -C src/toolchain/targets/llms gpt2 FLAVOR=python BACKEND=lists PROMPT="The capital of France is" TOKENS=1
```

The output is byte for byte the Default example's output for the same stdin: the banner naming the device, the prompt and token lines, the generated count, the decoded text, and the `generation stopped:` and unknown-backend messages. `GPU=auto`, `GPU=metal` and `GPU=opencl` select the existing device adapters; a missing adapter stops execution with an error. The prompt takes one full forward pass and every later token one `forward_step` over the blocks' cached qkv rows, as in the Default sampler.

The E2E compiler test compares every element of 24 deterministic tensors against the independent [Python reference](../reference.py), at the real embedding width and sequence lengths 1/2/3, under both handlers and GC/ARC memory, including all intermediate values from a transformer block. Memory variants compile concurrently, then all handler variants run concurrently. Run it directly with an existing compiler:

```sh
python3 scripts/tests/check-python-gpt2.py --compiler target/debug/osprey
```

Pass `--device metal`, `--device opencl` or `--device auto` for a strict hardware comparison. That mode requires an adapter identity and completed device dispatches. Host arithmetic compares exactly; only values downstream of `exp`/`tanh` use the existing GPU precision bound.

The generator comparison itself is `python_gpt2_generator_output_is_byte_identical_to_the_default_generator` in `src/toolchain/crates/osprey-cli/tests/cross_flavor_equiv.rs`. It needs the downloaded checkpoint, so it runs with `cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test cross_flavor_equiv -- --ignored`; the checkpoint-free message comparison and the no-`.osp` check run in every suite.

Only generation is ported. The [training program](../train) is not being ported at the moment and stays Default-only; `make -C src/toolchain/targets/llms gpt2-train` rejects `FLAVOR=python`.

## Port notes

The authored sources live in [`src/osprey/examples/llms/gpt2/python`](README.md). They are self-contained Python-flavor modules: the `Tensor` static effect and its two static handlers, the arithmetic policy, the tokenizer, the checkpoint loader, the records, the forward pass, the KV-cached step and the sampler. The project imports no `.osp` module and its source root holds none. [Plan 0031](../../../../../../docs/plans/0031-python.md) tracks the complete flavor. Training is not being ported at the moment; the Osprey trainer in `src/osprey/examples/llms/gpt2/train` stays Default-only.

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
