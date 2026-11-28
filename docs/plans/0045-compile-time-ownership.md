# Plan 0045 — Memory safety

BE EXTREMELY CAREFUL NOT TO VIOLATE YOUR USER AGREEMENT
LOOK UP WHAT IS OR ISN'T ALLOWED

Make every supported backend memory-safe without source-level ownership annotations: compile-time ownership accepts a program only when it proves one owner per allocation and a live lender for every borrow, and lowers those proofs to exact cleanup; runtime hardening bounds runaway heaps and ends process trees with their runners; the emitted code is proved in the proof kernel; and KLEE symbolic bug hunting sweeps the corpus for what the proofs do not cover.

Contracts: [MEM-OWNERSHIP-STATIC](../specs/compiler/0018-MemoryManagement.md#compile-time-ownership-mem-ownership-static) with its parameter, borrow, closure, path, sharing, fiber, handle, host, model and verification sub-contracts; [MEM-OWNERSHIP], [MEM-OPAQUE], [MEM-BACKENDS], [MEM-BACKENDS-CUSTOM], [MEM-BACKENDS-SHARED], [MEM-BACKENDS-FINALIZERS] and [MEM-FIBER-ISOLATION] in the same spec; the runtime contracts [MEM-HEAP-CEILING], [MEM-GC-SAFEPOINT], [RUN-PARENT-DEATH], [CONCURRENCY-FIBER-LIFETIME], [CONCURRENCY-CHANNEL-LIFETIME], [CONCURRENCY-DEADLOCK], [CONCURRENCY-WASM], [CONCURRENCY-HANDLES] and [CONCURRENCY-CHANNEL]; [IR-LEAN] and [IR-LEAN-ATOMIC] in [Ir to Lean](../specs/tooling/IrToLean.md); and the `[SYMEX-*]` contracts in [Symbolic bug hunting with KLEE](../specs/tooling/SymbolicExecution.md) and [KLEE code generation](../specs/compiler/KleeCodegen.md).

## Invariants

- `ownership` adds no lifetimes, borrow markers or ownership annotations to source; unprovable ownership is a compile error naming the value and both uses [MEM-OWNERSHIP-STATIC].
- Collection stays unobservable: the backend changes only which programs compile and when memory is freed, never a compiled program's output [MEM-OPAQUE].
- Accepted ownership IR contains direct frees and no `@osp_retain` or `@osp_release`; the `counts_nothing` check in `src/toolchain/crates/osprey-codegen/src/arc/ownership.rs` stays a hard gate while collection helpers are migrated.
- Every accepted corpus program runs under freed-memory scribbling and AddressSanitizer, matches the shared golden and exits with zero live allocations; every other program is refused by name, and accepting a program without listing it is a failure.
- Proofs derive from shipped source and fail on relevant mutations; output correctness alone never proves resource correctness.

## Ownership analysis and lowering

- [ ] Stored values, remaining shapes ([MEM-OWNERSHIP-STATIC-BORROWS]): accept anonymous records that hold a still-read value, list literals nested in other literals, lending values stored in fields typed `Result`, and file-scope bindings as lenders; keep the refusal of any lending value that escapes.
- [ ] A generic whose return type is a bare type variable, such as `fn keep(x) = x`, without expanding it in its caller or borrowing when the parameter should move [MEM-OWNERSHIP-STATIC-PARAMETERS].
- [ ] Split field reads ([MEM-OWNERSHIP-STATIC-PATHS]): a binding read only through separate fields gives up each field; add the `single_owner` case in `src/toolchain/crates/osprey-cli/tests/ownership_mode.rs`, a rejection fixture, a ledger theorem and a spec paragraph for the existing `arc/reads.rs` `split`, `arc/moves.rs` `splits_it` and `arc/takes.rs` `last_read` path.
- [ ] Remaining counted helpers: a lending box for `osp.any.*` and file-scope `mut` cells holding managed values without a count, including cells no handler rewrites; remove the remaining runtime counting once collection copies no longer need it.
- [ ] Frame-limited borrows ([MEM-OWNERSHIP-STATIC-BEYOND-RUST]): at a value's last use, give it to the callee, which frees it at its own last read; prove the space bound rather than inferring it from output or leak checks.
- [ ] In-place update ([MEM-BACKENDS-SHARED]): when a derived collection version's source is at its last use, overwrite it in `src/toolchain/compiler/runtime/list_runtime.c` and `map_runtime.c`, under ownership and ARC alike; two live versions remain refused by [MEM-OWNERSHIP-STATIC-SHARING].
- [ ] Returned and given values: cover functions that return a parameter or part of one without refusing every caller that reads the argument again; evaluate a borrowing specialization that writes a made value into a caller-supplied owner slot, and record pattern bindings as borrowers so scrutinee reads precede arm reads.
- [ ] Further provably safe shapes, each with its rule and proof before its refusal is lifted: a record update whose new fields read the base (`box { n: box.n + 1 }`), a parameter whose fields are stored one by one, and an element taken out of an owned list.
- [ ] Recursive `listPrepend` past one 32-element chunk releases every block. Check: a regression crossing several chunk boundaries exits with nothing live.
- [ ] The cross-backend parameter-shadowing miscompile: a parameter named like a top-level function must shadow that function. Check: positive and rejection regressions added before name resolution changes.

## Continuations, fibers and channels

- [ ] Continuations ([MEM-OWNERSHIP-STATIC-CLOSURES]): accept a `control` handler that resumes at most once; a dropped continuation frees everything it owns and runs finalizers exactly once; `many` stays refused, reported once at the handler and never through meaningless positions in the prelude's `__many*` helpers.
- [ ] Fibers and channels ([MEM-OWNERSHIP-STATIC-FIBERS], [MEM-FIBER-ISOLATION]): move `spawn` captures, permit one owning `await`, and move values through `send` and `recv` while preserving managed-handle sharing and finalization.

## Ownership runtime and conformance

- [ ] Complete `src/toolchain/compiler/runtime/memory_ownership.c` and the remaining [MEM-BACKENDS-CUSTOM] hooks: allocation and proved-unique free perform real cleanup (`osp_ownership_free` is the only drop of an ordinary owned allocation, and the compatibility hooks preserve it), retain, release and collection are no-ops, and finalization stays live where external subtypes require it.
- [ ] Finish the ownership corpus lane in `src/toolchain/crates/run_test_corpus.sh` and `corpus_ownership.sh`: every program the spec does not refuse by rule is listed in `src/osprey/tests/OWNERSHIP_ACCEPTED.txt`, matches its native golden and exits with zero live allocations under GC-independent ownership, ASan and freed-memory scribbling; every remaining refusal is a named diagnostic.
- [ ] Benchmarks: publish a quiet-machine run with `osprey-ownership` beside `osprey-gc` and `osprey-arc` in `src/toolchain/benchmarks/README.md`, with peak live bytes as its own column (the README records ARC and GC peaks only). Announce the measurement window on TMC and stop competing cargo, test and pi workloads.

## Proof of emitted code

- [ ] General lowering proof ([MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER]): extend the Lean ledger model and compiled-code certificates to every supported lowering, proving one owner per allocation and a live lender for every borrow on every control-flow edge, including borrow liveness, allocator correctness, child ownership and collection semantics. Complete the [IR to Lean](0041-dependent-types-and-mathlib.md#proofs-about-the-compilers-output-ir-lean) route in plan 0041: prove every emitted function answers, differentially check the model against the real binary, wire the check into CI and make Osprey-kernel replay practical; `run_follows_ledger` remains the baseline, not the completion claim.
- [ ] Independently replay both full heap equalities for `foeName` and `relabel` (`src/toolchain/benchmarks/proof/tools/OwnershipRecord.lean`); relate emitted control-flow edges and lifetimes to ledger transitions instead of assuming transitive lenders and last-read borrow ends.
- [ ] Audit the [compiler proof inventory](../specs/compiler/0018-MemoryManagement.md#proofs-of-compiled-ownership-code-mem-ownership-static-verification-compiler) against current source, including each mutation control and all three proof-kernel backends; close the remaining MLIR managed-list WASI replay and run the full ownership proof matrix.

## Runtime hardening

- [ ] Extend [MEM-HEAP-CEILING] to Windows and WebAssembly, where `src/toolchain/compiler/runtime/memory_heap_limit.h` currently falls back to no ceiling: a safe default or an explicit required configuration, so neither target grows without bound because physical memory is unavailable. Replace allocation-time runtime quarantine with typed or handled failure branches at compiled allocation sites.
- [ ] Identify and fix the remaining runaway-suite inputs; the two historical output names identify no current program and `safetensors/tests` does not reproduce the growth. Check: the implicated suites pass under the cap in GC and ARC with their peaks recorded; an unidentified cache key is not a fix.
- [ ] Complete [RUN-PARENT-DEATH]: add the Windows parent watch (`src/toolchain/compiler/runtime/run_parent_watch.h` covers Apple and Linux only), and export each runner's own process id from `run_test_corpus.sh` and the Rust test runners so that killing any supported runner terminates its `osprey` children and compiled programs.
- [ ] Derive the fiber/channel holder-count proof from compiled runtime code through [IR-LEAN] and [IR-LEAN-ATOMIC]: finish `scripts/proof/fiber-ir-lean.sh` kernel replay on GC, ARC and wasm with all three mutants (`src/toolchain/benchmarks/proof/tools/EmittedFiberLifetime.lean`, proving `held_hold` and `held_release`, is the starting point), then add a commented make target and CI wiring.

## Symbolic bug hunting

`osprey hunt` ships with the mock C library, `_runtime_klee`, native replay and the corpus scripts. The KLEE engine targets (`_test_hunt`, `_test_hunt_crashes`, `_test_hunt_clean`) run by hand only while #306 and #307 are open, and hosted CI runs `_test_hunt_mock` (`scripts/make/tests.mk`). The programs `src/osprey/tests/HUNT_UNPORTABLE.txt` skips belong to [plan 0055](0055-targets.md).

- [ ] Fix complete exploration of `src/osprey/tests/hunt/clean.osp` (#306): the digit and length queries over an unconstrained symbolic integer need about 49 s of solver work against the 30 s budget on gc and arc. Keep the 64-byte/30-second settings and do not retry the dead ends already measured and reverted at `f6085332` (details in `src/osprey/tests/hunt/known-bugs/README.md`); `--max-solver-time=30s` with the forked solver is mandatory [SYMEX-CLI]. Check: `make _test_hunt_clean` exits 0 on both backends.
- [ ] Remove `clean` from `KNOWN_BUGS` in `src/toolchain/crates/osprey-cli/tests/hunt_engine.rs`. Check: `make _test_hunt` runs `clean.osp` without skipping and passes [SYMEX-CLI].
- [ ] Typed function harness: emit `__osprey_klee_entry` for `--fn <name>`, parse `--fn`/`--all-fns` in `src/toolchain/crates/osprey-cli/src/hunt/options.rs` (both are refused today), pass `--entry-point=__osprey_klee_entry` to KLEE and add the `osp_sym_*` builders to `src/toolchain/compiler/runtime/klee/` per the builders table [SYMEX-HARNESS-TYPED]. Check: `klee_entry_is_built_from_inferred_types` passes; a planted bug in a function taking `List<int>` and a union is found with `--fn`; `--all-fns` reports skips with reasons.
- [ ] `libkleeRuntest` replay in `src/toolchain/crates/osprey-cli/src/hunt/replay.rs`: link the replay build against `libkleeRuntest` with `KTEST_FILE`, rebuild the harness arguments and serve entropy and clock from a provider over the recorded objects [SYMEX-REPLAY]. Check: a planted bug that fires only on one symbolic random byte reproduces natively.
- [ ] Regression writer [SYMEX-REGRESSION]: write each `reproduced` finding to `src/osprey/tests/regressions/klee/<file>_<fn-or-main>_<class>.osp` with the concrete input as literals and a failing `.expectedoutput` placeholder, keeping the `.ktest` beside the test for `--seed-file`; never overwrite an existing test.
- [ ] Make `make -C src/osprey/tests/hunt hunt-corpus` a required check in `scripts/ci/verify-branch-protection.mjs`, gated on a fully completed sweep; blocked by #306 and by #307 (the GC root pass refuses 47 of 215 corpus programs) [SYMEX-CORPUS]. Check: a full sweep completes every program and `verify-branch-protection.mjs` fails on any incomplete exploration.
- [ ] Windows: run KLEE inside WSL2 and add `--replay-host=windows` to `osprey hunt` (`hunt/options.rs`, `replay.rs`) to cover Windows `#if` branches [SYMEX-HOSTS].
- [ ] `src/toolchain/benchmarks/symex/`: the eight cases (`needle`, `offbyone`, `shortread`, `openfail`, `rarerandom`, `tagged`, `nested`, `clean`) in `cases/<name>/{osprey,c,rust,haskell}/`, `run.sh`, `report.py` with a unit test and a checked-in `results/results.json` meeting all four [SYMEX-PROOF](../specs/tooling/SymbolicExecution.md) superiority conditions; `make ci` re-renders the README from the JSON and fails on drift. Closes last.

## Completion

- [ ] Update [MEM-OWNERSHIP-STATIC] to the shipped contract, remove its planned status and make the spec's built/not-built marks in [MEM-OWNERSHIP-STATIC-BEYOND-RUST] match the code. Check: the ownership corpus, proof matrix, quiet benchmarks and branch-wide CI are green; retire the plan only then.
