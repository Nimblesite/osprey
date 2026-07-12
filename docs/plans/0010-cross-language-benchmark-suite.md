# Plan 0010 — Cross-Language Benchmark Suite

**Subsystem:** `benchmarks/` (harness + cases), `Makefile` (`make bench`),
`.devcontainer` (comparison toolchains)
**Status:** Suite shipped (22 cases × 5 languages); `intDiv` added; **native codegen now optimized (`-O2`) and allocation routed through a swappable backend** — Osprey is fastest of all five on 7 cases and at parity on most others; only `binarytrees` (escaping allocations) still lags. Feature-blocked classics (arrays, float) pending
**Spec ID:** `[BENCH-SUITE]`

## Summary

An **accurate, reproducible** way to see where Osprey sits on CPU time and peak
memory against **Rust, C, OCaml, and Haskell**. Every benchmark is implemented
in all five languages with the *same naive algorithm and parameters*, compiled
to a native binary, checked byte-for-byte against an integer oracle
(`expected.txt`), then timed with `hyperfine` (CPU) and `/usr/bin/time` (peak
RSS). All source lives **in-tree and version-controlled** under
`benchmarks/cases/<name>/` — `<name>.{osp,rs,c,ml,hs}` + `expected.txt` +
`bench.json`. Only build/run *output* (`benchmarks/results/`) is gitignored.

## What works today (22 cases)

**Recursion-bound:** `fib`, `ackermann`, `tak`, `hanoi`, `pascal`, `coins`, `mutual`, `exprtree`
**Iteration / number theory:** `primes`, `gcdsum`, `nestedloop`, `factorial`, `powmod`, `josephus`, `coprime`, `listops`
**Integer division (`intDiv`):** `collatz`, `digitsum`, `isqrt`
**String / map:** `textstats`, `wordfreq`
**Allocation / memory:** `binarytrees`

Harness: [benchmarks/run.sh](../../benchmarks/run.sh) (toolchain detection,
build-once, correctness oracle, CPU + memory measurement) →
[benchmarks/report.py](../../benchmarks/report.py) (CPU table, relative-speed
table, peak-memory table, geomean Osprey-slowdown). `make bench` /
`BENCH_FILTER=<name> make bench`. Dev container installs `ghc ocaml time` +
hyperfine.

## Key finding — and the fix

The suite's first run exposed a catastrophe: Osprey was 12–89× slower and used
120–2244× more peak RSS, with memory scaling by **operation count** (`fib(35)` ≈
1.4 GB). Root cause was **not** the language — it was the build pipeline: codegen
handed its LLVM IR to `clang` with **no optimization flag (`-O0`)**. Every
per-operation `Result` block stayed a live `malloc`.

Two changes closed almost the entire gap:

1. **Optimize the native build (`-O2`, overridable via `OSPREY_OPT`)** —
   [crates/osprey-cli/src/main.rs](../../crates/osprey-cli/src/main.rs)
   `opt_flag()`. LLVM proves the per-operation `Result` allocations non-escaping
   and removes them entirely (heap → registers) — the [MEM-OWNERSHIP] static
   free-at-last-use, done by the optimizer. fib(35): **0.52 s → 0.01 s** and
   **1.37 GB → 1.4 MB**. Across the suite Osprey went from ~12–19× slower to
   **parity with C/Rust and faster than OCaml/Haskell**, winning outright on
   `digitsum`, `mutual`, `josephus`, `pascal`, `primes`, `gcdsum`, `tak`.

2. **Swappable allocation backend** — all codegen heap allocation now funnels
   through one `@osp_alloc` hook ([builder.rs](../../crates/osprey-codegen/src/builder.rs)
   `heap_alloc` / `OSP_ALLOC_DECL`), implemented by
   [compiler/runtime/memory_runtime.c](../../compiler/runtime/memory_runtime.c)
   (default = `malloc` passthrough). The IR names no allocator, so ARC / tracing
   GC / arena swap in at link time per [MEM-BACKENDS]. Allocator attributes keep
   the `-O2` elimination intact.

The one remaining gap is `binarytrees` (905 MB): its tree nodes genuinely
*escape*, so the optimizer cannot statically free them and the default backend
does not reclaim. That is the case a real reclaiming backend (now unblocked by
the `@osp_alloc` boundary) must fix. See
[spec 0018 — Memory Management](../specs/0018-MemoryManagement.md).

## Blocked classics — need Osprey language features (not faked)

Each row is a benchmark we *cannot* express today. Ordered by leverage: the
feature that unblocks the most classics, with the least scope, first.

| Missing feature | Unblocks | Scope |
|-----------------|----------|-------|
| **Integer division `/` (+ already-present `%`)** | collatz, digit-sum, integer-sqrt, sieve-of-eratosthenes (index math), radix benchmarks | **Low** — new typed op, no new types |
| Mutable arrays / fixed-size buffers | sieve, matrix-multiply, quicksort, mergesort, fannkuch, n-queens | High — new aggregate type + codegen |
| `int`↔`float` conversion + `sqrt`/trig stdlib | mandelbrot, n-body, spectral-norm | Medium — math runtime + exact float oracle |
| List literals / cons / list pattern-matching ops | list-heavy classics already partly covered by `[h, ...t]` | Medium |
| Arbitrary-precision integers | pidigits, big-factorial (exact) | High |

**Decision:** integer division is the next feature to add — lowest scope,
unblocks the most classic integer benchmarks, and `%` already exists so the
codegen/runtime path is half-built.

## Implementation plan (next feature: integer division)

1. Find how `%` is lowered (`crates/osprey-codegen`) and the float `/` path; add
   integer `/` as a typed, overflow/zero-checked op returning
   `Result<int, MathError>` (divide-by-zero → `MathError`), mirroring `%`.
2. Type rule in `crates/osprey-types`: `int / int -> Result<int, MathError>`;
   keep `float / float -> float` unchanged (type-directed dispatch).
3. Whole-body arithmetic helpers already auto-unwrap `Result` — confirm `/`
   composes (e.g. `fn idiv(a: int, b: int) -> int = a / b`).
4. `find-similar` before adding any runtime helper; reuse the `%` path.
5. Add `collatz`, `digit-sum`, `integer-sqrt` benchmark cases (all 5 languages).
6. `make ci` green; refresh `benchmarks/results/`.

## TODO

- [x] Harness: build-once, correctness oracle, CPU (hyperfine) + peak RSS.
- [x] `report.py`: CPU + peak-memory tables, geomean-vs-each-language summary cards,
      fastest-cell badging + Osprey-win stars.
- [x] 22 cases × 5 languages, all source version-controlled under `cases/`.
- [x] `make bench` target + `BENCH_FILTER`; `.gitignore` tracks source, ignores `results/`.
- [x] Dev container: `ghc`, `ocaml`, `time`, hyperfine.
- [x] README documents methodology, fairness caveats, the memory finding.
- [x] Run full suite end-to-end → publish numbers in README findings.
- [x] `report.py` renders a self-contained **HTML** report (`results.html`, Osprey
      website CSS) and bakes the tables into the website `/benchmarks` page +
      methodology; `results.md` retired. Generated mechanically — never hand-edited.
- [x] **Add integer division** as the `intDiv` builtin (`/` stays float-only per
      spec) — codegen + types + `[BUILTIN-INTDIV]` spec + tested example.
- [x] Add `collatz`, `digitsum`, `isqrt` cases (all 5 languages, verified vs C oracle).
- [x] **Optimize the native build** (`-O2` via `opt_flag()`, `OSPREY_OPT`
      override) — the single change that took Osprey from 12–89× slower to
      parity/winning, and collapsed per-operation RSS (fib 1.37 GB → 1.4 MB).
- [x] **Swappable allocation backend** — codegen emits `@osp_alloc` (attributed
      so `-O2` still elides non-escaping allocs); default backend
      `memory_runtime.c` = `malloc`. Implements [MEM-BACKENDS]; not tied to malloc.
- [ ] (Next) a reclaiming backend behind `@osp_alloc` (ARC / arena / tracing GC
      per spec 0018) so **escaping** allocations are freed → fixes `binarytrees`,
      the last benchmark where Osprey trails.
- [ ] (Later) mutable arrays → sieve, matrix-multiply, sort/fannkuch/n-queens.
- [ ] (Later) `int`↔`float` + `sqrt` → mandelbrot, n-body, spectral-norm.
