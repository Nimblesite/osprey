# Plan Compile-time ownership (`--memory=ownership`)

Contract: [MEM-OWNERSHIP-STATIC](../specs/compiler/0018-MemoryManagement.md#compile-time-ownership-mem-ownership-static)

## Constraints

- Leave `--memory=none`, `src/toolchain/compiler/runtime/memory_runtime.c`, and the wasm `none` passthrough unchanged until Phase 3; they stay as [MEM-BACKENDS] describes.
- `ownership` adds no lifetimes, borrow markers, or ownership annotations to source; unprovable ownership is a compile error naming the value and both uses [MEM-OWNERSHIP-STATIC].
- Collection stays unobservable: the backend changes only which programs compile and when memory is freed, never a compiled program's output [MEM-OPAQUE].

## Prerequisites in place

- Perceus ownership ledger with proved last uses and `osp_release_unique` already exists in src/toolchain/crates/osprey-codegen/src/arc.rs [MEM-OWNERSHIP].

## Phase 0 — design decisions recorded in spec 0018

- [ ] Decide the persistent-list/map sharing rule (reject two live versions vs. copy shared nodes on second use) and record it in docs/specs/compiler/0018-MemoryManagement.md under [MEM-OWNERSHIP-STATIC].
- [ ] Decide the fiber/channel transfer rule (ownership transfers to exactly one side, or repeated `await` of a managed result is rejected) and record it in spec 0018 [MEM-FIBER-ISOLATION].
- [ ] Decide the closure/effect owner for captured values, resumptions, and handler state across `perform`/`resume`, and record it in spec 0018.
- [ ] Decide the host-library transfer rule for exported globals and C ABI results (WASM, iOS, Android, Python callers) and record it in spec 0018 [MEM-BACKENDS].
- [ ] Decide the rejection diagnostic's recommended source change (e.g. bind a copy or restructure the second use) and record it in spec 0018.
- Acceptance: each answer is written into spec 0018 before Phase 1 starts.

## Phase 1 — analysis without codegen

- [ ] Accept `--memory=ownership` in `parse_memory` (src/toolchain/crates/osprey-cli/src/invocation.rs) and in the usage strings; run the ownership ledger in checking mode and report every value that would need a run-time retain.
- Acceptance: `osprey --memory=ownership` compiles a simple program; an unprovable program fails with a diagnostic naming the value and both uses; `parse_memory("ownership")` returns Ok and the tests in src/toolchain/crates/osprey-cli/src/tests/execution.rs cover it.
- [ ] Add `src/osprey/examples/failscompilation/*.ospo` programs for each rejected pattern and `src/osprey/tests/regressions/` programs that must be accepted under `ownership`.
- Acceptance: the check rejects every failscompilation program with a precise diagnostic and accepts the regression set.
- [ ] Measure how much of `src/osprey/tests/` and `src/osprey/examples/` passes the check unchanged; record numbers only.
- Acceptance: a passing-fraction number is recorded.

## Phase 2 — codegen and runtime

- [ ] Emit direct frees at proved last uses and region exits in src/toolchain/crates/osprey-codegen/src/arc.rs and arc/last_use.rs; no `@osp_retain`/`@osp_release` calls survive in `ownership` IR.
- Acceptance: `ownership` IR contains frees only, no retain/release calls.
- [ ] Add `src/toolchain/compiler/runtime/memory_ownership.c` with allocation and free only; implement the remaining [MEM-BACKENDS-CUSTOM] hooks as no-ops.
- Acceptance: the archive builds and links; every ABI hook symbol is present with retain/release/finalizer/collection as no-ops.
- [ ] Run the corpus under `ownership` in src/toolchain/crates/run_test_corpus.sh alongside GC, ARC and wasm32, with zero live allocations at exit (the check ARC already performs).
- Acceptance: identical corpus output under all backends; zero leaks under `ownership`.

## Phase 3 — replace `none`

- [ ] (#378: v0.29.0's `--memory=none` macOS archive lacks `osp_mem_string_length` and cannot link; rejecting `none` closes it.) Delete `--memory=none` from `parse_memory` (src/toolchain/crates/osprey-cli/src/invocation.rs), `src/toolchain/compiler/runtime/memory_runtime.c`, the wasm `none` passthrough (`RUNTIME_LIB_NONE` / `libosprey_runtime_wasm.a` in src/toolchain/crates/osprey-cli/src/wasm.rs), and every test, bench and doc reference; `--memory=none` and `--memory=default` are both rejected.
- Acceptance: `parse_memory` rejects both spellings; grep finds no `memory_runtime.c` or `none`-backend references outside history.
- [ ] Report `ownership` beside GC and ARC in src/toolchain/benchmarks/run.sh (`LANGS`, `EXT`, and the compile case).
- Acceptance: `osprey-ownership` appears in benchmark output alongside `osprey-gc` and `osprey-arc`.
- [ ] Update docs/specs/compiler/0018-MemoryManagement.md [MEM-OWNERSHIP-STATIC] to state the shipped contract, removing the "Planned; not implemented" status; retire this plan.
- Acceptance: the spec no longer marks the backend planned.
