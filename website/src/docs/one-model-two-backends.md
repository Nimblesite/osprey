---
mlTwins: manual
layout: page
title: One model, two backends
description: Write a numeric program once against a static effect, then let a handler decide whether it runs over host lists or on the GPU surface. GPT-2 is the worked example.
permalink: /docs/one-model-two-backends/
tags: [algebraic-effects, handlers, gpu, static-effects]
---

Write the model against an effect that declares its tensor operations. Two handlers implement those operations: one with host lists, one with GPU buffers. The caller chooses a handler, and the compiler specialises the model for it, removing runtime effect dispatch.

The 124M-parameter GPT-2 model in [`examples/gpt2`](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2) includes a forward pass, greedy generation and a training step. Both handlers produce byte-identical output on the CPU.

```bash
make gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=gpu
make gpt2 PROMPT="The capital of France is" TOKENS=20 BACKEND=lists
```

The first line of each run names which handler answered: `gpt2 (124M) on host` or `gpt2 (124M) on lists`. Every other line is the same.

## Declare the operations

`Tensor<B>` declares buffer construction, access, length, combination, reduction, lookup, matrix products, list conversion, checkpoint loading and device identification. The handler chooses `B`; the model uses it without naming a concrete representation. Type aliases describe the two callback shapes:

```osprey
export type Kernel = fn(int) -> float
export type Combine = fn(float, float) -> float

export static effect Tensor<B> {
    build: fn(int, Kernel) -> B
    get: fn(B, int) -> float
    length: fn(B) -> int
    zipWith: fn(B, B, Combine) -> B
    reduce: fn(B, float, Combine) -> float
    indexOf: fn(B, float) -> int
    matMul: fn(B, B, int, int, int) -> B
    fromList: fn(List<float>) -> B
    load: fn(string, string) -> B
    device: fn() -> string
}
```

```osprey-ml
export type Kernel = (int) -> float
export type Combine = (float, float) -> float

export static effect Tensor B
    build : (int, Kernel) => B
    get : (B, int) => float
    length : B => int
    zipWith : (B, B, Combine) => B
    reduce : (B, float, Combine) => float
    indexOf : (B, float) => int
    matMul : (B, B, int, int, int) => B
    fromList : List<float> => B
    load : (string, string) => B
    device : Unit => string
```

`static` is the important word. A static effect is discharged at compile time: the compiler rewrites every `perform` into the body of the arm that answers it ([`[STAGE-HANDLE-STATIC]`](/spec/0017-algebraiceffects/)). The declared types travel with the rewrite, so a kernel the model writes as `fn(g, x) => g` is still a `fn(float, float) -> float` after discharge, and the backend lowers it as a float kernel rather than a kernel over raw words.

## Write the model against the effect

Every operation in the model is a `perform`. LayerNorm over the `t` rows of a `[t, width]` tensor builds two statistics and one normalised buffer:

```osprey
fn rowMeans(x, t) = perform Backends::Tensor.build(t, fn(r) => sumRange(x, r * width, width) / widthF)
fn rowVariances(x, t, means) = perform Backends::Tensor.build(t, fn(r) => squaredDeviation(x, r * width, width, at(means, r)) / widthF)

fn layerNorm(x, t, g, b) = {
    let means = rowMeans(x, t)
    let variances = rowVariances(x, t, means)
    perform Backends::Tensor.build(t * width, fn(i) => normalised(x, means, variances, g, b, i))
}
```

```osprey-ml
rowMeans (x, t) = perform Backends::Tensor.build t (\r => sumRange (x, r * width, width) / widthF)
rowVariances (x, t, means) = perform Backends::Tensor.build t (\r => squaredDeviation (x, r * width, width, at (means, r)) / widthF)

layerNorm (x, t, g, b) =
    means = rowMeans (x, t)
    variances = rowVariances (x, t, means)
    perform Backends::Tensor.build (t * width) (\i => normalised (x, means, variances, g, b, i))
```

`at(m, i)` is `perform Backends::Tensor.get(m, i)`. A linear layer is one product and one bias gather. The weights are records whose fields are `B`:

```osprey
type Layer<B> = {
    ln1g: B, ln1b: B, qkvW: B, qkvB: B, projW: B, projB: B,
    ln2g: B, ln2b: B, fcW: B, fcB: B, outW: B, outB: B
}

fn linear(x, rows, inner, cols, w, bias) = {
    let value = perform Backends::Tensor.matMul(x, w, rows, inner, cols)
    perform Backends::Tensor.build(rows * cols, fn(i) => at(value, i) + at(bias, i % cols))
}
```

```osprey-ml
type Layer B =
    ln1g : B
    ln1b : B
    qkvW : B
    qkvB : B
    projW : B
    projB : B
    ln2g : B
    ln2b : B
    fcW : B
    fcB : B
    outW : B
    outB : B

linear (x, rows, inner, cols, w, bias) =
    value = perform Backends::Tensor.matMul x w rows inner cols
    perform Backends::Tensor.build (rows * cols) (\i => at (value, i) + at (bias, i % cols))
```

Hindley-Milner inference carries `B` through the model's records and helpers without repeating type annotations.

## Answer with lists

The first handler answers every operation with ordinary list code. The product accumulates in `k` order from `0.0`, and that order is the contract: the two handlers must produce the same doubles, not nearly the same. Both handlers answer `indexOf` in the same two steps: element `i` names itself when it holds the value and the length otherwise (`hit`), and the smallest name wins (`minI`). Weights load with `loadTensor`, which reads a safetensors tensor straight into a `List<float>`, so this handler never touches the GPU surface: a program that installs only it links no GPU code. Each handler has its own `orAbort`, which ends the run through that handler's runtime when a load or a product fails, so the model never computes on a fabricated empty tensor.

```osprey
export fn onLists(work) = {
    handle static Tensor<List<float>> {
        build n f => range(0, n) |> map(f) |> toList()
        get b i => listGet(b, i) ?: 0.0
        length b => listLength(b)
        zipWith a b f => range(0, listLength(a)) |> map(fn(i) => f(listGet(a, i) ?: 0.0, listGet(b, i) ?: 0.0)) |> toList()
        reduce b seed f => range(0, listLength(b)) |> fold(seed, fn(a, i) => f(a, listGet(b, i) ?: 0.0))
        indexOf b x => range(0, listLength(b)) |> map(fn(i) => hit(listGet(b, i) ?: 0.0, x, i, listLength(b))) |> fold(listLength(b), minI)
        matMul a b rows inner cols => range(0, rows * cols) |> map(fn(i) => range(0, inner) |> fold(0.0, fn(acc, k) => acc + (listGet(a, intDiv(i, cols) * inner + k) ?: 0.0) * (listGet(b, k * cols + i % cols) ?: 0.0))) |> toList()
        fromList xs => xs
        load path name => orAbort(loadTensor(path, name))
        device => "lists"
    }
    work()
}
```

```osprey-ml
export onLists work =
    handle static Tensor<List<float>>
        build n f => range 0 n |> map f |> toList
        get b i => listGet b i ?: 0.0
        length b => listLength b
        zipWith a b f => range 0 (listLength a) |> map (\i => f (listGet a i ?: 0.0, listGet b i ?: 0.0)) |> toList
        reduce b seed f => range 0 (listLength b) |> fold seed (\(a, i) => f (a, listGet b i ?: 0.0))
        indexOf b x => range 0 (listLength b) |> map (\i => hit (listGet b i ?: 0.0, x, i, listLength b)) |> fold (listLength b) minI
        matMul a b rows inner cols => range 0 (rows * cols) |> map (\i => range 0 inner |> fold 0.0 (\(acc, k) => acc + (listGet a (intDiv (i, cols) * inner + k) ?: 0.0) * (listGet b (k * cols + i % cols) ?: 0.0))) |> toList
        fromList xs => xs
        load path name => orAbort (loadTensor path name)
        device => "lists"
    work ()
```

## Answer with the GPU surface

The second handler answers the same operations with `GpuBuffer<float>`: each `build` is a kernel over `gpuIota`, each `zipWith` and `reduce` forwards the model's callback to a kernel, `matMul` is `gpuMatMul`, and `device` is `gpuDevice()`. When `OSPREY_GPU` selects a device, every one of those kernels and products is compiled from this same source and executed on it, through Metal or OpenCL; with `OSPREY_GPU=host` the same handler runs them on the CPU. The model never changes.

```osprey
export fn onGpu(work) = {
    handle static Tensor<GpuBuffer<float>> {
        build n f => gpuIota(n) |> gpuMap(f)
        get b i => gpuGet(b, i) ?: 0.0
        length b => gpuLength(b)
        zipWith a b f => gpuZipWith(a, b, f)
        reduce b seed f => b |> gpuFold(seed, f)
        indexOf b x => gpuIota(gpuLength(b)) |> gpuMap(fn(i) => hit(gpuGet(b, i) ?: 0.0, x, i, gpuLength(b))) |> gpuFold(gpuLength(b), minI)
        matMul a b rows inner cols => orAbort(gpuMatMul(a, b, rows, inner, cols))
        fromList xs => toGpu(xs)
        load path name => orAbort(gpuLoadTensor(path, name))
        device => gpuDevice()
    }
    work()
}
```

```osprey-ml
export onGpu work =
    handle static Tensor<GpuBuffer<float>>
        build n f => gpuIota n |> gpuMap f
        get b i => gpuGet b i ?: 0.0
        length b => gpuLength b
        zipWith a b f => gpuZipWith a b f
        reduce b seed f => b |> gpuFold seed f
        indexOf b x => gpuIota (gpuLength b) |> gpuMap (\i => hit (gpuGet b i ?: 0.0, x, i, gpuLength b)) |> gpuFold (gpuLength b) minI
        matMul a b rows inner cols => orAbort (gpuMatMul a b rows inner cols)
        fromList xs => toGpu xs
        load path name => orAbort (gpuLoadTensor path name)
        device => gpuDevice ()
    work ()
```

Under `OSPREY_GPU=metal` or `opencl`, every kernel this handler launches runs on the GPU, including the next token's index and the training loss, so the CPU tokenizes, sequences the layers and prints. A kernel the device cannot compile stops the run and names itself; it never falls back to the CPU. `gpuDevice()` reports the adapter that answered, and the first line of each run prints it.

## Choose at the boundary

`main` is the one place that chooses. It reads the prompt, the token count and a backend name, then hands the whole run to `Backends::under`, which installs the handler that name selects, inside `Backends::strictly`, the arithmetic policy:

```osprey
fn main() = {
    let prompt = input()
    let count = parseInt(input()) ?: 20
    let backend = input()
    match Backends::strictly(fn() => Backends::under(backend, fn() => run(prompt, count))) {
        Success { value } => value
        Error { message } => print("generation stopped: ${message}")
    }
}

export fn strictly(work) = {
    handle Fault { halt fault => Error { message: fault } }
    handle Arith {
        overflow op l r _ => perform Fault.halt("integer overflow: ${l} ${op} ${r}")
        divideByZero op l => toFloat(perform Fault.halt("division by zero: ${l} ${op} 0"))
        remainderByZero l => perform Fault.halt("remainder by zero: ${l} % 0")
    }
    Success { value: work() }
}

export fn under(name, work) = match name {
    "gpu" => onGpu(work)
    "lists" => onLists(work)
    _ => print("unknown backend `${name}`: choose gpu or lists")
}
```

```osprey-ml
main () =
    prompt = input ()
    count = parseInt (input ()) ?: 20
    backend = input ()
    match Backends::strictly (\() => Backends::under (backend, \() => run (prompt, count)))
        Success value => value
        Error message => print "generation stopped: ${message}"

export strictly work =
    handle Fault
        halt fault => Error(message = fault)
    handle Arith
        overflow op l r _ => perform Fault.halt "integer overflow: ${l} ${op} ${r}"
        divideByZero op l => toFloat (perform Fault.halt "division by zero: ${l} ${op} 0")
        remainderByZero l => perform Fault.halt "remainder by zero: ${l} % 0"
    Success(value = work ())

export under (name, work) = match name
    "gpu" => onGpu work
    "lists" => onLists work
    _ => print "unknown backend `${name}`: choose gpu or lists"
```

`under` only forwards the run, so the compiler applies it at the call site, and each arm of its `match` becomes its own static region. A name that is neither runs nothing rather than silently landing on the GPU surface. The compiler specialises `run` and everything it reaches once per region, with the arms of that region inlined at every `perform`. The compiled program contains two copies of the model and no handler: the `Tensor` effect does not exist at run time. `Arith` is an ordinary runtime effect and stays, because a divisor the program computes needs a policy the handlers cannot supply. A fault there is a bug in the model, and a substituted `0.0` cannot be told apart from a real logit, so `strictly` substitutes nothing: each arm performs the `control` operation `Fault.halt`, which answers the whole region without resuming, and `main` receives an `Error` naming the operation and its operands. A device kernel's fault stops the run the same way.

## What the compiler proves

- **Both interpretations are total.** A static handler must answer every operation of its effect, so a new operation added to `Tensor` is a compile error in whichever handler forgot it ([`[STAGE-STATIC-TOTAL]`](/spec/0017-algebraiceffects/)).
- **Instantiations never cross.** `Tensor<List<float>>` and `Tensor<GpuBuffer<float>>` are different effects to the handler search. A request that names one is never answered by the other; a request that leaves the instantiation to inference takes the innermost region ([`[EFFECTS-GENERIC-INSTANTIATION]`](/spec/0017-algebraiceffects/)).
- **Forwarded kernels stay pure.** The GPU handler hands the model's callback to `gpuMap`, `gpuZipWith` and `gpuFold`. A callback that performs a runtime effect is rejected at compile time, by name: `kernel body is not stage-legal; it requires dynamic effects: Log.write` ([`[GPU-KERNEL-PURE]`](/spec/0034-gpucomputation/)).
- **The declared types hold.** The operation's written callback type is kept on the argument through discharge, so a kernel inside a generic helper keeps `fn(float, float) -> float` and is lowered over floats, under either handler.

## What the tests show

[`tests/effects/staged/tensor_backends.test.osp`](https://github.com/Nimblesite/osprey/blob/main/tests/effects/staged/tensor_backends.test.osp) runs a width-4 transformer block under both handlers and checks every element against values an independent oracle computed, then checks that the two handlers print the same doubles. Its ML twin shares the golden and must emit identical LLVM IR. The differential harness runs both under the GC and ARC memory backends and on wasm32.

For the full model's default prompt, both handlers print identical tokens and continuation, training loss, 148 gradient-tensor sums and updated loss. The GPU surface uses dense doubles and lifted kernel functions; the list handler boxes every element. Both columns below ran on the CPU (`OSPREY_GPU=host`); the [GPT-2 README](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2) has the Metal and OpenCL timings.

| Measured 2026-10-13, AMD Ryzen 9 3900X, Linux 6.18 (WSL2), release build, `--memory=arc` | `BACKEND=gpu` | `BACKEND=lists` |
| --- | --- | --- |
| 20 greedy tokens from the 5-token prompt, with the KV cache | 19.6 s, 1.4 GB peak | 7 min 18 s, 1.7 GB peak |
| One training step over the 5-token prompt | 52 s, 3.6 GB peak | 9 min, 4.6 GB peak |

## Limits

- Buffers are resident on the device from their first launch and come back only when the program reads them; there is no kernel fusion yet, so every kernel is its own dispatch. A `reduce` runs on one device thread in element order, which keeps its result identical to the host's, so a reduction over a large buffer is shaped as a per-row kernel and a `reduce` over the rows, as the training program's gradient checksums are.
- The device's `exp`, `ln` and `tanh` use a different implementation from host libm, so the training loss can differ from the host's in its last digit.
- A static handler is selected where it is written. `match input()` chooses between two regions the compiler has already specialised; a handler computed at run time needs a dynamic effect and a runtime handler instead.
- A callback forwarded through a static arm must be stage-legal: pure, or performing only effects another static region answers.
- The two handlers agree bit for bit only because both accumulate in the same order. A handler that reassociated a reduction would be a different interpretation, and the differential harness would say so.

The [GPU specification](/spec/0034-gpucomputation/) holds the contract every number above is measured against, and the [effects specification](/spec/0017-algebraiceffects/) covers static discharge, per-region backends and stage polymorphism.
