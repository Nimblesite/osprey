---
layout: page.njk
title: "GPT-2 Runs on Osprey’s GPU Backend, in the Python Flavor Too"
excerpt: "A real language model, written in Osprey’s Default flavor and again in its Python flavor, generating identical text on the CPU and the GPU. An algebraic effect handler chooses where the arithmetic runs, and a training step on Metal or OpenCL is 3.3× faster than on the CPU on an Apple M4 Max."
description: "How Osprey runs the published GPT-2 model on CPUs and GPUs from Default and Python-flavor source, what Metal and OpenCL support today, and where performance is improving."
modified: 2026-10-17
tags: ["blog", "gpu", "gpt2", "machine-learning", "python-flavor"]
author: "Christian Findlay"
readingTime: 4
image: /assets/images/blog/gpt2-on-the-gpu-surface.png
imageWebp: /assets/images/blog/gpt2-on-the-gpu-surface.webp
imageAlt: "A cyan wireframe osprey above a GPU chip and layered grids of glowing tensor values"
---

Osprey can run **GPT-2, the 124-million-parameter language model, on a GPU**. It loads the weights [exactly as Hugging Face publishes them](https://huggingface.co/openai-community/gpt2), generates text and runs a training step. The model itself is Osprey code: attention, matrix multiplication, predictions and weight updates.

**The model is also written in Osprey’s [Python flavor](/docs/python-flavor/).** Python-flavor source looks like typed Python, with `def`, annotations and decorated classes, but it is Osprey, not Python: the `osprey` compiler turns it into a native program, and CPython never runs it. The Python-flavor GPT-2 imports no Default-flavor code and prints exactly the same bytes as the Default version on host lists, Metal and OpenCL.

On an Apple M4 Max, the training step takes **7.6 seconds on Metal versus 25.2 seconds on the CPU**: about **3.3× faster**. Metal and OpenCL run the same model source.

## Written in the Python flavor

Osprey reads source through flavors. Each flavor parses its own syntax tree and lowers it into one shared AST, so inference, effects, modules and code generation are the same whichever syntax you write. The Python flavor is one of these flavors. Here is the GPT-2 effect and its GPU handler, from [the Python-flavor port](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python). The marker on the first line is what makes a `.py` file Osprey:

```python
# osprey: flavor=python
@static_effect
class Tensor[B]:
    def build(size: int, kernel: Kernel) -> B: ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> B: ...


@static_handler(Tensor[GpuBuffer[float]])
class on_gpu:
    def build(n, f):
        return gpuMap(gpuIota(n), f)

    def mat_mul(a, b, rows, inner, cols):
        return or_abort(gpuMatMul(a, b, rows, inner, cols))


def product(a, b, rows, inner, cols):
    return perform(Tensor.mat_mul(a, b, rows, inner, cols))
```

The [Python flavor guide](/docs/python-flavor/) covers the rest of the syntax: functions, records, `match`, effects, handlers, modules and mixed-flavor projects. It also explains how the Python flavor differs from Python itself: there is no interpreter, no Python packages and no exceptions, and so far only generation has been ported; training is still Default-only.

## Write the model once

Osprey provides typed GPU buffers and operations that apply ordinary Osprey functions to their contents. The compiler turns supported functions into GPU kernels—the small programs a GPU executes. You write the calculation in Osprey; you do not need a separate CUDA or Metal version of the model.

GPT-2 describes its calculations through a small set of array operations, declared as the algebraic effect `Tensor`. Two handlers answer those operations: one with CPU lists, one with GPU buffers executed through Metal or OpenCL. The model performs the operations and never names a buffer type, so the handler the program installs decides where the arithmetic runs. Choosing the handler is the only change; the model’s calculations stay the same. The effect is static, so the compiler resolves the handler before the program runs and nothing is looked up at run time. [One model, two backends](/docs/one-model-two-backends/) walks through it, and both [Default](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/model) and [Python](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python) sources are available to read.

## What actually runs on the GPU?

The CPU turns your prompt into tokens and loads the weights. The GPU handles the model’s layers, attention calculations, predictions and training calculations.

Weights upload once. Intermediate results stay on the GPU for the next operation, avoiding a trip back to the CPU after every calculation. The CPU coordinates the work, handles text and reads back the results the program needs.

Generation reuses earlier calculations as it adds tokens. The training example updates the weights once and checks the prediction error again.

An independent reference implementation, [`reference.py`](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/reference.py), is real Python, run by CPython, and checks the calculations of both Osprey versions. Recorded CPU and GPU runs generated the same text, with closely agreeing training results. The [verification details](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/README.md) explain the small numerical differences.

## Metal, OpenCL and CUDA

Osprey’s GPU interface has no CUDA dependency.

| Execution path | Current support |
| --- | --- |
| CPU | GPU-buffer operations on the host, plus a list implementation for comparison |
| Metal | Implemented for Apple GPUs |
| OpenCL | Implemented; provides a route to compatible AMD, Intel and NVIDIA devices with suitable drivers |
| CUDA | No dedicated CUDA backend today |

These measurements used an Apple M4 Max through Metal and OpenCL, not every supported GPU vendor.

## Where acceleration helps today

| Workload | CPU | Metal | OpenCL |
| --- | ---: | ---: | ---: |
| One training step | 25.2 s | **7.6 s** | **7.8 s** |
| Generate 20 tokens | **6.5 s** | 6.9 s | 6.8 s |

These recorded measurements are from 16 October 2026: Apple M4 Max, release build, ARC memory mode, a five-token prompt and the median of three runs. Times include loading the weights.

Training gives the GPU larger pieces of work and gets a clear speedup. Short text generation still pays for many small GPU launches. Combining those jobs and adding faster numerical formats are further work; this version uses carefully checked 64-bit arithmetic.

## Run it

From the repository root, after building Osprey:

```bash
# Choose the backend available on your machine: metal, opencl or host.
make gpt2 GPU=metal PROMPT="The capital of France is" TOKENS=20
make gpt2-train GPU=metal PROMPT="The capital of France is"
# The same generator, written in the Python flavor:
make gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
```

The first run downloads the weights and tokenizer data. The program prints its actual backend and reports unsupported GPU work without silently substituting CPU execution.

[Try the complete example](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2), and compare its CPU and GPU runs. The model and its GPU calculations are Osprey code you can read and change.
