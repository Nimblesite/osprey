---
mlTwins: manual
layout: page
title: One model, two backends
description: Write a numeric program once against a static effect, then let a handler decide whether it runs over host lists or on the GPU surface. GPT-2 is the worked example.
permalink: /docs/one-model-two-backends/
tags: [algebraic-effects, handlers, gpu, static-effects]
---

Write numeric code against a **static effect**. Install a list handler for host computation or a GPU-buffer handler for Metal/OpenCL. The compiler specialises each region and removes runtime effect dispatch; choosing the handler changes the implementation while keeping the model's operations.

The worked implementation is [124M-parameter GPT-2](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2): forward evaluation, greedy generation and a Default-flavor training step. The Python-flavor port currently covers generation.

## See the same product on two backends

{% from "native-capture.njk" import capture %}

{{ capture("/assets/images/guides/product-host.png", "Osprey matrix product running through the host GPU surface and returning first element 7", "Small reproducible matrix product · host backend · both product and read failures are handled.", 1440, 920) }}

{{ capture("/assets/images/guides/product-metal.png", "The same Osprey matrix product running on Metal and returning first element 7", "The same source · Metal backend · actual device execution, not a performance comparison.", 1440, 920) }}

The pictured 2 × 2 product multiplies `[1, 2; 3, 4]` by itself; its first element is 7. Select the GPU surface's backend with `OSPREY_GPU=host` or `OSPREY_GPU=metal`. This small example demonstrates execution and Result handling; it does not benchmark GPT-2.

<details>
<summary>Copy and run the small product example</summary>

Save as `product.py`:

```python
# osprey: flavor=python

def read_product(result):
    match result:
        case Error(message=message):
            return f"product failed: {message}"
        case Success(value=buffer):
            match gpuGet(buffer, 0):
                case Error(message=message):
                    return f"read failed: {message}"
                case Success(value=value):
                    return f"{gpuDevice()}: first element = {value}"


def main():
    values = toGpu([1.0, 2.0, 3.0, 4.0])
    print(read_product(gpuMatMul(values, values, 2, 2, 2)))
```

```sh
OSPREY_GPU=host osprey product.py --run
# macOS with Metal:
OSPREY_GPU=metal osprey product.py --run
```

</details>

For the complete model, from a built checkout:

```sh
make -C src/toolchain/targets gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=gpu
make -C src/toolchain/targets gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=lists
```

The first output line identifies the backend. Compare subsequent output under the documented host/device tolerances below.

## Declare a fallible tensor contract

`Tensor<B>` lists the operations; the handler chooses the buffer representation `B`. Products and checkpoint loads return **Result**. These signatures come directly from the current implementation:

```osprey
{{ gpuExample.nativeTensor | safe }}
```

The Python-flavor declaration uses the same contract (selected operations):

```python
# osprey: flavor=python
{{ gpuExample.tensor | safe }}
```

In ML syntax, the corresponding fallible operation signatures are:

```osprey-ml
matMul : (B, B, int, int, int) => Result<B, string>
load : (string, string) => Result<B, string>
```

Inference carries `B` through model helpers. `Kernel` and `Combine` name the callback shapes; the complete modules declare them alongside the effect.

## Install the implementation

The [list handler](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/model/cpu.osp) stores tensors in host lists and loads safetensors with `loadTensor`. Matrix accumulation follows `k` order from `0.0`; shape mismatches and failed loads return Error.

The [GPU handler](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/model/gpu.osp) uses `GpuBuffer<float>`, GPU kernels, `gpuMatMul` and `gpuLoadTensor`. Its current product/load arms forward their results through `answered`, which preserves Success or converts the error reason to text:

```osprey
{{ gpuExample.nativeGpuProduct | safe }}
```

The model handles those Results through `Backends::stop` inside `Backends::strictly`: its control effect answers the whole region with Error. Host arithmetic faults use that same boundary. Failed products and loads do not become fabricated tensors. On devices, **device-kernel faults currently abort rather than invoke a recovering arithmetic policy**; see the [GPU contract](/spec/0034-gpucomputation/).

`Backends::under` selects an already-specialised list or GPU region. Unknown backend names run no model and produce a diagnostic. A dynamically computed handler requires a dynamic effect; it cannot select an arbitrary static implementation at runtime.

## What is checked

- Both handlers must answer the declared operations with compatible types.
- Kernel callbacks must be stage-legal: pure, or requiring only effects discharged by another static region. A runtime effect such as `Log.write` is rejected by name.
- The [bounded tensor regression](https://github.com/Nimblesite/osprey/blob/main/tests/effects/staged/tensor_backends.test.osp) compares a width-4 transformer against independent expected values and compares both interpretations. Its ML twin shares the golden; the corpus runs under GC, ARC and wasm32.

## Measurements and limits

Historical measurements below ran **both GPU-surface and list handlers on the CPU** with `OSPREY_GPU=host`, not on a GPU. [GPT-2's README](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2) supplies device timings and reproduction details.

| 13 October 2026 · Ryzen 9 3900X · Linux 6.18 (WSL2) · release · ARC | GPU surface on host | Lists |
| --- | --- | --- |
| 20 greedy tokens, five-token prompt, KV cache | 19.6 s, 1.4 GB peak | 7 min 18 s, 1.7 GB peak |
| One training step, five-token prompt | 52 s, 3.6 GB peak | 9 min, 4.6 GB peak |

Buffers remain device-resident until read. Kernels are not fused: each is a separate dispatch. Reduction runs in element order; large reductions use per-row kernels followed by a reduction of rows. Reassociation changes numerical results. Device transcendental functions can differ from host libm in the last digit, so host parity does not establish universal device bit equality. Test your workload against the documented tolerances.

[Effects and static discharge](/spec/0017-algebraiceffects/) · [GPU operations and safety limits](/spec/0034-gpucomputation/)
