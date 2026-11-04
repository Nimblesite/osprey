# Plan 0034: Symbolic bug hunting with KLEE

Contract: [Symbolic bug hunting with KLEE](../specs/tooling/SymbolicExecution.md) (`[SYMEX-CLI]`, `[SYMEX-KLEE-INVOCATION]`, `[SYMEX-FINDINGS]`, `[SYMEX-REPLAY]`, `[SYMEX-REGRESSION]`, `[SYMEX-CORPUS]`, `[SYMEX-HOSTS]`, `[SYMEX-PROOF]`), [KLEE code generation](../specs/compiler/KleeCodegen.md) (`[SYMEX-CODEGEN]`, `[SYMEX-HARNESS-TYPED]`, `[SYMEX-RUNTIME-BITCODE]`)

## Prerequisites in place

- `osprey hunt` main mode, the mock libc, `_runtime_klee`, `compile_program_symbolic`, native stdin/file replay, corpus scripts (`scripts/hunt_corpus.sh`, `scripts/hunt_summary.py`, `scripts/hunt_programs.py`), `src/osprey/tests/HUNT_UNPORTABLE.txt`, the planted-crash gate (`src/osprey/tests/hunt/crash-cases.json`, `scripts/hunt_crashes.py`, `make -C src/osprey/tests/hunt hunt-crashes`) and the VSCode `osprey.hunt` command all ship.
- The KLEE engine targets (`_test_hunt`, `_test_hunt_crashes`, `_test_hunt_clean`) run by hand only; `make test`/CI exclude them while issues #306 and #307 are open (`scripts/make/tests.mk`).
- `_test_hunt` skips `clean.osp` via `HUNT_KNOWN_BUGS ?= clean`; its complete-exploration defect is #306, at the pinned 64-byte/30-second settings.

- The programs `src/osprey/tests/HUNT_UNPORTABLE.txt` skips, and the model work to bring them in, are in [plan 0055](0055-corpus-target-gaps.md) [CORPUS-TARGET-GAPS].

## Phase 3 completion proof `[SYMEX-CLI]`

- [ ] Fix `clean.osp` complete exploration (#306): `src/osprey/tests/hunt/clean.osp` prints a symbolic integer whose digit/length queries over the unconstrained value need ~49 s solver work, over the 30 s budget on both gc and arc. Do not relax the 64-byte/30-second settings; do not retry the dead ends recorded in `docs/reviews/review.md` (raw-view bounded parse, O(n) line copy, GC scan alignment filter, binary-search digit length). Acceptance: `make _test_hunt_clean` exits 0 on both backends.
- [ ] Remove `clean` from `HUNT_KNOWN_BUGS` in `scripts/make/tests.mk` so `_test_hunt` covers `src/osprey/tests/hunt/clean.osp` again. Acceptance: `make _test_hunt` runs clean.osp without skipping and passes.
- [ ] Wire `scripts/test-hunt-default-budget.py` into a hunt test target as a required gate (it exists but no make target references it). Acceptance: `make` runs it and fails on incomplete exploration at the default 64-byte/30-second settings.

## Verification gap `[SYMEX-CODEGEN]`

- [ ] Build `klee_corpus_parses_with_pinned_llvm` (named in KleeCodegen.md Verification): `llvm-as`-parse every module in the `src/osprey/tests/` corpus with KLEE's pinned LLVM 19. Acceptance: the test exists and passes.

## Phase 4: typed harness and libkleeRuntest replay `[SYMEX-HARNESS-TYPED]` `[SYMEX-REPLAY]`

- [ ] Implement the typed function harness: emit `__osprey_klee_entry` in codegen for `--fn <name>`, add `--fn`/`--all-fns` parsing to `src/toolchain/crates/osprey-cli/src/hunt/options.rs`, pass `--entry-point=__osprey_klee_entry` to KLEE, and add the `osp_sym_*` builders to `src/toolchain/compiler/runtime/klee/` per the builders table in [SYMEX-HARNESS-TYPED](../specs/compiler/KleeCodegen.md). Acceptance: test `klee_entry_is_built_from_inferred_types` passes; a planted bug in a function taking `List<int>` and a union is found with `--fn`; `--all-fns` reports skips with reasons.
- [ ] Implement `libkleeRuntest` replay in `src/toolchain/crates/osprey-cli/src/hunt/replay.rs`: link the replay build against `libkleeRuntest` with `KTEST_FILE`, rebuild harness arguments and serve entropy/clock from a provider over the recorded objects. Acceptance: a planted bug that fires only on one symbolic random byte reproduces natively.

## Phase 5: regression writer, corpus gate, Windows `[SYMEX-REGRESSION]` `[SYMEX-CORPUS]` `[SYMEX-HOSTS]`

- [ ] Implement the regression writer: write each `reproduced` finding to `src/osprey/tests/regressions/klee/<file>_<fn-or-main>_<class>.osp` with the concrete input as literals and a failing `.expectedoutput` placeholder; keep the `.ktest` beside the test for `--seed-file`; never overwrite an existing test.
- [ ] Make `make -C src/osprey/tests/hunt hunt-corpus` a required check in `scripts/verify-branch-protection.mjs`, gated on a fully completed sweep (zero budget-cut programs). Blocked by #306 (`clean.osp`) and #307 (the GC root pass refuses 47 of 215 corpus programs, `docs/reviews/review.md`). Acceptance: a full sweep completes every program and `verify-branch-protection.mjs` fails on any incomplete exploration.
- [ ] Windows: run KLEE inside WSL2 and add `--replay-host=windows` to `osprey hunt` (parse in `src/toolchain/crates/osprey-cli/src/hunt/options.rs`, native replay in `replay.rs`) to cover Windows `#if` branches.

## Phase 6: proof benchmarks `[SYMEX-PROOF]`

- [ ] Build `benchmarks/symex/`: the eight cases (`needle`, `offbyone`, `shortread`, `openfail`, `rarerandom`, `tagged`, `nested`, `clean`) in `cases/<name>/{osprey,c,rust,haskell}/`, `run.sh`, `report.py` with a unit test, and a checked-in `results/results.json` meeting all four [SYMEX-PROOF](../specs/tooling/SymbolicExecution.md) superiority conditions. `make ci` re-renders the README from the JSON and fails on drift. Closes last.
