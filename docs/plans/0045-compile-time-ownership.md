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
- Acceptance: the check rejects every failscompilation program with a precise diagnostic and accepts the regression set. The native and WASI CLI regression `ownership_check_agrees_with_lowering` compares check-only and LLVM emission across the accepted and refused fixtures ([MEM-OWNERSHIP-STATIC-VERIFICATION], c693bc797).
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
- **Owner: PrismCodex.** The file exists (21cb47d01) over the shared tracked heap. Its retain still marks a block as moving and its release frees (c2ef56a69), because accepted IR still calls them. Once generated code and runtime helpers transfer or free every owned value explicitly, PrismCodex makes both compatibility hooks no-ops. The archive and `memory_ownership_tests` must demonstrate that only `osp_ownership_free` drops an owned allocation and that the remaining ABI hooks preserve it.
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

- [x] C1. Generic functions own parameters. Infer per function body, apply to every instance ([MEM-OWNERSHIP-STATIC-PARAMETERS]). Done: an owning generic is emitted as a real function under `ownership`; a list literal given to an owned parameter is moved; a fresh list literal gives its reference to its runtime copy (it leaked one element per literal passed straight to a deriving operation). `single_owner` `generics()` exits with no live object; corpus under ARC unchanged, no leak. Open: a generic whose return type is a bare type variable (`fn keep(x) = x`) is still expanded in its caller and borrows.
- [ ] C2. A value stored in a record, Result or collection that is then only read (the largest class, about 945 refusals): the constructor lends when the new value does not outlive the stored one.
  - Built for named record and union constructors ([MEM-OWNERSHIP-STATIC-BORROWS], "Lent fields"): a field written as a binding moves in at its last use and is lent otherwise; the lent field is left out of the new value's drop walk, and the value can't be returned, kept or bound at file scope. A top-level function used as a value is a constant cell and needs no owner. `single_owner` `stores()` covers a chain of stores, one value in two fields, a union and a loop; `stored_then_read` and `stored_twice` now refuse the escaping value. Corpus 40 to 54 of 181, each accepted golden byte-identical with no live object under scribbled frees; 164 goldens under ARC unchanged, no leak. Stored refusals 922 to 808.
  - Built for closures ([MEM-OWNERSHIP-STATIC-CLOSURES]): a closure literal takes each capture at its last use and holds it lent otherwise; an inline lambda's environment borrows; a lending value written in place moves into the value that holds it, which lends in turn; a refusal of a lending value names the binding it holds. `spawn` never lends. `single_owner` `closures()` exits with no live object. Corpus 56 to 59 of 181; refusals 1785 to 1410 (stored 802 to 516, bound 164 to 59).
  - Proved ([MEM-OWNERSHIP-STATIC-MODEL], `OwnershipLedger.lean`): lent fields and captures (`lent_pins_owner`, `lent_field_not_freed`, `lent_then_released`, `lent_twice_released`), field takes (`take_moves_owner`, `taken_field_not_freed`, `take_needs_no_loan`, `taken_freed_once`), no use after free (`dead_rejects`) and nested holders (`edge_frames`). Also proved, for the rules already built: every death is one free of an unborrowed value (`only_free_kills`), a given value is gone from its giver (`given_is_gone`), one move per slot (`moved_once`), joins agree on the owner (`join_needs_one_owner`), borrowers share one owner (`borrow_keeps_owner`), reads are of live values (`read_needs_live`). `scripts/proof/ownership-ledger.sh` replays all 18 ledger theorems in Osprey's proof kernel under GC, ARC and WASI with pinned axioms, and three weakened models lose their proofs. Every new ledger rule lands with its theorem.
  - Open: list literals, Results and anonymous records that hold a still-read value; file-scope bindings as lenders.
- [ ] C3. Record update whose base is at its last use moves the fields it keeps; a field read out of a value at its last use moves.
  - Built for temporaries ([MEM-OWNERSHIP-STATIC-PATHS], "Taking a field", `arc/takes.rs`): a field read out of an unnamed value the function owns takes its reference at a return, a binding, a store, a call that owns the argument and a `match` join; the slot is cleared so the value frees without it. `single_owner` `takes()` exits with no live object; `borrowed_from_a_temporary` now refuses a read from a temporary that only lends. Corpus 54 to 56 of 181, each byte-identical with no live object under scribbled frees.
  - Open: a field of a named binding or an owned parameter at its last use; record update.
- [ ] C4. The remaining classes: bound, returned, joined, kept, shared. Each gets a rule or stays a documented refusal with a fixture.
- Acceptance: `ownership` accepts every program in `src/osprey/tests/` that does not use a construct the spec refuses by rule. Today 59 of 181.

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
- [x] V3. The required corpus matrix runs `make _conformance-ownership` against the same native goldens as GC and ARC. The PR gate rejects removal of a path from `OWNERSHIP_ACCEPTED.txt`. Local verification on 2026-11-24 matched all 52 listed goldens with zero live objects at each exit. Full-corpus acceptance remains open in phase 2.
- [x] V4. Verification lane ([MEM-OWNERSHIP-STATIC-VERIFICATION]): every accepted program also runs with freed memory scribbled and under AddressSanitizer; a test fails when `ownership` IR calls `@osp_retain` or `@osp_release` once C5 lands.
  - The native ownership lane scribbles freed objects, bypasses recycling and instruments generated loads and stores with AddressSanitizer. On 2026-11-24 all 52 accepted programs matched their goldens with zero live objects. The runtime test checks that freeing a tree poisons its root and both descendant levels. A second full-corpus run accepted 54 of 342 programs. All 54 matched their goldens and reported zero live objects; the required lane now includes both dialects of the struct-allocation stress test. The instrumenting compiler driver now rejects reference-count calls before compilation. All 54 required goldens passed the IR check and ASan after union rendering began transferring its result through the join and synchronous rendering began borrowing the descriptor payload (54da733ef). The subsequent full instrumented run matched 90 of 342 goldens with zero live objects and no sanitizer reports. All 90 are now required by the lane. The other 252 programs remain outstanding for V3.
- [ ] V5. Benchmarks: `osprey-ownership` beside `osprey-gc` and `osprey-arc` (phase 3 item 2), with peak live bytes as its own column. Run on a quiet machine: agents announce a window on TMC and stop cargo, tests and pis for it.
  - The harness records ARC and ownership peak live allocation bytes in the correctness pass. On 2026-11-24 both measured 29 bytes on `fib` and 257,352 bytes on `listops`; all six runs across GC, ARC and ownership matched their goldens. Published timings still require the quiet run.
- [x] V6. The root drop, both shared ABI drop entry points and child traversal live in the tracked heap. Each backend supplies its own last-owner rule. GC and the tracked heap share checked zeroed allocation. Deslop 0.41.0 reported no cluster containing `memory_arc.c` or `memory_ownership.c`; the scoped scan of `memory_*` stayed at 68 duplicated lines. The GC, ARC and ownership C suites and the ownership ASan suite passed.
- [x] V7. Hosts ([MEM-OWNERSHIP-STATIC-HOSTS]): on 2026-11-24 WASI, Apple silicon macOS, the arm64 iOS simulator, and the arm64 Android emulator each linked and ran `single_owner` under ownership. All four matched the native golden byte for byte and reported zero live objects. Each full cross-target corpus run now repeats this check on its own host.
- [ ] V8. The proof ([MEM-OWNERSHIP-STATIC-BEYOND-RUST]): a Lean model of the ledger's rules, derived from the spec and checked against OspreyFable2's rules as they land, with the theorem that lowering keeps one owner per allocation and a live lender for every borrow on every control-flow edge. First the model and the statement; then the proof.
  - `make _test_ownership_proof` checks `OwnershipLedger.from_empty` with Lean 4.25. Every allocation on a path accepted by the model has exactly one owner while live, and each outstanding loan names that owner. Allowing a free while a loan survives breaks both the preservation proof and the executable rejection example. The model assumes the compiler resolves transitive lenders and ends each borrow after its last read. Relating emitted control-flow edges and lifetimes to these model transitions remains open.

  - The [compiler proof inventory](../specs/compiler/0018-MemoryManagement.md#proofs-of-compiled-ownership-code-mem-ownership-static-verification-compiler) records the checked list append specializations, list/map header origins, the erased-string renderer, 15 runtime string operations, assumptions and negative controls. General lowering, borrow liveness, allocator correctness, child ownership and collection semantics remain open.

### Together, last

- [ ] The spec's "Built" and "Not built" marks in [MEM-OWNERSHIP-STATIC-BEYOND-RUST] match the code; phase 2 and 3 boxes are ticked with evidence; this plan is retired and its rules live in spec 0018.

### Order

OspreyFable2: C1, C2, C3, then C6 and C5, C8, C7, C9, C4. PrismCodex: V5 and V3 first (the user asked to see the bench), then V4, V6, V7, V8. A change that crosses the split is announced on TMC before it is made.
