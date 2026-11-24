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
  - 2026-11-24: enforced. A derived version takes its source by move and releases it at once; `...rest` is a lent view (`osprey_list_view`); a `fold` step owns its accumulator. Open: a recursive `listPrepend` past one 32-element chunk still leaves blocks under the ownership runtime (ARC frees them).
  - 2026-11-24, audit of three hazards PrismCodex raised from the research: a call that gives a value away and lends a field of it is refused (`given_then_read`); every handler of a `control` operation is refused, resumed or dropped (`continuation_kept`); a `many` handler is refused. No accepted program captures a continuation. Open: a `many` refusal also lists the prelude's `__many*` helpers with meaningless positions.
- `spawn`, `await`, channel sends and handler environments retain what crosses them ([MEM-OWNERSHIP-STATIC-FIBERS]).
- A `mut` file-scope binding read by a function is published with its own reference. An immutable one hands the module the reference it holds.


- [ ] Emit direct frees at proved last uses and region exits in src/toolchain/crates/osprey-codegen/src/arc.rs and arc/last_use.rs; no `@osp_retain`/`@osp_release` calls survive in `ownership` IR.
- Acceptance: `ownership` IR contains frees only, no retain/release calls.
- **Owner: OspreyFable2** (item C5 below; C6 and C7 remove the fiber, channel and handler retains). PrismCodex adds the test that fails on a retain or release call in `ownership` IR (V4).
- [ ] Add `src/toolchain/compiler/runtime/memory_ownership.c` with allocation and free only; implement the remaining [MEM-BACKENDS-CUSTOM] hooks as no-ops.
- Acceptance: the archive builds and links; every ABI hook symbol is present with retain/release/finalizer/collection as no-ops.
- **Owner: PrismCodex.** The file exists (21cb47d01) over the shared tracked heap. Its retain still marks a block as moving and its release frees (c2ef56a69), because accepted IR still calls them. PrismCodex makes retain a no-op and release the direct free the day OspreyFable2 reports C5 done, and ticks this with the archive build and `memory_ownership_tests`.
- [ ] Run the corpus under `ownership` in src/toolchain/crates/run_test_corpus.sh alongside GC, ARC and wasm32, with zero live allocations at exit (the check ARC already performs).
- Acceptance: identical corpus output under all backends; zero leaks under `ownership`.
- **Owner: PrismCodex** for the lane and its gate (V2 done, V3). **OspreyFable2** raises the number of programs it accepts (C1 to C4). Ticked when every program the spec does not refuse by rule passes the lane.

## Phase 3 — replace `none`

- [x] (#378: v0.29.0's `--memory=none` macOS archive lacks `osp_mem_string_length` and cannot link; rejecting `none` closes it.) Delete `--memory=none` from `parse_memory` (src/toolchain/crates/osprey-cli/src/invocation.rs), `src/toolchain/compiler/runtime/memory_runtime.c`, the wasm `none` passthrough (`RUNTIME_LIB_NONE` / `libosprey_runtime_wasm.a` in src/toolchain/crates/osprey-cli/src/wasm.rs), and every test, bench and doc reference; `--memory=none` and `--memory=default` are both rejected.
- Acceptance: `parse_memory` rejects both spellings; grep finds no `memory_runtime.c` or `none`-backend references outside history.
- [x] Report `ownership` beside GC and ARC in src/toolchain/benchmarks/run.sh (`LANGS`, `EXT`, and the compile case). On 2026-11-24 the measurement stage compiled all 24 benchmarks in each mode and all 72 outputs matched their goldens.
- Acceptance: `osprey-ownership` appears in benchmark output alongside `osprey-gc` and `osprey-arc`.
- [ ] Update docs/specs/compiler/0018-MemoryManagement.md [MEM-OWNERSHIP-STATIC] to state the shipped contract, removing the "Planned; not implemented" status; retire this plan.
- Acceptance: the spec no longer marks the backend planned.

## Phase 4 — build what spec 0018 requires

The spec states the contract: [MEM-OWNERSHIP-STATIC-SAFETY], [MEM-OWNERSHIP-STATIC-VERIFICATION], [MEM-OWNERSHIP-STATIC-BEYOND-RUST] and [MEM-BACKENDS-SHARED]. Every item below is one requirement of it that is not built. One checkout, branch `fixes`, explicit-path commits, no push. Each owner edits only their files, works test-first, and reports the commit and its evidence on TMC.

### OspreyFable2 — the compiler and the collection runtime

Files: `src/toolchain/crates/osprey-codegen/src/arc/**`, `collections*`, `iter.rs`, `pattern/**`, `expr/**` where the ledger is called, `compiler/runtime/list_runtime.c` and `map_runtime.c`, `failscompilation/ownership/`, `tests/regressions/ownership/`.

Acceptance of ordinary programs, largest refusal class first. After each item: `ownership_mode` and `osprey-codegen` tests pass, `single_owner` gains the case and exits with no live object, and the accepted count below is updated.

- [ ] C1. Generic functions own parameters. Infer per function body, apply to every instance ([MEM-OWNERSHIP-STATIC-PARAMETERS]).
- [ ] C2. A value stored in a record, Result or collection that is then only read (the largest class, about 945 refusals): the constructor lends when the new value does not outlive the stored one.
- [ ] C3. Record update whose base is at its last use moves the fields it keeps; a field read out of a value at its last use moves.
- [ ] C4. The remaining classes: bound, returned, joined, kept, shared. Each gets a rule or stays a documented refusal with a fixture.
- Acceptance: `ownership` accepts every program in `src/osprey/tests/` that does not use a construct the spec refuses by rule. Today 39 of 181.

Soundness and the space bound:

- [ ] C5. No `@osp_retain`/`@osp_release` in `ownership` IR (phase 2 item 1): a lending box for `osp.any.*`, `mut` file-scope bindings.
- [ ] C6. Continuations ([MEM-OWNERSHIP-STATIC-CLOSURES]): accept a `control` handler that resumes at most once; a dropped continuation frees what it owns; `many` stays refused, reported once at the handler and never in the prelude's `__many*` helpers.
- [ ] C7. Fibers and channels move ([MEM-OWNERSHIP-STATIC-FIBERS]): `spawn` captures, `await` once, `send` and `recv`.
- [ ] C8. Frame-limited borrows ([MEM-OWNERSHIP-STATIC-BEYOND-RUST], space bound): a value at its last use is given to the callee, which frees it at its own last read.
- [ ] C9. In-place update: a derived version whose source is at its last use overwrites it (`list_runtime.c`, `map_runtime.c`), under `ownership` and `arc` alike ([MEM-BACKENDS-SHARED]).

### PrismCodex — the harness, verification, the runtime contract and the proof

Files: `src/toolchain/crates/run_test_corpus.sh`, `src/toolchain/benchmarks/**`, `osprey-cli/src/**` and its tests, `compiler/runtime/memory_*`, the Makefiles and CI, `docs/` other than spec 0018's ownership sections, the Lean proof tooling.

- [x] V1. `--memory=none` deleted everywhere (phase 3 item 1; c54aa2118, 70689c8af, d56504cc2, da6f758c5).
- [x] V2. Corpus harness runs `ownership` (0ab14eb53): baseline 52 of 342 goldens, no leak among the programs that ran.
- [ ] V3. The corpus lane is a gate: a program on the accepted list that is refused, leaks or differs from `gc` and `arc` fails CI. The list only grows.
- [ ] V4. Verification lane ([MEM-OWNERSHIP-STATIC-VERIFICATION]): every accepted program also runs with freed memory scribbled and under AddressSanitizer; a test fails when `ownership` IR calls `@osp_retain` or `@osp_release` once C5 lands.
- [ ] V5. Benchmarks: `osprey-ownership` beside `osprey-gc` and `osprey-arc` (phase 3 item 2), with peak live bytes as its own column. Run on a quiet machine: agents announce a window on TMC and stop cargo, tests and pis for it.
- [ ] V6. Shared code ([MEM-BACKENDS-SHARED]): run deslop over `compiler/runtime/memory_*`; move what `arc` and `ownership` both need into `memory_tracked_heap.h`; no copy survives in two backends, no technique is weakened.
- [ ] V7. Hosts ([MEM-OWNERSHIP-STATIC-HOSTS]): `ownership` archives for wasm32, Apple and Android link and run `single_owner` with its golden.
- [ ] V8. The proof ([MEM-OWNERSHIP-STATIC-BEYOND-RUST]): a Lean model of the ledger's rules, derived from the spec and checked against OspreyFable2's rules as they land, with the theorem that lowering keeps one owner per allocation and a live lender for every borrow on every control-flow edge. First the model and the statement; then the proof.

### Together, last

- [ ] The spec's "Built" and "Not built" marks in [MEM-OWNERSHIP-STATIC-BEYOND-RUST] match the code; phase 2 and 3 boxes are ticked with evidence; this plan is retired and its rules live in spec 0018.

### Order

OspreyFable2: C1, C2, C3, then C6 and C5, C8, C7, C9, C4. PrismCodex: V5 and V3 first (the user asked to see the bench), then V4, V6, V7, V8. A change that crosses the split is announced on TMC before it is made.
