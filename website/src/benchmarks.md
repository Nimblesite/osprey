---
layout: page.njk
wide: true
title: Benchmarks
description: How Osprey's elapsed time and peak memory compare to Rust, C, C#, Dart, OCaml, and Haskell on classic compute benchmarks.
date: "git Last Modified"
tags: ["benchmarks", "performance"]
author: "Christian Findlay"
---

Elapsed execution time and peak resident memory for arithmetic, collections and allocation workloads. [Sources and expected outputs](https://github.com/Nimblesite/osprey/tree/main/benchmarks/cases) are public; each language must produce the same result before it is timed.

## Reading the figures

- **CPU time** is elapsed wall-clock time, including process startup, excluding compilation. Cells show the mean in milliseconds or seconds; lower is better.
- **Peak memory** is the maximum resident set size (RSS) across three runs, in KiB or MiB. WebAssembly shows `—` because Wasmtime's host RSS does not measure the module's memory alone.
- **Osprey**, **ARC** and **GC** compile the same source with different memory backends. The historical **Osprey** column uses the then-default unmanaged allocator, which retains general allocations; ARC and GC reclaim them. Current native builds default to GC. These stored figures have not been remeasured for that default change.
- **Highlighting** identifies Osprey columns and the lowest recorded cells. A star marks a strictly lowest value for the historical unmanaged Osprey column. Summary ratios are geometric means of Osprey time divided by the comparison language's time: `1.59×` means 1.59 times as long; below `1×` means less time.

**The tables combine runs from different dates and machines.** Some historical records have no machine metadata. Ratios, wins and highlighted minima describe those stored records; they do not establish a controlled cross-language speedup.

{% include "benchmarks-tables.html" %}

## How measurements are collected

1. Compile each program once to a persistent executable. Build and link time are excluded; WebAssembly timings include Wasmtime startup.
2. Check stdout against the case's `expected.txt`. Failed builds, wrong output and failed timing runs do not replace a valid recorded result.
3. Run `hyperfine -N --warmup 3 --min-runs 10` to collect the mean and standard deviation. Seeded workloads receive `0` on stdin for repeatable input.
4. Measure native peak RSS with `/usr/bin/time`, taking the maximum of three runs. Every Wasmtime wrapper uses the same 4 MiB call-stack budget (`-W max-wasm-stack=4194304`).

These are microbenchmarks. Collection representations and iteration styles differ across implementations: Osprey's persistent maps and lists allocate differently from mutable hash tables and arrays. Osprey has no tail-call optimization. Integer faults require an explicit [arithmetic policy](/spec/0037-arithmeticeffects/); the plain Rust and C columns use unchecked arithmetic.

| Language | Release command |
| --- | --- |
| Osprey | `osprey <file>.osp --compile` — LLVM IR through Clang `-O2` |
| Osprey ARC / GC | The same command with `--memory=arc` / `--memory=gc` |
| Osprey WebAssembly | `osprey <file>.osp --target=wasm32 --compile`, executed under Wasmtime |
| Rust | `rustc -C opt-level=3 -C overflow-checks=off` |
| C | `cc -O2` |

Other toolchain commands and optional columns are defined in [`benchmarks/run.sh`](https://github.com/Nimblesite/osprey/blob/main/benchmarks/run.sh).

## Measurement provenance

- **3 October 2026:** 22 cases across Osprey's four backends passed their output checks. Only `fib`, `binarytrees` and `wordfreq` were also remeasured in C and Rust. Host: AMD Ryzen 9 3900X, Linux under WSL2, pinned to logical CPU 20; Clang 21.1.8, Rust 1.98.1, GCC 15.2.0, Wasmtime 49.0.2. Shared-machine outliers affected short cases.
- **5 October 2026:** `quicksort` and `mergesort` were measured across languages on an Apple M4 Max, macOS 26.6; Clang 22.1.8, Rust 1.99.0, .NET 10.0.303, Dart 3.13.3, OCaml 5.4.1, GHC 9.14.1, Wasmtime 46.0.1. Compare these rows within that run.
- **Historical records:** 148 missing cells were recovered from [20 August results](https://github.com/Nimblesite/osprey/blob/713366f9e9e6ff9b7dec26691a1257f969df2d53/benchmarks/results/results.json) on 11 October, preserving newer values. Their machines are unidentified; recovered Rust WebAssembly timings predate the explicit stack budget.

The following **3 October sample** reports mean ± standard deviation in milliseconds. It is a dated comparison, not a copy of the full table's latest values:

| Case | Osprey | Osprey ARC | Osprey GC | Rust | C |
| --- | ---: | ---: | ---: | ---: | ---: |
| fib | 64.0 ± 2.4 | 64.1 ± 1.9 | 68.5 ± 7.3 | 29.2 ± 2.5 | 28.1 ± 2.5 |
| binarytrees | 612.6 ± 14.0 | 479.0 ± 18.8 | 2362.7 ± 352.0 | 1967.2 ± 685.5 | 891.5 ± 175.1 |
| wordfreq | 112.6 ± 48.4 | 62.3 ± 4.2 | 172.9 ± 28.9 | 9.1 ± 3.1 | 1.9 ± 2.8 |

`wordfreq` shows the cost of Osprey's persistent map compared with mutable hash tables. The tree result favors ARC in this sample, but the baseline variance limits speedup claims.

## Reproduce

```bash
make -C benchmarks bench-osprey                # refresh all Osprey backends only
BENCH_FILTER=wordfreq make -C benchmarks bench # one workload across installed toolchains
```

[`benchmarks/report.py`](https://github.com/Nimblesite/osprey/blob/main/benchmarks/report.py) generates the website tables and `benchmarks/results/results.html` from `results.json`. Partial reruns preserve older values for skipped, unavailable or failed cases. Record the date, machine, toolchain versions, selected languages and variance when sharing a comparison.
