# GPT-2 in the Python flavor

The whole program is Osprey source in the Python flavor: no `.osp` module is imported and the project's one source root holds none. These `.py` files start with `# osprey: flavor=python` and are compiled by `osprey`; they are not Python, and CPython cannot run them. The real Python in this example is [reference.py](../reference.py), the independent oracle. [backends.py](backends.py) declares the `Tensor` static effect and spells the arithmetic policy with a `Fault` effect, two handler classes and `with`. [cpu.py](cpu.py) answers `Tensor` over host lists and [gpu.py](gpu.py) over GPU buffers. [tokenizer.py](tokenizer.py) is the byte-level BPE tokenizer. [model.py](model.py) holds the constants, the records, the checkpoint loader, the forward pass and the KV-cached step. [generate.py](generate/generate.py) is the Default sampler in the Python flavor. The [Python flavor guide](https://www.ospreylang.dev/docs/python-flavor/) explains the flavor and how it differs from Python.

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
        return gpuMap(gpuIota(n), f)

    def mat_mul(a, b, rows, inner, cols):
        return or_abort(gpuMatMul(a, b, rows, inner, cols))


# model.py
def product(a, b, rows, inner, cols):
    return perform(Tensor.mat_mul(a, b, rows, inner, cols))
```

From the repository root:

```sh
make -C apps gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
make -C apps gpt2 FLAVOR=python BACKEND=lists PROMPT="The capital of France is" TOKENS=1
```

The output is byte for byte the Default example's output for the same stdin: the banner naming the device, the prompt and token lines, the generated count, the decoded text, and the `generation stopped:` and unknown-backend messages. `GPU=auto`, `GPU=metal` and `GPU=opencl` select the existing device adapters; a missing adapter stops execution with an error. The prompt takes one full forward pass and every later token one `forward_step` over the blocks' cached qkv rows, as in the Default sampler.

The E2E compiler test compares every element of 24 deterministic tensors against the independent [Python reference](../reference.py), at the real embedding width and sequence lengths 1/2/3, under both handlers and GC/ARC memory, including all intermediate values from a transformer block. Memory variants compile concurrently, then all handler variants run concurrently. Run it directly with an existing compiler:

```sh
python3 scripts/check-python-gpt2.py --compiler target/debug/osprey
```

Pass `--device metal`, `--device opencl` or `--device auto` for a strict hardware comparison. That mode requires an adapter identity and completed device dispatches. Host arithmetic compares exactly; only values downstream of `exp`/`tanh` use the existing GPU precision bound.

The generator comparison itself is `python_gpt2_generator_output_is_byte_identical_to_the_default_generator` in `crates/osprey-cli/tests/cross_flavor_equiv.rs`. It needs the downloaded checkpoint, so it runs with `cargo test -p osprey-cli --test cross_flavor_equiv -- --ignored`; the checkpoint-free message comparison and the no-`.osp` check run in every suite.

Only generation is ported. The [training program](../train) is not being ported at the moment and stays Default-only; `make -C apps gpt2-train` rejects `FLAVOR=python`.

The [port notes](../../../docs/plans/python-flavor-port/README.md) show the concrete changes from reference Python.
