# GPT-2 in the Python flavor

The GPT-2 model asks the `Tensor` effect to multiply matrices. The sampler selects a handler to run that multiplication. These excerpts show the same operation in the effect and its two handlers. [Effect declaration](backends.py) · [Host lists](cpu.py) · [GPU buffers](gpu.py)

```python
# backends.py
@static_effect
class Tensor[B]:
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> Result[B, str]: ...

# cpu.py
@static_handler(Tensor[list[float32]])
class on_lists:
    def mat_mul(a, b, rows, inner, cols):
        return product_of(a, b, rows, inner, cols) if len(a) == rows * inner and len(b) == inner * cols else misfit(a, b, rows, inner, cols)

# gpu.py
@static_handler(Tensor[GpuBuffer[float32]])
class on_gpu:
    def mat_mul(a, b, rows, inner, cols):
        return answered(gpuMatMul(a, b, rows, inner, cols))
```

Tokenization, checkpoint file reads and the loop that chooses each new token run on the CPU. A selected GPU receives a dense buffer on its first kernel use. Later kernels can reuse that buffer. A host read can bring the result back. Set `OSPREY_GPU_TRACE=1` to see completed dispatches with their kernel names and thread counts. [Tokenizer](tokenizer.py) · [Forward pass](model.py) · [Sampler](generate/generate.py) · [GPU dispatch and residency](../../../../../../docs/specs/runtime/0034-GPUComputation.md#device-resident-buffers--gpu-buffer-resident)

## Run the model

From the repository root, these commands fetch the published weights and tokenizer merges on first use. The first uses Metal. The second runs the list handler on the CPU.

```sh
make -C src/toolchain/targets/llms gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
make -C src/toolchain/targets/llms gpt2 FLAVOR=python BACKEND=lists PROMPT="The capital of France is" TOKENS=1
```

The sampler prints the device, prompt token IDs, generated count and decoded text. It runs one forward pass for the prompt and reuses each block’s cached state on subsequent tokens. The training example uses the Default flavor. [Training project](../train) · [Target definitions](../../../../../toolchain/targets/llms/Makefile)

## Check the handlers without model weights

Compile the fixture once. It prints each tensor’s name and elements for matrix multiplication, normalization, attention and a transformer block. Run the last command on a Metal-capable Mac.

```sh
cargo build --manifest-path src/toolchain/Cargo.toml -p osprey-cli --bin osprey
target/debug/osprey src/osprey/examples/llms/gpt2/python/check --compile --memory=arc -o target/gpt2-check
printf 'lists\n' | target/gpt2-check
printf 'gpu\n' | OSPREY_GPU=host target/gpt2-check
printf 'gpu\n' | OSPREY_GPU=metal OSPREY_GPU_TRACE=1 target/gpt2-check
```

On 2026-11-23 the three runs printed the same 48,687 binary32 values across 24 tensors. The Apple M4 Max completed 84 Metal dispatches. ARC reported zero live objects after each run. These figures cover the checkpoint-free fixture. [Fixture source](check/check.py)

With the real checkpoint installed, run this test from the repository root to compare the Default and Python generators byte for byte. On macOS both produced identical output for 20 tokens from the France prompt on host, Metal and OpenCL, and for one token from the Melbourne prompt on lists.

```sh
cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test cross_flavor_equiv python_gpt2_generator_output_is_byte_identical_to_the_default_generator -- --ignored
```

## Port notes

The independent reference uses Python. The Osprey compiler builds the marked `.py` modules in this directory. The port keeps the reference’s tensor order and uses the same checkpoint as the Default generator. [Python reference](../reference.py) · [Python flavor guide](https://www.ospreylang.dev/docs/python/flavor/)

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

## Sampling checks

On 2026-11-23 the ARC generator ran both prompts with budgets of 0, 1, 20 and 50 tokens on `host`, `metal:0` and `opencl:0` on an Apple M4 Max. The checkpoint SHA-256 was `248dfc3911869ec493c76e65bf2fcf7f615828b0254c12b473182f0f81d3a707`. All 24 processes exited 0. Each backend produced the same token IDs and decoded bytes for each prompt and budget.

The France prompt was `The capital of France is` (IDs `464 3139 286 4881 318`). The Melbourne prompt was `Melbourne is the capital of` (IDs `21102 12544 318 262 3139 286`). Append the UTF-8 text in the last column to the prompt to recover the decoded output. JSON quotes make the leading space and any line breaks visible.

| Prompt | Budget | Generated token IDs | Decoded continuation |
| --- | ---: | --- | --- |
| France | 0 | `[]` | `""` |
| France | 1 | `[262]` | `" the"` |
| France | 20 | `[262, 3139, 286, 262, 4141, 2066, 11, 290, 262, 3139, 286, 262, 4141, 2066, 318, 262, 3139, 286, 262, 4141]` | `" the capital of the French Republic, and the capital of the French Republic is the capital of the French"` |
| France | 50 | `[262, 3139, 286, 262, 4141, 2066, 11, 290, 262, 3139, 286, 262, 4141, 2066, 318, 262, 3139, 286, 262, 4141, 2066, 13, 198, 198, 464, 4141, 2066, 318, 262, 3139, 286, 262, 4141, 2066, 13, 198, 198, 464, 4141, 2066, 318, 262, 3139, 286, 262, 4141, 2066, 13, 198, 198]` | `" the capital of the French Republic, and the capital of the French Republic is the capital of the French Republic.\n\nThe French Republic is the capital of the French Republic.\n\nThe French Republic is the capital of the French Republic.\n\n"` |
| Melbourne | 0 | `[]` | `""` |
| Melbourne | 1 | `[262]` | `" the"` |
| Melbourne | 20 | `[262, 6638, 9747, 25219, 13, 198, 198, 464, 1748, 318, 1363, 284, 262, 4387, 1271, 286, 661, 287, 262, 995]` | `" the Australian Capital Territory.\n\nThe city is home to the largest number of people in the world"` |
| Melbourne | 50 | `[262, 6638, 9747, 25219, 13, 198, 198, 464, 1748, 318, 1363, 284, 262, 4387, 1271, 286, 661, 287, 262, 995, 11, 351, 517, 621, 352, 13, 20, 1510, 661, 2877, 287, 262, 1748, 13, 198, 198, 464, 1748, 318, 635, 1363, 284, 262, 4387, 1271, 286, 661, 287, 262, 995]` | `" the Australian Capital Territory.\n\nThe city is home to the largest number of people in the world, with more than 1.5 million people living in the city.\n\nThe city is also home to the largest number of people in the world"` |

Every sample used its full budget. A separate control supplied logits `[1, 9, 9]` to the sampler’s `Model.index_of(logits, Model.peak(logits))` expression. All three backends selected index 1. In two copies of the sampler, forcing the first prediction to `END_OF_TEXT` (50256) produced zero new tokens, and forcing the next prediction to `END_OF_TEXT` produced one. All nine control processes exited 0.

To repeat the matrix from the repository root, use the weights and merges downloaded by the `gpt2` target. The sampler prints decoded text and prompt IDs. The second binary adds one output line containing the IDs it passed to the decoder. Removing this line reproduced the first binary’s full output byte for byte on all 24 runs.

```sh
src=src/osprey/examples/llms/gpt2/python
work=$(mktemp -d)
cp -R "$src" "$work/trace"
awk '{print} /^    print\(f"generated / {print "    print(f\"@ids:{Tokenizer.ids_text(out)}\")"}' \
  "$src/generate/generate.py" > "$work/trace/generate/generate.py"
shasum -a 256 src/osprey/examples/llms/gpt2/weights/model.safetensors
target/debug/osprey "$src/generate" --compile --memory=arc -o "$work/generate"
target/debug/osprey "$work/trace/generate" --compile --memory=arc -o "$work/ids"
for app in "$work/generate" "$work/ids"; do
  for device in host metal opencl; do
    for count in 0 1 20 50; do
      for prompt in 'The capital of France is' 'Melbourne is the capital of'; do
        printf '%s\n%s\ngpu\n' "$prompt" "$count" | OSPREY_GPU="$device" "$app" || exit 1
      done
    done
  done
done
```
