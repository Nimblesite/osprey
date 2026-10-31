# Recent OpenAI proof ports — updated 2026-10-30

Verification record for the inputs and revision named below. Current development follows the [Lean oracle and discipline-correction contract](../messaging.md#proof-development-and-the-lean-oracle); this record does not define a competitive objective or establish consistency.

Twelve selected supporting lemmas from the October 2026 OpenAI mathematics release now check in the shared Osprey kernel under GC, ARC and wasm32, including one finite-index injectivity proof added after repairing typed proof conversion. All twelve reject a corrupted proof and a false replacement statement on every target. The final compiler checkpoint passes all 36 checks and 72 mutation refusals in `target/logs/ci-prep/proof-openai-host-abi-current.log`, after the constructor-alias eta correction. The shared compiler hash is `6f5c55438f39162831fc9c2ccf3dcdb5d45dcc1a9a984deec4c73de768831dae`. The replacement statement is `∀ p : Prop, p`; the original proof is retained. Each corrupt-proof control retains the original statement and substitutes its type expression as the proof. Every rejection must be a kernel type mismatch, not a parser or missing-dependency error.

These are the checked-export layer of [PROOF-RUNNABLE-LIBRARY]. They are not completed ports of the release's headline results, and a readable Osprey proof frontend is still separate work. The library proofs are checked by Osprey code; Lean produces the input certificate and independently cross-checks it. Browser, native and VSIX hosts reuse this same kernel.

## Source and exact statements

Upstream: [openai/math](https://github.com/openai/math/tree/7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a), commit `7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a`. The original release uses Lean 4.34.1. These selected declarations compile with the project's pinned Lean 4.25.0 after replacing unused Mathlib imports with `Init`, spelling `Type*` with an explicit universe and restoring natural-number notation. The original sources and Apache-2.0 license are retained under `examples/proof/openai/`; `corpus.json` pins original source, compatible source and export hashes.

The first six selected stack lemmas in `OAI.PrefixFlows.Interspersed` are `push_zero`, `push_succ`, `prepend_nil`, `prepend_cons`, `prepend_append` and `tail_push`. They retain the upstream statements and proof bodies. `push_inj` and `push_constant` now also check, retaining their exact upstream statements and proof bodies. These prove stack-constructor injectivity and that pushing a constant onto its constant infinite stack gives the same stack. Each genuinely depends on `Quot.sound`, which derives function extensionality; neither is described as axiom-free. The corpus harness now checks the complete quotient-soundness axiom statement, universe parameter and safety flag, with mutation tests for each.

The selected arithmetic lemmas in `OAI.LowSpaceSubsetSum.WordSeedPowerExecution` are `loop_value`, `run_value` and `run_work`. The loop doubles a value and charges three work units per iteration; its exact result is `(value * 2^fuel, work + 3*fuel + 1)`. The two corollaries prove the initialized loop returns `2^width` and charges `3*width+1`. The statements are unchanged. The final `omega` proof step was replaced by constructive equalities using `Nat.mul_succ`, `Nat.add_assoc` and `Nat.add_comm`; this reduced the dependency export from 71,953 lines to 4,307 without changing the result.

| Module | Selected lemmas | Export lines | Bytes | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| OpenAIStacks | 8 | 2,400 | 132,300 | `f2256725919162d349eca2454b26ebc23fe939caec07650db4f1a9ccc83855f3` |
| OpenAIWordSeed | 3 | 4,307 | 218,919 | `14b3950c6e2ea1e73ed3fa529baf236346cb397da4bfa5d695c0049511b81a33` |

There are 101 distinct theorem names across the three checked dependency closures; twelve are the selected upstream lemmas. The dependency count is not a count of newly ported research results.

The finite-index injectivity proof, `OAI.PermanentBorder.AmbientPolar.groupIndex_injective`, now checks from an empty Osprey environment. It maps `Option Bool` to the three distinct elements of `Fin 3`. Its statement is unchanged; the proof uses constructive finite cases in place of the newer upstream decision procedure. The former level-normalization, K-reduction and typed proof-irrelevance blockers were fixed through focused regressions. Its 10,122-line, 539,668-byte complete export is `examples/proof/openai/ambient-polar.ndjson`; SHA-256 `b3cef268b53ce335e0df7eec9fa50d0fa7fdf6118748d62fc421b4c24bf7a177`. Fresh Lean export and `lean4checker --fresh` pass, followed by actual Osprey acceptance and both mutation refusals under GC.

The separate public core-library corpus checks all 175 selected statements under GC, ARC and wasm32: 525 successful checks and 1,050 rejected proof/statement mutations. This includes 172 original exports and three compatible constructive proofs of the exact original Nat statements. The three original certificates remain refused at their nested `Lean.Syntax` dependency; acceptance of an alternative body does not establish support for the original certificate. Fresh exports match byte for byte, Lean's fresh checker accepts the modules, and each selected root's axiom report matches fresh `#print axioms`. `examples/proof/lean-core/compatible.json` and `frontier.json` retain the statement comparison and the original-certificate boundary; `target/logs/ci-prep/proof-core-host-abi-current.log` records the final matrix. No unsupported declaration is skipped.

Both actual empty and supplementary-Unicode string constructor proofs now pass under all three backends, with six axiom-free receipts and twelve rejected mutations, byte-identical fresh exports and independent Lean replay. The retained `scripts/test-proof-literals.py` regression no longer crashes; final evidence is in `target/logs/ci-prep/proof-string-host-abi-current.log`. Private five-colour number-theory alternatives are separate candidates and are not included in the public twelve-statement OpenAI corpus.

## Trust and regressions

All three OpenAI closures contain the standard Lean axiom `propext` (the stack closure also contains the exact `Quot.sound` statement), with statement `∀ a b : Prop, (a ↔ b) → a = b`. The corpus harness checks that exact expression structure, universe and safety flag, and refuses extra axioms or a changed statement. The batch entry now reports each root's transitive axiom dependencies, including declaration types, values and inductive metadata. The original unused-axiom regression is green: `id_imp` reports `axioms: none` despite an unrelated assumption in its file. Eleven focused trust tests pass under GC. The corpus pins separate whole-closure and per-root ledgers; fresh `#print axioms` output from Lean must match the latter. Five stack lemmas require no axioms; `prepend_append` and the arithmetic/injectivity lemmas retain `propext`. An axiom, definition, type former, constructor or recursor cannot satisfy a request for a proved theorem. Missing files and missing theorem names are explicit failures. No unsupported declaration is skipped.

The stack export exposed two incomplete universe reductions: `imax 1 (u+1) = u+1` and a maximum dominated by a larger maximum. The new focused regression failed before the fix. The normalizer now factors a common successor from a maximum and uses its existing sound level-order rules to remove dominated sides. All twelve focused universe tests pass, including unequal-level controls. An external pi reviewed both statement fidelity and the new normalization rules; its findings were reviewed by the primary agent. This review is not a formal proof of kernel soundness.

## Reproduce

From the repository root:

```sh
python3 scripts/proof-openai.py --compiler target/ci/osprey
python3 scripts/test-proof-openai.py
```

The first command rebuilds the shared batch checker under GC, ARC and wasm32, regenerates all three exports byte for byte, independently replays each compatible module through `lean4checker --fresh`, checks all named theorems and verifies every mutation. Target: thirty-six `PASS` rows, each with its actual per-root axiom report and `corrupted proof and false statement rejected`. The second command runs twenty-five ledger, inventory, receipt, statement-provenance, staging and mutation regressions. The existing CI `test-proof` job runs both with its original conditions and timeout.

An immediate native proof check after the build is:

```sh
printf 'examples/proof/openai/word-seed.ndjson\tOAI.LowSpaceSubsetSum.WordSeedPowerExecution.loop_value\n' | target/proof/openai/check-batch.gc
```

Expected: `PASS OAI.LowSpaceSubsetSum.WordSeedPowerExecution.loop_value | axioms: propext`.

Local evidence is retained in `target/proof/openai/batch-matrix.log`, `verified-batch.log`, the focused red/green universe logs and CI-wiring logs. The final expanded kernel suite passes all 325 cases on each of GC, ARC and wasm32; `target/logs/ci-prep/pi-kernel-host-abi-current.status` records `kernel-gate-exit=0`, with equal before/after compiler hashes. Full repository CI and workbench visual acceptance remain separate coordinated work.


## Shared app integration checkpoint (2026-10-30)

`apps/workbench/src/imported.ospml` now supplies the same pure `proof::ImportedProofs` adapter to all hosts. It checks the complete current source from an empty environment, accepts only theorem declarations, exposes the actual statement and per-root axiom list, refuses empty catalogues and bounds imports at 8 MiB before parsing. Its eight cases are registered in the canonical workbench suite. The final frozen app suite passes 108 cases on GC, ARC and wasm32. Measured application coverage is 2,053/2,261 lines (90.8%). The shared catalogue stages twelve export groups and two unchanged upstream licences; host interactions and final native/installed-VSIX visual acceptance remain separate gates. See the [website learning paths](https://www.ospreylang.dev/docs/proofs/) and [current port guide](https://www.ospreylang.dev/docs/mathlib-port/).
