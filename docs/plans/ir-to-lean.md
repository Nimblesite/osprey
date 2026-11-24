# IR to Lean framework

Spec: [IR-LEAN](../specs/tooling/IrToLean.md). Goal: replace the regex translators in `scripts/proof/*.mjs` with a framework that reads the compiler's real IR through standard tools, runs it under a Lean semantics with a real memory model, checks the model against the real binary, and proves the ownership rules for every function.

## State on 2026-11-24

Committed:

- [x] `6977e6487` first Lean semantics (superseded by the work below).
- [x] `da4e68358` compiler fix [TYPE-GENERICS-FN]: a function passed through a recursive generic helper stayed the first caller's function for every later caller. Found while writing the reader. Regression in `tests/flavors/functions` (three twins). Corpus 340 of 341 under gc and arc; the one failure is another agent's uncommitted edit to `core/gpu/scalar_contracts.test.ospml`.

Built and checked by Lean 4.25.0, committed with this plan:

- [x] [IR-LEAN-FORMAT] LLVM's importer accepts the whole of `single_owner.test.osp` (9143 operations) with no error.
- [x] [IR-LEAN-READER] `@osprey/xdsl` lexer, attribute parser and generic operation parser. It reads that whole module in about half a second.
- [x] [IR-LEAN-TRANSLATE] `ir_lean` command. It translates 12 of the 76 defined functions; see the refusal table below.
- [x] [IR-LEAN-SYNTAX] [IR-LEAN-MEMORY] [IR-LEAN-EXEC] Lean subset, memory model and interpreter.
- [x] [IR-LEAN-GENERAL] `run_follows_ledger`, axioms `propext` and `Quot.sound`.
- [x] [IR-LEAN-KERNEL-RFL] `kernel_rfl`; a false statement is refused.
- [x] [IR-LEAN-FORCE] forcing functions with identity proofs.
- [x] [IR-LEAN-EMITTED] `growScalar`, `growManaged`, `growWhen` accepted by Lean's kernel in under a second.

Measured in Osprey's kernel (`check-batch.gc`):

| Version | `run_follows_ledger` | `growScalar` | `growWhen` |
| --- | --- | --- | --- |
| Proof carried in the state | 2.4 s | 122 s | not run |
| Plain state, no forcing | not rerun | 6.3 s | stopped after 6 minutes |
| Plain state with forcing | **not measured** | **not measured** | **not measured** |

A profile of the slow run put almost every sample in `Terms.underBinders` (substitution) and the allocation it causes.

## Not verified

These were written but not run to green. Treat each as unproven.

- [ ] Osprey-kernel replay time and result with [IR-LEAN-FORCE] in place, on gc, arc and wasm.
- [ ] The WASM kernel trapped (`null function or function signature mismatch` in `Terms.underBinders`) on `growScalar` with the proof-carrying state. Not retested since.
- [ ] `scripts/proof/ir-lean.sh` ran end to end only against the first design and stopped at that WASM trap. It has not run since the interpreter was rewritten. It pins `Quot.sound, propext` for every theorem; `growScalar` now reports `propext` alone, so the pins are wrong.
- [ ] The mutation step of that script has never run.

## To do, in order

### 1. Make the kernel replay fast

- [ ] Measure the three theorems with forcing on gc, arc and wasm. Target: each replay under 10 seconds.
- [ ] If `growWhen` is still slow, profile again and find which value is still evaluated more than once. Candidates: the `block` and `env` passed between blocks, and Nat arithmetic the kernel does not accelerate (`%`, `>>>`, `/`, `≤`).
- [ ] Kernel: make `Terms.underBinders` keep unchanged subterms instead of rebuilding them (return "same" or "new" from the walk), and drop the separate `mentioned` pre-walk. Decide closedness of a substituted value once per substitution, not once per occurrence.
- [ ] Kernel, larger: cache the loose-variable bound in each term node so substitution skips closed subterms. This changes the `Term` type, which about 2000 sites construct or match; coordinate with whoever owns `examples/proof/kernel` and `workbench`.
- [ ] Kernel, largest: share evaluation of a value used more than once (a memoising reducer). Without it, any proof by computation has to be written around the kernel, as [IR-LEAN-FORCE] does.

### 2. Wire it into CI

- [ ] Fix the axiom pins in `scripts/proof/ir-lean.sh` to what each theorem reports, then run it to green including the four mutants.
- [ ] Add `_test_ir_lean` to `scripts/make/proof.mk`, make `_test_ownership_proof` depend on it, and add a commented entry to `make help`.
- [ ] Install `mlir-translate` and `mlir-opt` in the `Tests: proof kernel and Lean speed gate` job and pin the LLVM version. Locally they come from Homebrew LLVM 23.1.4.

### 3. Make `@osprey/xdsl` a governed package

The package directory exists without the files every package needs. Until these are done, package checks that enumerate `src/osprey/packages` may fail.

- [ ] `README.md`, `LICENSE`, `LICENSES/` with xDSL's Apache-2.0 WITH LLVM-exception text, and `NOTICE.md` naming xDSL commit `efd7a0e7`.
- [ ] Entries in `src/osprey/packages/license-audit.json`, `source-audit.json`, the table in `packages/README.md` and `coverage-thresholds.json`.
- [ ] A real test suite in `tests/` with an oracle recorded from xDSL's own parser on the same inputs. `tests/main.py` is currently a development driver that prints an operation count.
- [ ] Tests and an `expectedoutput` for the `ir_lean` example app.
- [ ] Doc comments on the union variants and constants the compiler warns about.

### 4. Differential check [IR-LEAN-DIFFERENTIAL]

- [ ] Add the allocation and free trace to `memory_tracked_heap.h` and `memory_ownership.c`.
- [ ] A subject program with functions over integers and records for the exact tier, and list functions for the boundary tier.
- [ ] A Lean executable that reads each recorded case, runs the model and reports the first disagreement.
- [ ] Thousands of seeded random inputs per function in CI.

### 5. Widen the subset

Refusals on `single_owner.test.osp` (64 of 76 functions):

| Count | Reason | Needs |
| --- | --- | --- |
| 58 | `llvm.mlir.addressof` | read-only globals for string constants, and models for `sprintf`, `osp_format_size`, `osp_format_into` |
| 2 | `osprey_list_get` | extern model |
| 1 each | `osprey_list_builder_new_of`, `osp_string_show`, `osp_string_compare` | extern models |
| 1 | indirect call | function pointers and closures |

- [ ] Record functions (`pairUp`, `pick`): the layout-mask drop walk uses `%` and `>>>`; confirm Osprey's kernel can reduce them or restate with accelerated operations.
- [ ] Publish the translated count per program and gate it so it only rises.

### 6. Retire the regex translators

- [ ] Run the clang-compiled runtime (`list_runtime.c`, `map_runtime.c`, `string_runtime.c`, `memory_ownership.c`) through the same pipeline and restate `runtimeRootOrigins`, `renderedStringOrigin`, `stringResultOrigins`, `compiledFreeNull`, `compiledPairFields`, `compiledChildDrop`, `compiledWorklistPush`, `compiledPairQueue` and `compiledGrowthBytes` over `LlvmSubset`.
- [ ] Delete `ownership-lowering.mjs`, `runtime-returns.mjs`, `child-drop.mjs`, `record-ownership.mjs`, `worklist-drain.mjs` and `OwnershipLowering.lean.in`. Another agent has uncommitted edits in several of these; coordinate first.

### 7. The compiler-wide theorem

- [ ] Prove that every function the compiler emits answers, so `run_follows_ledger` applies to all of them (plan 0045 V8). This needs the compiler's ownership decisions (which parameters a function owns) as structured data beside the IR.

### 8. Documentation

- [ ] `website/src/docs/proofs/how-proofs-check-code.md` still describes the regex translator and says "nothing is skipped". Rewrite steps 2 to 4 for this pipeline once it is in CI.
- [ ] Record this framework in [plan 0045](0045-compile-time-ownership.md) and the memory management spec.

## Decisions

- **Python flavor for the reader**, so the port stays line-comparable with xDSL.
- **Lean-MLIR is not a dependency.** Reasons are in [IR-LEAN-MLIR].
- **Plain state and a separate ledger proof**, not a proof carried in the state.
