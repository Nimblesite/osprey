# Plan Compile-time ownership (`--memory=ownership`)

Contract: [MEM-OWNERSHIP-STATIC](../specs/compiler/0018-MemoryManagement.md#compile-time-ownership-mem-ownership-static)

## Constraints

- Leave `--memory=none`, `src/toolchain/compiler/runtime/memory_runtime.c`, and the wasm `none` passthrough unchanged until Phase 3; they stay as [MEM-BACKENDS] describes.
- `ownership` adds no lifetimes, borrow markers, or ownership annotations to source; unprovable ownership is a compile error naming the value and both uses [MEM-OWNERSHIP-STATIC].
- Collection stays unobservable: the backend changes only which programs compile and when memory is freed, never a compiled program's output [MEM-OPAQUE].

## Prerequisites in place

- Perceus ownership ledger with proved last uses and `osp_release_unique` already exists in src/toolchain/crates/osprey-codegen/src/arc.rs [MEM-OWNERSHIP].

## Phase 0 — design decisions recorded in spec 0018

- [x] Decide the persistent-list/map sharing rule (reject two live versions vs. copy shared nodes on second use) and record it in docs/specs/compiler/0018-MemoryManagement.md under [MEM-OWNERSHIP-STATIC].
- [x] Decide the fiber/channel transfer rule (ownership transfers to exactly one side, or repeated `await` of a managed result is rejected) and record it in spec 0018 [MEM-FIBER-ISOLATION].
- [x] Decide the closure/effect owner for captured values, resumptions, and handler state across `perform`/`resume`, and record it in spec 0018.
- [x] Decide the host-library transfer rule for exported globals and C ABI results (WASM, iOS, Android, Python callers) and record it in spec 0018 [MEM-BACKENDS].
- [x] Decide the rejection diagnostic's recommended source change (e.g. bind a copy or restructure the second use) and record it in spec 0018.
- Acceptance: each answer is written into spec 0018 before Phase 1 starts.

## Phase 1 — analysis without codegen

- [x] Accept `--memory=ownership` in `parse_memory` (src/toolchain/crates/osprey-cli/src/invocation.rs) and in the usage strings; run the ownership ledger in checking mode and report every value that would need a run-time retain.
  - Built: the flag, the checking mode in src/toolchain/crates/osprey-codegen/src/arc/ownership.rs and src/toolchain/crates/osprey-cli/tests/ownership_mode.rs. The diagnostic names the value, the use that needs a second owner and the other use that keeps it (a later read with its place, or a second read in the same statement).
- Acceptance: `osprey --memory=ownership` compiles a simple program; an unprovable program fails with a diagnostic naming the value and both uses; `parse_memory("ownership")` returns Ok and the tests in src/toolchain/crates/osprey-cli/src/tests/execution.rs cover it.
- [x] Add `src/osprey/examples/failscompilation/*.ospo` programs for each rejected pattern and `src/osprey/tests/regressions/` programs that must be accepted under `ownership`.
  - Refused: `src/osprey/examples/failscompilation/ownership/*.osp`, each beside its exact diagnostics. They are `.osp`, not `.ospo`, because every one compiles and runs under the collector; only `--memory=ownership` refuses them. Accepted: `src/osprey/tests/regressions/ownership/single_owner.test.osp`, which the corpus also runs under GC, ARC and wasm32. Both sets are driven by src/toolchain/crates/osprey-cli/tests/ownership_mode.rs.
- Acceptance: the check rejects every failscompilation program with a precise diagnostic and accepts the regression set.
- [x] Measure how much of `src/osprey/tests/` and `src/osprey/examples/` passes the check unchanged; record numbers only.
- Acceptance: a passing-fraction number is recorded.
  - 2026-11-23, single-file `.osp` programs compiled with `--llvm --memory=ownership`: tests 17 of 180 accepted (4,954 refusals: 3,624 stored, 407 bound, 304 kept, 295 joined, 239 returned, 85 shared); examples 7 of the 29 that compile alone. First run the same day, before any move was added: tests 15 of 180, 5,956 refusals.
  - 2026-11-24, after path moves, inferred borrows and owned parameters: tests 18 of 181 accepted, 4,699 refusals (3,744 stored, 237 returned, 207 kept, 197 bound, 180 joined, 85 shared, 49 given). Every accepted test and example prints its collector output with zero live blocks and freed memory scribbled.
  - Open, found on everyday idioms rather than the corpus: a record update whose base is read again, which shares every field it does not replace; a field moved out of a value at its last use. Accumulators and identity functions are accepted since owned parameters.
  - 2026-11-24, with lent closures, lent argument lists and lent collection reads: tests 21 of 181 accepted, 2,819 refusals. An independent review (a boxed pi running 17 adversarial programs under gc, arc and ownership with scribbled frees) found no memory-safety defect.
  - Largest open class in the corpus: values stored in a record, Result or collection that is then only read, where the stored value has another reader. Closures and list literals written as arguments lend since lent closures.

## Phase 2 — codegen and runtime

Found while building phase 1, each a place where an accepted program still counts a reference at run time and so blocks an allocate-and-free-only runtime:

- The helpers the compiler synthesises for rendering and structural equality (`osp.any.*`) box a borrowed payload and retain it for the life of the helper. They need a box that lends.
- The list runtime duplicates the elements it copies (`osprey_list_builder_push_borrowed`) and shares structure between versions, in C. The sharing rule [MEM-OWNERSHIP-STATIC-SHARING] has to be enforced at compile time before those counts can go.
- `spawn`, `await`, channel sends and handler environments retain what crosses them ([MEM-OWNERSHIP-STATIC-FIBERS]).
- A `mut` file-scope binding read by a function is published with its own reference. An immutable one hands the module the reference it holds.


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
