---
layout: page.njk
title: Benchmarks
description: How Osprey's elapsed time and peak memory compare to Rust, C, C#, Dart, OCaml, and Haskell on classic compute benchmarks.
date: "git Last Modified"
tags: ["benchmarks", "performance"]
author: "Christian Findlay"
---

Osprey compiles through LLVM to native binaries and WebAssembly. These benchmarks measure elapsed time and peak resident memory for recursive arithmetic, immutable collections and tree allocation.

## Current measurements

On **3 October 2026**, all **22 cases** were rebuilt and measured for Osprey's default, ARC, GC and WebAssembly backends. All **88 programs produced the expected output**. C and Rust were remeasured only for `fib`, `binarytrees` and `wordfreq`; the other languages' records were preserved.

The machine was an AMD Ryzen 9 3900X host running Linux under WSL2. Measurement processes were pinned to logical CPU 20. Tools were Clang 21.1.8, Rust 1.98.1, GCC 15.2.0 and Wasmtime 49.0.2. This was a shared machine: timing outliers occurred, particularly on short cases. The following values are **mean ± standard deviation in milliseconds**, not a controlled before/after speedup claim.

| Case | Osprey | Osprey ARC | Osprey GC | Rust | C |
| --- | ---: | ---: | ---: | ---: | ---: |
| fib | 64.0 ± 2.4 | 64.1 ± 1.9 | 68.5 ± 7.3 | 29.2 ± 2.5 | 28.1 ± 2.5 |
| binarytrees | 612.6 ± 14.0 | 479.0 ± 18.8 | 2362.7 ± 352.0 | 1967.2 ± 685.5 | 891.5 ± 175.1 |
| wordfreq | 112.6 ± 48.4 | 62.3 ± 4.2 | 172.9 ± 28.9 | 9.1 ± 3.1 | 1.9 ± 2.8 |

These three rows compare measurements made on this machine. `wordfreq` remains substantially slower in Osprey, even with ARC. Its persistent map and the mutable hash tables used by other implementations have different allocation and update costs. `fib` also shows a time gap. The tree result favors ARC in this sample, but the baseline variance prevents a precise speedup claim.

`quicksort` and `mergesort` were added on **5 October 2026** and measured in every language in one run on a different machine: an Apple M4 Max laptop running macOS 26.6, with Clang 22.1.8, Rust 1.99.0, .NET 10.0.303, Dart 3.13.3, OCaml 5.4.1, GHC 9.14.1 and Wasmtime 46.0.1. All 22 programs produced the expected output. Their two rows compare with each other, not with the 3 October rows. Both sorts build each partition and merged run as a new persistent list, so the default allocator peaks at 474 MB and 848 MB where ARC stays near 2 MB, and Osprey takes 129 ms and 231 ms where Rust takes 4 ms.

## Full recorded results

**The full tables combine this Osprey run and the six fresh C/Rust measurements with historical records.** The historical records do not identify their machine. Cross-language aggregate ratios and highlighted minimum cells in these tables are therefore descriptions of the stored data, not controlled performance comparisons. Use the three rows above for the current C/Rust sample.

The tables are generated from the harness output by [`benchmarks/report.py`](https://github.com/Nimblesite/osprey/blob/main/benchmarks/report.py). Osprey columns are highlighted; a star marks a lowest recorded Osprey value subject to the provenance limitation above.

{% include "benchmarks-tables.html" %}

## Methodology

Sources and deterministic expected outputs live in [`benchmarks/cases/`](https://github.com/Nimblesite/osprey/tree/main/benchmarks/cases). Each language runs the same workload and checksummed result. Iteration and collection representations differ, so source inspection matters when interpreting a gap.

1. **Build once, time execution.** Native programs are compiled to persistent executables. Compiler and linker time are excluded. Wasmtime execution includes VM startup.
2. **Check correctness first.** Every binary must produce its case's expected output before it is timed. A failed Osprey refresh leaves published results unchanged.
3. **Elapsed time.** `hyperfine -N --warmup 3 --min-runs 10` reports the mean and standard deviation. Seeded cases receive `0` on standard input.
4. **Memory.** `/usr/bin/time` records peak RSS, taking the maximum of three runs. Wasmtime host RSS is not reported as module memory.
5. **WebAssembly stack.** Every language's Wasmtime wrapper uses `-W max-wasm-stack=4194304`. The explicit 4 MiB call-stack budget lets the unchanged `ackermann` workload complete with `8189`; its previous failure exhausted the host's default budget.

| Language | Release command |
| --- | --- |
| Osprey | `osprey <file>.osp --compile` — LLVM IR through Clang `-O2` |
| Osprey ARC / GC | The same command with `--memory=arc` / `--memory=gc` |
| Osprey WebAssembly | `osprey <file>.osp --target=wasm32 --compile`, executed under Wasmtime |
| Rust | `rustc -C opt-level=3 -C overflow-checks=off` |
| C | `cc -O2` |

Other recorded columns use the commands in the [benchmark harness](https://github.com/Nimblesite/osprey/blob/main/benchmarks/run.sh).

## Arithmetic and memory policies

Osprey's integer operators return plain values. Overflow transfers to the explicit `Arith` policy installed by the benchmark; wrapping and saturating helpers state their policy directly. Unhandled fallible arithmetic is rejected. The Rust command disables overflow checks, so these configurations have different arithmetic guarantees. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

On `binarytrees`, the default allocator peaked at **601.6 MiB**, ARC at **3.11 MiB**, and GC at **14.25 MiB**. The default allocator retains general heap allocations; ARC and GC reclaim them. The source and expected output are identical across those backends. Reclamation costs vary by workload, as the timing table shows.

Osprey has no tail-call optimization. Iterative cases use `range |> fold`; recursive cases retain the stated recursion. These measurements do not establish general performance parity with C or Rust.

## Reproduce

```bash
make bench-osprey                # refresh all Osprey backends only
BENCH_FILTER=wordfreq make bench # one workload across installed toolchains
```

Results are tracked in `benchmarks/results/results.json` and `results.html`; the website tables are regenerated from the same data. A partial run preserves unmeasured cells. Keep the measurement date, machine, selected languages and timing variance beside any comparison drawn from them.
