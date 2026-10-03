---
layout: page.njk
title: Benchmarks
description: How Osprey's CPU time and peak memory compare to Rust, C, C#, Dart, OCaml, and Haskell on classic compute benchmarks.
date: "git Last Modified"
tags: ["benchmarks", "performance"]
author: "Christian Findlay"
---

Osprey compiles through LLVM to a native binary, so the fair question is how it
sits against other native-compiled languages. This page measures **CPU time** and
**peak memory** against **Rust, C, C#, Dart, OCaml, and Haskell** on classic compute
benchmarks — the same naive algorithm, the same parameters, in every language.

The tables below are generated **mechanically** from the benchmark harness output
by [`benchmarks/report.py`](https://github.com/Nimblesite/osprey/blob/main/benchmarks/report.py)
— never hand-edited. The Osprey column is highlighted; the fastest cell in each
row is emphasised, and **★ marks a benchmark Osprey wins outright** (strictly
faster, or lighter, than every other language).

{% include "benchmarks-tables.html" %}

## Methodology

Every benchmark is implemented identically in every language under
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
| C#       | `dotnet publish -c Release` (AOT) |
| Dart     | `dart compile exe` |
| OCaml    | `ocamlopt -O3 -unsafe` |
| Haskell  | `ghc -O2` |

## Reading the numbers fairly

- **Same algorithm everywhere.** Identical *naive* algorithm and parameters in
  every language — no memoization, closed forms, SIMD, or parallelism. We measure
  the language/compiler/runtime, not who is cleverest. Ranges match Osprey's
  half-open `range(a, b)` = `[a, b)` exactly.
- **Integer arithmetic is checked.** Osprey's integer `+ - *`, unary `-`, and
  `abs` return a plain `int`. Overflow is detected without trapping and sent to
  the `Arith` handler the program installed, which chooses the result; a zero
  divisor for `/` or `%` goes to the same handler. Arithmetic never silently
  wraps or panics, and a program that can fail without a handler does not
  compile. `checkedAdd`/`checkedSub`/`checkedMul` return `Result<int, Error>`
  for code that wants overflow as data. See
  [Arithmetic Effects](/spec/0037-arithmeticeffects/).
- **The Rust command disables Rust's overflow checks.** The comparison is
  deliberately asymmetric: Osprey always checks for overflow while this Rust
  configuration measures wrapping release arithmetic.
- **These numbers predate the `Arith` handler model.** They were measured when
  integer `+ - *` returned a heap-allocated `Result<int, MathError>`, and have
  not yet been re-measured.
- **Osprey loops via `range |> fold`,** not deep linear recursion, because it has
  no tail-call optimization yet (a 1e6-deep recursion overflows the stack). The
  work is identical; only the iteration mechanism differs.
- **OCaml is built without flambda** (stock `ocamlopt`), so its numbers are
  conservative versus an flambda build.
- **Single machine, wall clock.** Treat ratios as indicative; re-run locally with
  `make bench`.

## Where the gap remains

Osprey is not the fastest language in this table on any case. Averaged across
the suite it runs **11.6× Rust's CPU time and 13.1× C's**, and the default
memory backend never wins a row either — `binarytrees` peaks at **1.77 GB**
against C's 1.75 MB.

The CPU gap is not attributable to `/` and `%` alone. When these numbers were
measured, integer `+ - *` also produced an explicit `Result<int, MathError>`, so
every arithmetic-heavy row carried that cost: `fn addup(a, b) = a + b` returned
a heap-allocated Result, not an `i64`. Under
[Arithmetic Effects](/spec/0037-arithmeticeffects/) it returns an `i64` and
reports overflow on a separate, rarely taken branch; how much of the gap that
closes is unmeasured until the suite is re-run.

**The memory gap is a backend choice, not a language one.** Allocation funnels
through the one swappable boundary of the
[Memory Management spec](/spec/0018-memorymanagement/), and under
`--memory=arc` (Perceus reference counting) **every case drops to 1.5–3.4 MB**,
with no change to a line of Osprey source. That lands ARC within striking
distance of C — a median of **1.06× C's peak RSS**, from 0.30× on `exprtree`
(where it beats C outright) to 1.65× on `binarytrees` — turning a 1000×
memory deficit into a rounding error.

Wall clock cuts both ways, and the split is the opposite of what you might
expect. On the 17 allocation-light cases ARC is slightly *faster* than the
non-reclaiming default (up to 6% on `ackermann`, `coins`, `mutual`), because
freeing early keeps the working set in cache. On the 5 allocation-heavy cases it
is *slower* — `binarytrees` by 21%, `wordfreq` by 17%, `exprtree` and `listops`
by 8% — which is the refcount traffic those cases exist to provoke. Reclaiming
memory is a real trade, not a free win. `--memory=gc` offers the same trade with
a tracing collector.

## Reproduce it

```bash
make bench                       # build everything, run the whole suite
BENCH_FILTER=fib make bench      # only cases whose name contains "fib"
```

Results land in `benchmarks/results/` — `results.html` (this report, standalone),
`results.json` (structured), and the per-case `hyperfine` exports.
