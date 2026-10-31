# Plan 0044 — Lean source compatibility

> **STOP — NOT YET IN FORCE.** Written on 2026-10-31 while the `feat/type-disciplines` PR is still open. **Any agent working on that PR ignores this plan until the PR has merged.** Do not delete, rename, rewrite or stop maintaining anything because of this text before then.

Contract: [PROOF-SOURCE](../specs/language/0049-DependentTypesAndProofs.md#proof-source-proof-source), [spec 0051](../specs/applications/0051-ProofWorkbench.md), [spec 0052](../specs/applications/0052-WorkbenchLessons.md). Status: **not started.** This plan runs first after the merge; plans [0041](0041-dependent-types-and-mathlib.md), [0042](0042-proof-workbench.md) and [0043](0043-workbench-lessons.md) resume behind it.

## Summary

`.lean` is the only proof format. Osprey runs Lean proofs: first by driving the pinned Lean toolchain and checking its export with Osprey's kernel, then by running Lean's compiled front end itself, then by replacing hot paths natively with identical results. Everything that is not a proof sits in one extension format that refers into the `.lean` file by declaration name. The two Osprey-invented proof formats are deleted.

## Phase 0 — delete the contamination

Everything below is removed, not deprecated. No shim, no reader kept "for old files", no test left skipped.

- [ ] `apps/workbench/src/formula.ospml`, `statement.ospml`, `step.ospml`, `goals.ospml`, `emit.ospml`, `kernel.ospml` (the impredicative prelude) and the step-elaborating half of `verdict.ospml`; their tests; `tests/golden/`; every fixture holding English steps or `// osprey: types=lean4`.
- [ ] `examples/proof/kernel/src/surface.ospml` and its tests (`id_imp_typed`, `imp_trans`, `forall_typed`, `wrong_typed`); the M2 lines of `expectedoutput`.
- [ ] Lesson JSON fields `statement`, `given`, `solution`, `steps`, `expected` and `formula` blocks in the old notation; the shipped lessons are rewritten under phase 3.
- [ ] `benchmarks/proof/m0_identity`; the speed gate in `benchmarks/proof/run.sh`, `scripts/make/proof.mk` and `.github/workflows/ci.yml`; the ordering requirement in `benchmarks/proof/README.md`. Reports keep numbers only.
- [ ] Every mention of the deleted formats in `apps/workbench/README.md`, `docs/designs/proof-workbench/`, `docs/specs/applications/FormulaCodeGeneration.md` and AGENTS.md Commands.
- [ ] A grep gate in `make ci` that fails on `types=lean4`, `Assume h`, `By cases on`, `It suffices to show`, `by imported`, `prove port` anywhere outside git history, this plan and the deletion notices of specs 0049 and 0051.

Exit: `make ci` green with the gate in place; `apps/workbench` coverage still at 90% over what remains.

## Phase 1 — toolchain stage

- [ ] `osprey prove check <file.lean | Lake project>`: locate the pinned toolchain (elan, `lean-toolchain`, or a bundled copy), run `lake build`/`lean`, run the pinned `lean4export` on the built module, check the export with the kernel, print Lean's diagnostics verbatim; exit 0 only when both accept. Exact command and output in AGENTS.md Commands.
- [ ] The `Lean` host capability in `@osprey/native` (`LeanCheck`, `LeanGoals`) over Lean's language server; completions carry a revision ticket.
- [ ] Workbench `Document`, `Lean` and `Verdicts` modules of spec 0051; `document`, `goal` and `read` regions over a `.lean` buffer; the toolchain-unavailable path.
- [ ] macOS host bundles the toolchain; VS Code host finds or installs it; browser and iPadOS report Unavailable.
- [ ] Acceptance corpus 1: Lean's own `tests/lean` at v4.25.0 through `osprey prove check` gives the same accept/reject verdict and the same first error for every file. Corpus 2: every file in `benchmarks/proof/tools/` and `examples/proof/openai/`.
- [ ] File-to-verdict measurement recorded by hand for `AndSwap.lean` and `OpenAIStacks.lean`; numbers only.

Exit: a mathematician opens a fresh `.lean` file in the workbench, writes `and_swap`, sees Lean's goals and a kernel-filled badge; the same file passes `osprey prove check` and `lake build`.

## Phase 2 — front-end stage

- [ ] A reader for `.olean` files (the pinned format) into Osprey's term and environment store.
- [ ] An interpreter for Lean's compiled IR, in Osprey, sufficient to run `Lean.Elab.Frontend` over `Init`; then `Std`; then Mathlib's metaprograms.
- [ ] `osprey prove check` and the language-server services run with no Lean executable present; exported terms byte-identical to phase 1 on corpora 1 and 2, then on `Mathlib/Algebra/Group/Basic.lean`.
- [ ] Browser and iPadOS hosts gain verdicts.

Exit: the independence test of plan 0041 milestone 8 passes for every corpus file: no Lean binary, same verdicts, same errors, same export bytes.

## Phase 3 — lessons and the extension format

- [ ] Each lesson is `lessons/<slug>.lean` plus `lessons/<slug>.json`; the JSON refers to declarations by name ([LESSONS-FORMAT](../specs/applications/0052-WorkbenchLessons.md#the-lesson-format-lessons-format)).
- [ ] `Judge::prove`, `Judge::state`, `Judge::judge` through the `Lean` capability and the kernel; the learner types Lean.
- [ ] The seven lessons rewritten to teach Lean's words (`intro`, `cases`, `constructor`, `exact`) with their English explanation beside each.
- [ ] Integrity suite: every lesson `.lean` checks; every solution `Checked`; every judge declaration `Checked` or `Rejected`.

Exit: the lesson suite and both host smoke tests pass on the rewritten lessons.

## Phase 4 — native stage

One component at a time, each behind a switch that is off until the corpus is identical: the exporter, then the unifier, then type-class search, then individual tactics. A component that differs anywhere on corpora 1, 2 or Mathlib stays off. Record each switch-on with its corpus receipt in `docs/reports/`.

## What this plan never does

- Add a keyword, marker, header, dialect or second proof format.
- Keep a reader, writer, test or lesson for the deleted formats "for compatibility".
- Gate CI on a timing.
- Report a verdict the Lean toolchain or Osprey's front end did not give.
