---
mlTwins: manual
layout: page
title: Run the same model on CPU or GPU
description: Write a model's maths once. A handler decides whether it runs on the CPU or the GPU, and the compiler builds a separate fast version for each. GPT-2 is the worked example.
permalink: /docs/cpu-or-gpu/
tags: [algebraic-effects, handlers, gpu, static-effects]
---

**Write a model's maths once, and choose where it runs by choosing a handler.** The model asks for operations such as "multiply these matrices" through an effect called `Tensor`. One handler answers with ordinary lists on the CPU. Another answers with GPU buffers on Metal or OpenCL. The model code is the same in both cases.

`Tensor` is a *static* effect: the handler is fixed when the program is compiled, so the compiler builds a separate version of the model for each handler and nothing is looked up at run time. The worked example is [GPT-2 (124M)](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/gpt2), with generation and a training step.

## Try it

{% from "native-capture.njk" import capture %}

{{ capture("/assets/images/guides/product-host.png", "Osprey matrix product running through the host GPU surface and returning first element 7", "A 2 × 2 matrix product on the CPU: the first element is 7.", 1440, 920) }}

{{ capture("/assets/images/guides/product-metal.png", "The same Osprey matrix product running on Metal and returning first element 7", "The same source on a Metal GPU: the same answer.", 1440, 920) }}

<details>
<summary>Copy and run this small example</summary>

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

Run all of GPT-2 on either backend from a built checkout. The first output line names the backend:

```sh
make -C llms gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=gpu
make -C llms gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=lists
```

## How it works

The effect lists the operations. `B` is the buffer type, and each handler picks its own: `List<float>` on the CPU, `GpuBuffer<float32>` on the GPU. Operations that can fail, such as a matrix product or loading weights, return `Result`:

```osprey
{{ gpuExample.nativeTensor | safe }}
```

The same declaration in Python flavor (selected operations):

```python
# osprey: flavor=python
{{ gpuExample.tensor | safe }}
```

And the failing operations in ML flavor:

```osprey-ml
matMul : (B, B, int, int, int) => Result<B, string>
load : (string, string) => Result<B, string>
```

The [CPU handler](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/llms/gpt2/model/cpu.osp) keeps tensors in lists. The [GPU handler](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/llms/gpt2/model/gpu.osp) runs GPU kernels; its product passes the GPU's `Result` straight back:

```osprey
{{ gpuExample.nativeGpuProduct | safe }}
```

If a product or load fails, the model stops and returns the `Error`; it never carries on with a made-up tensor. One exception: **device-kernel faults currently abort** instead of reaching an arithmetic handler (see the [GPU contract](/spec/0034-gpucomputation/)).

## What the compiler checks

- Both handlers answer every operation with matching types.
- GPU kernel code is pure: a kernel that tries to log, for example, is rejected by name.
- A [regression test](https://github.com/Nimblesite/osprey/blob/main/src/osprey/tests/effects/staged/tensor_backends.test.osp) runs a small transformer under both handlers and compares them with independent expected values.

## Speed and limits

These timings ran **both handlers on the CPU** (`OSPREY_GPU=host`), not on a GPU; [GPT-2's README](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/gpt2) has GPU timings.

| 13 October 2026 · Ryzen 9 3900X · ARC | GPU code on CPU | Lists |
| --- | --- | --- |
| 20 tokens, with KV cache | 19.6 s, 1.4 GB | 7 min 18 s, 1.7 GB |
| One training step | 52 s, 3.6 GB | 9 min, 4.6 GB |

Data stays on the GPU until it is read back. Kernels are not fused, so each one is a separate dispatch. GPU maths functions can differ from the CPU's in the last digit, so check your results against the documented tolerances.

[Effects specification](/spec/0017-algebraiceffects/) · [GPU specification](/spec/0034-gpucomputation/)
