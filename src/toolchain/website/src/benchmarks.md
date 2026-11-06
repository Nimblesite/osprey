---
layout: page.njk
wide: true
title: Benchmarks
description: Elapsed time and peak memory for Osprey, Rust, C, C#, Dart, Codon, OCaml and Haskell on 24 small compute programs, with the machine and toolchain behind every figure.
date: "git Last Modified"
tags: ["benchmarks", "performance"]
author: "Christian Findlay"
---

Elapsed time and peak resident memory for 24 small programs, each written with the same algorithm in every language. A program must print its expected output before it is timed. The [methodology](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/README.md#methodology), [sources](https://github.com/Nimblesite/osprey/tree/main/src/toolchain/benchmarks/cases), [harness](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/run.sh) and [raw data](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/results/results.json) are open.

| Figure | What it is |
| --- | --- |
| CPU time | Mean wall-clock time over at least 10 runs after 3 warm-up runs (`hyperfine -N`), including process start-up and excluding compilation. The small figure below it is the standard deviation. Hover a cell for the median, minimum, maximum and run. |
| Peak memory | Highest resident set size over three runs. `—` means not measured: a WebAssembly run's RSS is the Wasmtime host's. |
| ★ and bold | ★ marks the lowest time in a row when it is an Osprey column; bold marks the lowest otherwise. |

Figures come from the runs listed in the last table, on different dates and machines.

{% include "benchmarks-tables.html" %}

## Reproduce

```bash
make -C src/toolchain/benchmarks bench-osprey                # re-time the Osprey columns only
BENCH_FILTER=wordfreq make -C src/toolchain/benchmarks bench # one program across installed toolchains
```

Each run replaces only the figures it measured and records its machine and toolchain versions.
