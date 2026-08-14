# Chapter 12 — Ship a real program

## Reader outcome

Choose `--check`, `--run`, or `--compile`, select a supported target, and describe what the resulting artifact depends on.

## Flight Log state

The tested project becomes a native executable and a supported WebAssembly artifact with platform limits documented beside each build.

## Core sections

1. Check, run, and compile are different jobs
2. Native code travels through LLVM and clang
3. WebAssembly supports a portable subset
4. Debug information and profiling answer different questions
5. Memory management is a build choice
6. Measure before changing the memory mode
7. C libraries are powerful and outside the safety guarantee

## Compiler-feedback exercise

Attempt one unsupported target/runtime combination and preserve the actual failure as a platform qualification, not a workaround recipe.

## Flight Log checkpoint

Produce a native artifact, record its command and environment, then build only the portion supported on WebAssembly.

## Planned visuals

- Source-to-target compile pipeline
- Native versus Wasm decision map
- Memory-mode comparison

## Source map

`0018-MemoryManagement`, `0019-ForeignFunctionInterface`, `0022-WebAssemblyTarget`, `0028-Profiler`

