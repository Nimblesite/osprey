# Plan 0045 — Memory safety

Make every supported backend memory-safe without source-level ownership annotations. Compile-time ownership must accept ordinary programs only when it can prove one owner per allocation and a live lender for every borrow, then lower those proofs to exact cleanup. Runtime hardening must bound runaway heaps, terminate process trees with their runners, and derive concurrency-safety proofs from the compiled runtime.

Primary contracts: [MEM-OWNERSHIP-STATIC](../specs/compiler/0018-MemoryManagement.md#compile-time-ownership-mem-ownership-static), [MEM-OWNERSHIP-STATIC-SAFETY], [MEM-OWNERSHIP-STATIC-VERIFICATION], [MEM-OWNERSHIP-STATIC-BEYOND-RUST], [MEM-OWNERSHIP], [MEM-OPAQUE], [MEM-BACKENDS], [MEM-BACKENDS-CUSTOM], [MEM-BACKENDS-SHARED] and [MEM-FIBER-ISOLATION].

Ownership sub-contracts: [MEM-OWNERSHIP-STATIC-PARAMETERS], [MEM-OWNERSHIP-STATIC-BORROWS], [MEM-OWNERSHIP-STATIC-CLOSURES], [MEM-OWNERSHIP-STATIC-PATHS], [MEM-OWNERSHIP-STATIC-SHARING], [MEM-OWNERSHIP-STATIC-FIBERS], [MEM-OWNERSHIP-STATIC-HANDLES], [MEM-OWNERSHIP-STATIC-HOSTS], [MEM-OWNERSHIP-STATIC-MODEL], [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] and [MEM-BACKENDS-FINALIZERS].

Runtime contracts: [MEM-HEAP-CEILING], [MEM-GC-SAFEPOINT], [RUN-PARENT-DEATH], [CONCURRENCY-FIBER-LIFETIME], [CONCURRENCY-CHANNEL-LIFETIME], [CONCURRENCY-DEADLOCK], [CONCURRENCY-WASM], [CONCURRENCY-HANDLES], [CONCURRENCY-CHANNEL], [FFI-FILE-SERVER-FOLDER], [BUILTIN-WEBSOCKET-FRAMING], [SYMEX-MOCK-THREADS], [IR-LEAN] and [IR-LEAN-ATOMIC].

## Invariants

- `ownership` adds no lifetimes, borrow markers or ownership annotations to source. Unprovable ownership is a compile error naming the value and both uses [MEM-OWNERSHIP-STATIC].
- Collection stays unobservable: the backend changes only which programs compile and when memory is freed, never a compiled program's output [MEM-OPAQUE].
- Accepted ownership IR contains direct frees and no `@osp_retain` or `@osp_release`. The `counts_nothing` check remains a hard gate while collection helpers are migrated.
- Every accepted corpus program runs with freed-memory scribbling and AddressSanitizer, matches the shared golden and exits with zero live allocations. Every other program is refused by name; accepting a program without listing it is a failure.
- Proofs derive from shipped source and fail on relevant mutations. Output correctness alone never proves resource correctness.

## Ownership analysis and lowering

- [ ] C2, remaining stored values ([MEM-OWNERSHIP-STATIC-BORROWS]): accept anonymous records that hold a still-read value, list literals nested in other literals, lending values stored in fields typed as `Result`, and file-scope bindings as lenders. Preserve the refusal of any lending value that escapes.
- [ ] Accept a generic whose return type is a bare type variable, such as `fn keep(x) = x`, without expanding it in its caller and borrowing when the parameter should move ([MEM-OWNERSHIP-STATIC-PARAMETERS]).
- [ ] Pin split field reads: a binding read only through separate fields gives up each field. Add the `single_owner` case, rejection fixture, ledger theorem and spec paragraph for the existing `arc/reads.rs` `split`, `arc/moves.rs` `splits_it` and `arc/takes.rs` `last_read` path ([MEM-OWNERSHIP-STATIC-PATHS]).
- [ ] C5, remaining counted helpers: implement a lending box for `osp.any.*` and lower file-scope `mut` cells holding managed values without a count, including cells no handler rewrites. Remove the remaining runtime counting once collection copies no longer need it.
- [ ] C8, frame-limited borrows ([MEM-OWNERSHIP-STATIC-BEYOND-RUST]): at a value's last use, give it to the callee, which frees it at its own last read. Prove the space bound rather than inferring it from output or leak checks.
- [ ] C9, in-place update ([MEM-BACKENDS-SHARED]): when a derived collection version's source is at its last use, overwrite it in `list_runtime.c` and `map_runtime.c`, under ownership and ARC alike. Two live versions remain refused by [MEM-OWNERSHIP-STATIC-SHARING].
- [ ] Returned and given values: cover functions that return a parameter or part of one without refusing every caller that reads the argument again. Evaluate a borrowing specialization that writes a made value into a caller-supplied owner slot, and record pattern bindings as borrowers so scrutinee reads precede arm reads.
- [ ] Accept additional provably safe shapes: a record update whose new fields read the base (`box { n: box.n + 1 }`), a parameter whose fields are stored one by one, and an element taken out of an owned list. Keep the current refusals until each shape has a rule and proof.
- [ ] Fix recursive `listPrepend` past one 32-element chunk so ownership releases every block; add a regression that crosses several chunk boundaries and exits with nothing live.
- [ ] Fix the cross-backend parameter-shadowing miscompile: a parameter named like a top-level function must shadow that function. Add positive and rejection regressions before changing name resolution.

## Continuations, fibers and channels

- [ ] C6, continuations ([MEM-OWNERSHIP-STATIC-CLOSURES]): accept a `control` handler that resumes at most once; a dropped continuation frees everything it owns and runs finalizers exactly once. `many` stays refused, reported once at the handler and never through meaningless positions in the prelude's `__many*` helpers.
- [ ] C7, fibers and channels ([MEM-OWNERSHIP-STATIC-FIBERS], [MEM-FIBER-ISOLATION]): move `spawn` captures, permit one owning `await`, and move values through `send` and `recv` while preserving managed-handle sharing and finalization.

## Ownership runtime and conformance

- [ ] Complete `src/toolchain/compiler/runtime/memory_ownership.c` and the remaining [MEM-BACKENDS-CUSTOM] hooks: allocation and proved-unique free perform real cleanup; retain, release and collection are no-ops; finalization remains live where external subtypes require it. Prove that only `osp_ownership_free` drops an ordinary owned allocation and compatibility hooks preserve it.
- [ ] Finish the ownership corpus lane in `src/toolchain/crates/run_test_corpus.sh`: every program the spec does not refuse by rule is listed in `OWNERSHIP_ACCEPTED.txt`, matches its native golden and exits with zero live allocations under GC-independent ownership, ASan and freed-memory scribbling. All remaining refusals stay named diagnostics.
- [ ] V5, benchmarks: publish a quiet-machine run with `osprey-ownership` beside `osprey-gc` and `osprey-arc`, including peak live bytes as its own column. Announce the measurement window on TMC and stop competing cargo, test and pi workloads.

## Proof of emitted code

- [ ] V8, general lowering proof ([MEM-OWNERSHIP-STATIC-BEYOND-RUST]): extend the Lean ledger model and compiled-code certificates to every supported lowering. Prove one owner per allocation and a live lender for every borrow on every control-flow edge, including borrow liveness, allocator correctness, child ownership and collection semantics.
  - [ ] Complete the [IR to Lean](../specs/tooling/IrToLean.md) route in [its plan](ir-to-lean.md): prove every emitted function answers, differentially check the model against the real binary, wire the check into CI and make Osprey-kernel replay practical. `run_follows_ledger` remains the baseline, not the completion claim.
  - [ ] Independently replay both full heap equalities for `foeName` and `relabel`; relate emitted control-flow edges and lifetimes to ledger transitions instead of assuming transitive lenders and last-read borrow ends.
- [ ] Audit the [compiler proof inventory](../specs/compiler/0018-MemoryManagement.md#proofs-of-compiled-ownership-code-mem-ownership-static-verification-compiler) against current source, including each mutation control and all three proof-kernel backends. Close the remaining MLIR managed-list WASI replay and run the full ownership proof matrix.

## Runtime hardening

- [ ] Extend [MEM-HEAP-CEILING] to Windows and WebAssembly with a safe default or an explicit required configuration; neither target may grow without bound merely because physical memory is unavailable. Replace allocation-time runtime quarantine with typed or handled failure branches at compiled allocation sites.
- [ ] Identify and fix the remaining runaway-suite inputs. The two historical output names do not identify a current program and `safetensors/tests` does not reproduce the growth. The implicated suites must pass under the cap in GC and ARC with their peaks recorded; an unidentified cache key is not a fix.
- [ ] Complete [RUN-PARENT-DEATH]: add the Windows parent watch and export each runner's own process id from `run_test_corpus.sh` and the Rust test runners so killing any supported runner terminates its `osprey` children and compiled programs.
- [ ] Derive the fiber/channel holder-count proof from compiled runtime code through [IR-LEAN] and [IR-LEAN-ATOMIC]. Finish `scripts/proof/fiber-ir-lean.sh` kernel replay on GC, ARC and wasm with all three mutants, then add a commented make target and CI wiring. `EmittedFiberLifetime.lean` proving `held_hold` and `held_release` is the starting point.

## Completion

- [ ] Update [MEM-OWNERSHIP-STATIC] to describe the shipped contract, remove its planned status, and make the spec's built/not-built marks in [MEM-OWNERSHIP-STATIC-BEYOND-RUST] match the code. Retire this plan only after the ownership corpus, proof matrix, quiet benchmarks and branch-wide CI are green.
