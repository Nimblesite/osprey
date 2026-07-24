---
layout: page.njk
title: Benchmarks
description: How Osprey's CPU time and peak memory compare to Rust, C, OCaml, and Haskell on classic compute benchmarks.
date: "git Last Modified"
tags: ["benchmarks", "performance"]
author: "Christian Findlay"
---

Osprey compiles through LLVM to a native binary, so the fair question is how it
sits against other native-compiled languages. This page measures **CPU time** and
**peak memory** against **Rust, C, OCaml, and Haskell** on classic compute
benchmarks — the same naive algorithm, the same parameters, in every language.

The tables below are generated **mechanically** from the benchmark harness output
by [`benchmarks/report.py`](https://github.com/Nimblesite/osprey/blob/main/benchmarks/report.py)
— never hand-edited. The Osprey column is highlighted; the fastest cell in each
row is emphasised, and **★ marks a benchmark Osprey wins outright** (strictly
faster, or lighter, than every other language).

{% include "benchmarks-tables.html" %}

## Methodology

Every benchmark is implemented identically in all five languages under
[`benchmarks/cases/<name>/`](https://github.com/Nimblesite/osprey/tree/main/benchmarks/cases),
compiled to a native binary, checked for correct output, then timed.

1. **Build once, time the binary.** `osprey … --compile` emits a persistent
   native executable; we time *that*, never `--run` (which would fold compile and
   link into the measurement). Every language uses its standard optimizing
   release flags.
2. **Correctness oracle.** Each binary runs once and its output is compared to the
   case's `expected.txt`. A mismatch or build failure is excluded from timing — we
   never publish a number for a program that computed the wrong thing. Every case
   has a single deterministic **integer** result, so output is byte-comparable
   across languages.
3. **CPU.** [`hyperfine`](https://github.com/sharkdp/hyperfine) `-N --warmup 3
   --min-runs 10` per case → statistical mean ± standard deviation.
4. **Memory.** `/usr/bin/time` peak resident set size (`-l` on macOS, `-v` on
   Linux), max over a few runs.

### Compile commands

| Language | Command |
|----------|---------|
| Osprey   | `osprey <f>.osp --compile` (LLVM IR → clang `-O2`; override with `OSPREY_OPT`) |
| Rust     | `rustc -C opt-level=3 -C overflow-checks=off` |
| C        | `cc -O2` |
| OCaml    | `ocamlopt -O3 -unsafe` |
| Haskell  | `ghc -O2` |

## Reading the numbers fairly

- **Same algorithm everywhere.** Identical *naive* algorithm and parameters in
  every language — no memoization, closed forms, SIMD, or parallelism. We measure
  the language/compiler/runtime, not who is cleverest. Ranges match Osprey's
  half-open `range(a, b)` = `[a, b)` exactly.
- **Osprey wraps every `+ - * %` in a `Result`,** and that wrapper — not a safety
  check — is a real part of any Osprey gap. Each operation emits its `add i64`
  and then heap-allocates a `{ payload, discriminant, errmsg }` struct which the
  consumer immediately unwraps. No overflow check is performed today: `MAX + 1`
  wraps silently and `10 % 0` is undefined. So the cost is allocation overhead
  for a guarantee that is not yet enforced, and comparing it against Rust's
  `-C overflow-checks=off` is not a safety-versus-speed trade. [ARITH-PLAIN]
  ([plan 0019](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0019-ml-elegance.md))
  removes the wrapper from `+ - *` and makes the overflow guarantee real and
  opt-in via `checkedAdd`/`checkedSub`/`checkedMul`; these numbers should be
  re-measured after it lands.
- **Osprey loops via `range |> fold`,** not deep linear recursion, because it has
  no tail-call optimization yet (a 1e6-deep recursion overflows the stack). The
  work is identical; only the iteration mechanism differs.
- **OCaml is built without flambda** (stock `ocamlopt`), so its numbers are
  conservative versus an flambda build.
- **Single machine, wall clock.** Treat ratios as indicative; re-run locally with
  `make bench`. The exact set of outright wins shifts run-to-run because Osprey,
  Rust, and C now sit within measurement noise of one another.

## Where the gap remains: memory

On compute, Osprey is at parity with C and Rust and ahead of OCaml and Haskell.
Peak memory matches C on every case **except `binarytrees`**. That benchmark
builds, holds, and checksums millions of small heap nodes — they genuinely
*escape*, so the optimizer cannot statically free them, and Osprey's default
allocator does not reclaim memory during a run yet.

This is the contract of the [Memory Management spec](/spec/0018-memorymanagement/):
allocation funnels through one swappable backend boundary, so a reclaiming
manager (reference counting, a tracing collector, or an arena) can be linked in
to close this last gap without changing a line of Osprey source.

## Reproduce it

```bash
make bench                       # build everything, run the whole suite
BENCH_FILTER=fib make bench      # only cases whose name contains "fib"
```

Results land in `benchmarks/results/` — `results.html` (this report, standalone),
`results.json` (structured), and the per-case `hyperfine` exports.
