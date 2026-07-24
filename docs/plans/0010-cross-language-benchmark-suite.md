# Plan 0010 — Cross-Language Benchmark Suite

**Subsystem:** `benchmarks/` (harness + cases), `Makefile` (`make bench`),
`.devcontainer` (comparison toolchains)
**Status:** Suite shipped (22 cases × 7 languages — Osprey, Rust, C, C#, Dart,
OCaml, Haskell — plus `osprey-wasm`/`rust-wasm` and the `osprey-arc`/`osprey-gc`
backend columns); `intDiv` added; **native codegen optimized (`-O2`) and
allocation routed through a swappable `@osp_alloc` backend, with two reclaiming
backends shipped**. Osprey is at parity or faster than C/Rust on most cases.
`binarytrees` now lags **only under the DEFAULT (non-reclaiming) backend**
(633 MB peak RSS): with the opt-in `--memory=arc` it peaks at **2.97 MB** — 213×
less than the default, 1.30× Rust's 2.28 MB and 1.71× C's 1.74 MB — and it is
*faster* than the default (0.216 s vs 0.249 s);
`--memory=gc` peaks at 18.5 MB. Because both reclaiming backends are **opt-in
flags, not the default**, the headline `osprey` column in the published tables
still shows the 633 MB figure. Feature-blocked classics (arrays, float) pending
**Spec ID:** `[BENCH-SUITE]`

## Summary

An **accurate, reproducible** way to see where Osprey sits on CPU time and peak
memory against **Rust, C, C#, Dart, OCaml, and Haskell**. Every benchmark is
implemented in all seven languages with the *same naive algorithm and
parameters*, compiled to a native binary, checked byte-for-byte against an
integer oracle (`expected.txt`), then timed with `hyperfine` (CPU) and
`/usr/bin/time` (peak RSS). All source lives **in-tree and version-controlled**
under `benchmarks/cases/<name>/` — `<name>.{osp,rs,c,cs,dart,ml,hs}` +
`expected.txt` + `bench.json`. Only build/run *output* (`benchmarks/results/`)
is gitignored. `results.json` additionally carries the `osprey-arc`,
`osprey-gc`, `osprey-wasm` and `rust-wasm` columns.

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

Three changes closed the gap:

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
   `heap_alloc` / `OSP_ALLOC_DECL`; the layout-carrying twins
   `OSP_ALLOC_TAGGED_DECL` / `OSP_ALLOC_TAGGED_NOINIT_DECL` follow the same
   shape). The IR names no allocator, so the backend swaps in **at link time**
   per [MEM-BACKENDS]. Allocator attributes keep the `-O2` elimination intact.

3. **Three backends behind that one hook**, selected by `--memory=` and resolved
   by swapping `libfiber_runtime{,_gc,_arc}.a` at link time
   ([`parse_memory` in crates/osprey-cli/src/main.rs](../../crates/osprey-cli/src/main.rs),
   accepting `default | gc | arc`):
   [memory_runtime.c](../../compiler/runtime/memory_runtime.c) (`malloc`
   passthrough, the default), [memory_gc.c](../../compiler/runtime/memory_gc.c)
   (conservative mark & sweep), [memory_arc.c](../../compiler/runtime/memory_arc.c)
   (Perceus reference counting).

`binarytrees` is the one case where the *default* backend still trails: its tree
nodes genuinely *escape*, so `-O2` cannot statically free them and a `malloc`
passthrough never reclaims. Both reclaiming backends fix it outright — measured
in [benchmarks/results/results.json](../../benchmarks/results/results.json) and
reproducible with `./target/release/osprey benchmarks/cases/binarytrees/binarytrees.osp --run --memory=arc`:

| backend | peak RSS | mean wall | checksum |
|---------|----------|-----------|----------|
| default (`malloc`) | 633 MB | 0.249 s | 19659600 (correct) |
| `--memory=arc` | **2.97 MB** (213× less) | **0.216 s** (faster than default) | 19659600 (correct) |
| `--memory=gc` | 18.5 MB | 1.331 s | 19659600 (correct) |

For reference C peaks at 1.74 MB and Rust at 2.28 MB on this case, so ARC sits at
1.71× C and 1.30× Rust — and beats every managed runtime measured (C# 16.8 MB,
Haskell 11.6 MB, OCaml 5.37 MB, Dart 23.6 MB). `OSPREY_ARC_DEBUG=1` on the same run reports
`[osp-arc] exit: 0 live objects, 0 KiB (+0 immortal)` — zero leaked language
values. See [spec 0018 — Memory Management](../specs/0018-MemoryManagement.md).

The caveat is exposure, not correctness: ARC/GC are **opt-in flags**, so the
default `osprey` column of the published benchmark tables is still the 633 MB
one. Making a reclaiming backend the default is a spec-0018 decision, not a
benchmark-suite one.

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
- [x] 22 cases × 7 languages (`.osp/.rs/.c/.cs/.dart/.ml/.hs`), all source
      version-controlled under `cases/`.
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
- [x] **Reclaiming backends behind `@osp_alloc`** — two of them ship, selected by
      `--memory=` (`parse_memory` accepts `default | gc | arc`) and linked by
      archive swap (`libfiber_runtime{,_gc,_arc}.a`): `memory_gc.c` (conservative
      mark & sweep) and `memory_arc.c` (Perceus refcounting), per
      [spec 0018 — Memory Management](../specs/0018-MemoryManagement.md)
      ([MEM-BACKENDS], [GC-ARC-PERCEUS]). `binarytrees` is fixed: 633 MB →
      **2.97 MB** under `--memory=arc` (213× less, and *faster* than the default:
      0.216 s vs 0.249 s) and 18.5 MB under `--memory=gc`, checksum `19659600`
      identical on all three. `OSPREY_ARC_DEBUG=1` reports **0 live objects** at
      exit. Unit-tested in `make test` via `_test_c_runtime`
      (`memory_arc_tests` + `memory_gc_tests`) and `parse_memory` flag tests.
      **Still opt-in** — the default backend, and therefore the headline
      benchmark column, is unchanged.
- [ ] **Wire the per-backend conformance oracles into CI.** `_conformance-gc` and
      `_conformance-arc` (Makefile) re-run the whole differential harness under
      `--memory=gc` / `--memory=arc` and assert byte-identical output, but
      **neither is a prerequisite of `test:` or `ci:`, and `.github/workflows/ci.yml`
      never invokes them** (`ci: lint test bank-test bank-e2e build`; `test:` runs
      `_test_rust`, `_coverage_check_rust`, `_test_c_runtime`, `_test_differential`,
      `_test_profiler`, `_test_vscode_extension`, `_coverage_check_vscode_extension`).
      What CI actually enforces for the backends today is the C unit suites plus
      the `parse_memory` unit tests — not end-to-end backend equivalence. Add both
      targets to the CI job (or to `test:`) so a backend that silently diverges
      fails the build.
- [ ] **Make the ARC zero-leak bar actually enforced.** `crates/diff_examples.sh`
      only computes and prints `ARC_LEAKY` when `OSPREY_ARC_DEBUG` is set
      (`if [[ -n "${OSPREY_ARC_DEBUG:-}" ]]`), **no make target sets that variable**,
      and `_test_differential` greps only for `FAIL=0`, `NOEXP=0` and `FC_OK` — so
      even with the variable set nothing asserts `ARC_LEAKY=0`. The leak bar is
      therefore doubly opt-in, which contradicts
      [docs/specs/0018-MemoryManagement.md](../specs/0018-MemoryManagement.md)
      (“enforced on every run by the harness (`ARC_LEAKY` must be 0)” … “machine-checked
      on every run”). Fix by exporting `OSPREY_ARC_DEBUG=1` in `_conformance-arc`
      and adding an `ARC_LEAKY=0` grep, then correcting spec 0018 if the scope
      differs.
- [ ] **Refresh the stale ARC figure in `benchmarks/README.md`** — it quotes
      binarytrees ARC peak RSS as “~4.9 MB”, but the committed measurement in
      `benchmarks/results/results.json` is 2 965 504 B ≈ **2.97 MB**. (The same
      README's “905 MB” default figure is likewise ahead of the committed 633 MB.)
      Regenerate rather than hand-edit.
- [ ] (Later) mutable arrays → sieve, matrix-multiply, sort/fannkuch/n-queens.
- [ ] (Later) `int`↔`float` + `sqrt` → mandelbrot, n-body, spectral-norm.
