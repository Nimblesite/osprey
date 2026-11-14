# Plan 0033: Generated-code performance

Contract: [spec 0037 arithmetic effects](../specs/language/0037-ArithmeticEffects.md), [performance gate](../specs/ecosystem/PerformanceGate.md), [tracing heap layout](../specs/compiler/0018-MemoryManagement.md#tracing-heap-layout-mem-backends-gc-heap)

## Prerequisites in place
The `rust-checked` column tooling is already wired: `src/toolchain/benchmarks/run.sh` (`LANGS` includes `rust-checked`, builds it with `-C overflow-checks=on`), `src/toolchain/benchmarks/report.py` (`ORDER`/`LABEL` include `Rust (checked)`), `src/toolchain/benchmarks/README.md` documents the column. No timings have been recorded for it yet.

## Phase 1: honest baseline
- [ ] Record `rust-checked` timings so the published table shows the like-for-like checked-Rust column. Run `make -C src/toolchain/benchmarks bench` (the full suite: `src/toolchain/benchmarks/run.sh` builds `rust-checked` with `-C overflow-checks=on`, then `merge_results.py` + `report.py` merge and render). Acceptance: `src/toolchain/benchmarks/results/results.json` lists `"rust-checked"` in `languages` and every benchmark case has a `rust-checked` cell with `status: "ok"`; the baked `src/toolchain/website/src/_includes/benchmarks-tables.html` contains a `Rust (checked)` column. Requires `rustc` on PATH (absent toolchains are skipped, not fatal).

## Phase 2: tracing heap from size-class pages [MEM-BACKENDS-GC-HEAP]
Measured on 14 November 2026 with a one-second sample of `binarytrees` under `--memory=gc`: system `malloc`/`free` 38%, the table insert in `gc_managed_alloc` 28%, collection 25%, zeroing 3%, the program 5%; 133 collections with about 640 KiB (27,318 blocks) live. The recorded result is 0.98 s, 58 times Haskell's. Mergesort, quicksort and wordfreq under GC are 3 to 5 times slower than Haskell and Dart, and are expected to share the cause.

All work is in `src/toolchain/compiler/runtime/` (`memory_gc.c`, `memory_gc_table.h`, a new `memory_gc_pages.h`); files stay under 500 lines.

- [ ] Tests first, in `memory_gc_tests.c`, red against the current runtime where they describe new behaviour: blocks of every size class and of one byte above the largest class survive and die correctly across collections; finalizers and string blocks keep their flags; a pinned block survives; a word that today does not keep a block alive still does not; a page with no live cell is returned (resident memory falls after an idle collection).
- [ ] Page allocator: size classes for small blocks, pages aligned so a page is found from an address, per-page mark and flag state outside the cells, bump allocation then free-cell reuse within a class.
- [ ] `gc_managed_alloc` and `gc_lookup` use pages for small blocks and the existing table for large ones; `osp_gc_live_objects` and the other diagnostics count both.
- [ ] Sweep clears unmarked cells, runs finalizers before any reuse, and returns empty pages.
- [ ] Remove the code the pages make dead; rewrite the comment at `gc_managed_alloc` that records the rejected recycling pool so it describes what is built.
- [ ] Verify: C runtime suites under UBSan, the test corpus under GC (322 programs), the proof kernel and package suites under GC, then `make ci`.
- [ ] Re-time with `make -C src/toolchain/benchmarks bench-osprey` on a quiet machine. Acceptance: `binarytrees` under GC takes at most half its recorded 0.98 s, no Osprey GC cell is more than 5% slower than its record, and no other language's value changes.
- [ ] Re-measure the 4 MiB heap floor (`OSPREY_GC_HEAP_MB`) against 2 MiB once pages are in; lower it only if no case slows.

Out of scope: ARC allocation (`binarytrees` under ARC is 12 times Haskell's, not yet profiled), a moving or generational collector, and Windows support for GC.
