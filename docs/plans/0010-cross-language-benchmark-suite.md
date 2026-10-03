# Plan 0010 — Cross-Language Benchmark Suite

**Subsystem:** `benchmarks/` (harness + cases), `Makefile` (`make bench`),
`.devcontainer` (comparison toolchains)
**Status:** Suite shipped (22 cases × 7 languages — Osprey, Rust, C, C#, Dart,
OCaml, Haskell — plus `osprey-wasm`/`rust-wasm` and the `osprey-arc`/`osprey-gc`
backend columns); `intDiv` added; **native codegen optimized (`-O2`) and
allocation routed through a swappable `@osp_alloc` backend, with two reclaiming
backends shipped**. On 2026-10-03, all 22 cases passed under all four Osprey backends on the Ryzen 3900X/WSL2 host. `binarytrees` peaked at **631 MB** with the default allocator, **3.26 MB** with ARC, and **14.9 MB** with GC. C/Rust were remeasured only on three cases; other language records are historical. Machine details, timing variance and comparison limits are in [the benchmark report](../../website/src/benchmarks.md). Feature-blocked array and
floating-point cases remain pending, as does arbitrary-precision `pidigits`.
quicksort and mergesort are **no longer blocked** — the list-literal layout
defect that crashed them is fixed; only the cases themselves are unwritten.

**Update — the backend oracles are now enforced.** `make test` runs the
differential harness three times (default, `--memory=gc`, `--memory=arc`) and
the ARC pass fails unless every example ends with `ARC_LEAKY=0`. What remains on
this plan is documentation freshness and the blocked cases below.

## Summary

The suite measures Osprey CPU time and peak memory against **Rust, C, C#, Dart,
OCaml, and Haskell**. Every benchmark is
implemented in all seven languages with the *same naive algorithm and
parameters*, compiled to a native binary, checked byte-for-byte against an
integer oracle (`expected.txt`), then timed with `hyperfine` (CPU) and
`/usr/bin/time` (peak RSS). All source lives **in-tree and version-controlled**
under `benchmarks/cases/<name>/` — `<name>.{osp,rs,c,cs,dart,ml,hs}` +
`expected.txt` + `bench.json`. The measured **outputs are tracked too** —
`benchmarks/results/{raw.jsonl,results.json,results.html}` are in git; only the
per-case binaries (`results/bin/`) and raw hyperfine exports (`results/hf/`) are
gitignored. That is what makes every figure quoted in this plan checkable against
a committed file. `results.json` additionally carries the `osprey-arc`,
`osprey-gc` and `rust-wasm` columns; the last published run has **no**
`osprey-wasm` column, because the wasm runtime archive was absent when it ran.

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

## Initial performance finding and fix

The initial run measured Osprey at 12–89× slower and 120–2244× more peak RSS,
with memory scaling by **operation count** (`fib(35)` ≈ 1.4 GB). The build
pipeline handed codegen's LLVM IR to `clang` with **no optimization flag
(`-O0`)**. Every
per-operation `Result` block stayed a live `malloc`.

Three changes addressed these measurements:

1. **Optimize the native build (`-O2`, overridable via `OSPREY_OPT`)** —
   [crates/osprey-cli/src/main.rs](../../crates/osprey-cli/src/main.rs)
   `opt_flag()`. LLVM proves the per-operation `Result` allocations non-escaping
   and removes them entirely (heap → registers) — the [MEM-OWNERSHIP] static
   free-at-last-use, done by the optimizer. fib(35): **0.52 s → 0.01 s** and
   **1.37 GB → 1.4 MB**.

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
   passthrough that additionally frees values codegen proves uniquely owned —
   see spec 0018; it is no longer a pure no-reclamation backend),
   [memory_gc.c](../../compiler/runtime/memory_gc.c)
   (conservative mark & sweep), [memory_arc.c](../../compiler/runtime/memory_arc.c)
   (Perceus reference counting).

`binarytrees` remains memory-heavy under the *default* backend: its tree
nodes genuinely *escape*, so `-O2` cannot statically free them and a `malloc`
passthrough never reclaims them. Measurements for all three backends are in
[benchmarks/results/results.json](../../benchmarks/results/results.json) and can
be reproduced with `./target/release/osprey benchmarks/cases/binarytrees/binarytrees.osp --run --memory=arc`:

Measurements below are from the 2026-10-03 refresh (decimal MB; elapsed time is mean ± standard deviation). They replace the older figures previously quoted by this plan.

| backend | peak RSS | elapsed time | checksum |
| --- | --- | --- | --- |
| default | 630.83 MB | 0.613 ± 0.014 s | 19659600 |
| `--memory=arc` | 3.26 MB | 0.479 ± 0.019 s | 19659600 |
| `--memory=gc` | 14.94 MB | 2.363 ± 0.352 s | 19659600 |

ARC and GC remain opt-in. The native conformance suite requires zero live ARC objects at exit; all 217 programs passed that gate during arithmetic-effects finalization. This shared host produced timing outliers. Do not compare these measurements with historical records from an unidentified machine or infer a general speedup from them.

## Blocked benchmark families

Each row records benchmarks that cannot be expressed with the current language
surface.

| Missing feature | Unblocks | Scope |
|-----------------|----------|-------|
| Mutable arrays / fixed-size buffers | sieve, matrix-multiply, n-sieve, fannkuch, n-queens | High — new aggregate type + codegen |
| `int`↔`float` conversion + `sqrt`/trig stdlib | mandelbrot, n-body, spectral-norm | Medium — math runtime + exact float oracle |
| ~~Recursive persistent-List `filter`+concat codegen defect~~ **fixed** | quicksort, mergesort — now writable | None; write the two cases |
| Iterator→`List` materialization (`filter` returns `Iterator<T>`, nothing collects it back) | a `filter`-shaped quicksort; the head/tail-recursive shape works today | Medium — one builtin + spec anchor |
| Arbitrary-precision integers | pidigits | High — new numeric representation |

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
- [x] Add `collatz`, `digitsum`, `isqrt` cases (all 7 languages, verified vs C oracle).
- [x] **Optimize the native build** (`-O2` via `opt_flag()`, `OSPREY_OPT`
      override) — reduced fib RSS from 1.37 GB to 1.4 MB and runtime from
      0.52 s to 0.01 s.
- [x] **Swappable allocation backend** — codegen emits `@osp_alloc` (attributed
      so `-O2` still elides non-escaping allocs); default backend
      `memory_runtime.c` = `malloc`. Implements [MEM-BACKENDS]; not tied to malloc.
- [x] **Reclaiming backends behind `@osp_alloc`** — two of them ship, selected by
      `--memory=` (`parse_memory` accepts `default | gc | arc`) and linked by
      archive swap (`libfiber_runtime{,_gc,_arc}.a`): `memory_gc.c` (conservative
      mark & sweep) and `memory_arc.c` (Perceus refcounting), per
      [spec 0018 — Memory Management](../specs/0018-MemoryManagement.md)
      ([MEM-BACKENDS], [GC-ARC-PERCEUS]). `binarytrees` peaks at 631 MB by default,
      **3.26 MB** under ARC and 14.9 MB under GC in the current run, with checksum `19659600`
      identical on all three. `OSPREY_ARC_DEBUG=1` reports **0 live objects** at
      exit. Unit-tested in `make test` via `_test_c_runtime`
      (`memory_arc_tests` + `memory_gc_tests`) and `parse_memory` flag tests.
      **Still opt-in** — the default backend, and therefore the headline
      benchmark column, is unchanged.
- [x] **Wire the per-backend conformance oracles into CI.** `_conformance-gc`
      and `_conformance-arc` are now prerequisites of `test:` (Makefile), which
      `ci:` already depends on — so every CI run replays the whole differential
      harness twice more, once under `--memory=gc` and once under
      `--memory=arc`, and fails if either diverges byte-for-byte from the
      default. Both targets dropped their `build` prerequisite to match every
      other private `_test_*` target: `test:` already depends on `build`, and a
      sub-make prerequisite re-ran the entire workspace + VSIX build twice.
      Verified locally: `PASS=148 FAIL=0` under default, `--memory=gc`, and
      `--memory=arc` alike.
- [x] **Make the ARC zero-leak bar actually enforced.** `_conformance-arc`
      exports `OSPREY_ARC_DEBUG=1`, which is what makes `memory_arc.c` report its
      live-object count at exit and `run_test_corpus.sh` count leaky programs at
      all. The bar was doubly opt-in before: no target set the variable and
      nothing asserted the count, so [GC-ARC-PERCEUS]'s "zero leaked language
      values" was documented but unchecked.
      **Mechanism corrected (2026-07-30):** this item used to say the *Makefile*
      greps for `ARC_LEAKY=0`. It does not — `grep ARC_LEAKY Makefile` is empty.
      The real gate is the harness's own exit expression
      (`crates/run_test_corpus.sh`: `[[ $fail -eq 0 && $leaky -eq 0 && … ]]`),
      which fails closed for the same reason: a lost env var yields no
      `[osp-arc] exit:` lines, the leak count cannot be confirmed zero, and the
      run fails rather than passing silently. Verified: the arc pass reports
      `TEST_CORPUS_ARC_LEAKY=0` over 160 programs. Spec 0018 says **where** the
      bar is enforced (`make test` via `_conformance-arc`) rather than implying
      every bare harness run checks it.
- [x] **Refresh the stale ARC figure in `benchmarks/README.md`** —
      `benchmarks/report.py` regenerates the marked README measurement from
      `benchmarks/results/results.json` alongside the HTML report
      (`update_readme`, driven by `render`). Current tracked data: default
      **631 MB**, ARC **3.26 MB**, GC **14.9 MB**. `measured_peak` replaced a
      bare `:.3g`, which rendered the gigabyte-scale default as `2.53e+03 MB` in
      published prose.
- [x] **`update_readme` is pinned.** `UpdateReadmeTests` in
      `benchmarks/test_merge_results.py` (run by `make test` via
      `_test_bench_tools`) feeds known peaks into a temporary README and asserts
      the emitted line byte-for-byte, the filtered-run no-op and the
      missing-marker failure. Writing it exposed a boundary defect: 999,999,999
      bytes rounded to 1000 MB and rendered as `1e+03 MB`, the very form
      `measured_peak` exists to prevent; the unit is now chosen after rounding.
- [ ] Write the **quicksort** and **mergesort** cases (all 7 languages + oracle).
      No longer blocked: the list-literal layout defect that segfaulted them is
      fixed, and a head/tail-recursive partition runs correctly under default, GC
      and ARC. Use explicit recursion, not `filter` — see the next item.
- [ ] Materialize an `Iterator<T>` back into a `List<T>`. `filter`/`map` return
      `Iterator<T>` and only `forEach`/`fold` consume one, and neither accepts a
      `List` (`fold(xs, …)` over a list is `cannot unify Iterator<t> with
      List<int>`). So no `filter`-shaped list pipeline can be written at all,
      which is the real reason the `filter` spelling of quicksort is unavailable.
      Needs a builtin, a spec anchor in 0012, and doc entries.
- [ ] (Later) mutable arrays → sieve, matrix-multiply, n-sieve, fannkuch,
      n-queens.
- [ ] (Later) `int`↔`float` + `sqrt` → mandelbrot, n-body, spectral-norm.
- [ ] (Later) arbitrary-precision integers → pidigits.
