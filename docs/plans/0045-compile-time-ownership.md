# Plan 0045 — Compile-time ownership (`--memory=ownership`)

> **NOT IN ACTIVE WORK.** Written on 2026-11-01 to record the design. Nobody implements, tests or benchmarks anything in this plan until the owner schedules it. Until then the `none` backend stays exactly as [spec 0018](../specs/compiler/0018-MemoryManagement.md) describes it.

Contract: [MEM-OWNERSHIP-STATIC](../specs/compiler/0018-MemoryManagement.md#compile-time-ownership-mem-ownership-static). Status: **not started.**

## Summary

A third memory backend, after tracing GC and ARC: the compiler proves when every managed value dies and emits the free itself, the way Rust's ownership and borrowing rules do. There is no collector and no reference count at run time. Unlike Rust, Osprey source carries no lifetimes, `&`, `move` or ownership annotations; the existing ownership ledger ([MEM-OWNERSHIP]) infers owners and borrows, and a program whose ownership the compiler cannot prove is rejected with a diagnostic naming the value and both uses.

`ownership` replaces the unmanaged `none` backend (the old `--memory=default`). `none` never reclaims general allocations; `ownership` reclaims all of them with the same zero run-time bookkeeping. When this plan lands, `none` and `compiler/runtime/memory_runtime.c` are deleted, not deprecated.

## Why it fits Osprey

- **The heap is acyclic** ([MEM-ACYCLIC]): immutable values cannot form cycles, so a single-owner tree plus borrows covers the whole heap.
- **The ledger already exists**: codegen already tracks fresh owners, moves, borrows, proved last uses and proved-unique values (`osp_release_unique`). Today an unproved case falls back to `osp_retain`/`osp_release`; under `ownership` an unproved case is a compile error instead.
- **Collection stays unobservable** ([MEM-OPAQUE]): the backend changes when memory is freed and which programs compile, never a compiled program's output.

## Open design questions

Decide these before phase 1; record each answer in spec 0018.

- **Sharing.** Persistent lists and maps share structure between versions. Either the backend rejects programs that keep two live versions, or shared nodes are copied on the second use (an implicit clone the diagnostic can recommend instead).
- **Fibers and channels.** `spawn`, `await` and channel sends currently co-own values with the runtime ([MEM-FIBER-ISOLATION]). Ownership must transfer to exactly one side, or repeated `await` of a managed result is rejected.
- **Closures and effects.** Captured values, resumptions and handler state need an owner the analysis can see across `perform`/`resume`.
- **Host libraries.** Exported globals and C ABI results ([MEM-BACKENDS]) need a documented transfer rule for WASM, iOS, Android and Python callers.
- **Diagnostics.** The rejection must say what to change in source with no annotation syntax available, for example binding a copy or restructuring the second use.

## Phase 1 — analysis without codegen

- [ ] `--memory=ownership` accepted by the CLI; it runs the ledger in checking mode and reports every value that would need a run-time retain.
- [ ] `examples/failscompilation/` programs for each rejected pattern; `tests/regressions/` programs that must be accepted.
- [ ] A measurement of how much of `tests/` and `examples/` passes the check unchanged; numbers only.

Exit: the check rejects every failscompilation program with a precise diagnostic and accepts the regression set.

## Phase 2 — codegen and runtime

- [ ] Codegen emits direct frees at proved last uses and region exits; no retain/release calls survive in `ownership` IR.
- [ ] A runtime archive with allocation and free only; the backend hook ABI ([MEM-BACKENDS-CUSTOM]) keeps the other hooks as no-ops.
- [ ] `crates/run_test_corpus.sh` runs the corpus under `ownership` alongside GC, ARC and wasm32, with zero live allocations at exit as ARC already checks.

Exit: identical corpus output under all backends; zero leaks under `ownership`.

## Phase 3 — replace `none`

- [ ] Delete `--memory=none`, `memory_runtime.c`, the wasm `none` passthrough and every test, bench and doc reference; the old spelling is rejected like `--memory=default`.
- [ ] Benchmarks report `ownership` beside GC and ARC.
- [ ] Spec 0018 states the shipped contract; this plan is retired.
