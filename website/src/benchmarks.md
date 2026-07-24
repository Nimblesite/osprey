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
- **`+ - *` no longer allocate; `/` and `%` still do.** Osprey used to wrap
  every arithmetic operator in a `Result`, heap-allocating a
  `{ payload, discriminant, errmsg }` struct per operation that the consumer
  immediately unwrapped — overhead for a guarantee that was never enforced.
  [ARITH-PLAIN](/spec/0013-errorhandling/) has since landed
  ([plan 0019](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0019-ml-elegance.md)):
  `+ - *` return plain scalars, and the overflow guarantee is now real and
  opt-in through `checkedAdd`/`checkedSub`/`checkedMul`. `/` and `%` keep
  `Result` because zero has no representable quotient, and `%` gained the zero
  check it never had — `10 % 0` is an `Error`, not undefined.
  This is legible straight down the memory column: every case whose inner loop
  is `+ - *` only — `fib`, `pascal`, `hanoi`, `tak`, `coins`, `ackermann` —
  now sits at C's ~1.5 MB. Every case still far above it either builds real
  heap data or runs `/`, `%`, or `intDiv` in its hot loop, which is one
  allocation per operation.
- **Comparing against Rust's `-C overflow-checks=off` is not a
  safety-versus-speed trade.** Neither side is checking overflow; Osprey's
  checked operators are opt-in and are not what these cases call.
- **Osprey loops via `range |> fold`,** not deep linear recursion, because it has
  no tail-call optimization yet (a 1e6-deep recursion overflows the stack). The
  work is identical; only the iteration mechanism differs.
- **OCaml is built without flambda** (stock `ocamlopt`), so its numbers are
  conservative versus an flambda build.
- **Single machine, wall clock.** Treat ratios as indicative; re-run locally with
  `make bench`. The exact set of outright wins shifts run-to-run because Osprey,
  Rust, and C now sit within measurement noise of one another.

## Where the gap remains

Both axes tell the same story, and it is an allocation story.

**CPU.** Osprey is at parity with C on every case whose inner loop is `+ - *`
and calls — `coins`, `fib`, `hanoi`, `pascal`, `tak`, `ackermann`, `mutual` —
and beats C on `binarytrees`. It is behind everywhere a `/`, `%`, `intDiv`, or a
real heap structure dominates the loop, because each of those is an allocation
C does not make.

**Memory.** On the default backend the peak tracks that same allocation count:
the `+ - *` cases sit at C's ~1.5 MB, and the rest climb with the number of
`Result` nodes they build and never free. The default allocator does not reclaim
during a run.

**That is a backend choice, not a language one.** Allocation funnels through the
one swappable boundary of the
[Memory Management spec](/spec/0018-memorymanagement/), and under
`--memory=arc` (Perceus reference counting) **every case drops to 1.5–3.5 MB** —
matching C throughout, beating it on `exprtree` — with no change to a line of
Osprey source and, on the allocation-heavy cases, slightly *faster* wall clock.
`--memory=gc` offers the same trade with a tracing collector.

## Reproduce it

```bash
make bench                       # build everything, run the whole suite
BENCH_FILTER=fib make bench      # only cases whose name contains "fib"
```

Results land in `benchmarks/results/` — `results.html` (this report, standalone),
`results.json` (structured), and the per-case `hyperfine` exports.
