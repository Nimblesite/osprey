---
layout: page
title: The Mathlib port is underway
description: "Current evidence for Osprey's proof-library port: seventeen OpenAI supporting lemmas, 175 selected Lean-core statements, explicit assumptions and rejected corruptions."
permalink: /docs/mathlib-port/
---

Osprey already checks real imported mathematical proofs using a kernel written in Osprey. The library port is underway: pinned source files, dependency exports, exact statements, axiom reports and negative controls live in the repository. This page records the verified checkpoint of **30 October 2026**.

[Watch the programmer introduction](/docs/proofs/videos/mathlib-bridge/) or [the mathematics version](/docs/proofs/videos/mathlib-bridge-math/). The films show ten selected supporting lemmas; the public corpus has since grown to seventeen. The newer totals below describe the repository checkpoint, not additional footage hidden in those recordings.

## Watch: for programmers

{% videoPlayer "mathlib-bridge", true %}

[Transcript and chapters](/docs/proofs/videos/mathlib-bridge/)

## Watch: for mathematicians

{% videoPlayer "mathlib-bridge-math", true %}

[Transcript and chapters](/docs/proofs/videos/mathlib-bridge-math/)

## Development oracle and independent operation

Lean provides pinned source, proof terms, reference execution and axiom reports for the compatible discipline. Osprey independently checks the imported evidence. Fresh Lean replay is an additional development check, not the implementation behind the Workbench's kernel verdict. See [the oracle relationship](/docs/proofs/#lean-as-an-oracle).

Known unsound oracle behaviour must become an Osprey rejection regression, never an acceptance requirement. Repairs to an implementation and changes to a theory or axiom policy have distinct version identities. A replacement or fork requires affected proofs and library dependencies to be checked afresh; withdrawing an assumption does not automatically reconstruct proofs that used it.

The [completion milestones](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion) separate full certificate coverage, readable source translation and reconstruction of every source proof. Independence requires authoring, checking, automation and a clean library rebuild with no Lean installation or service. That broader workflow remains delivery work; the working checked exports below are evidence of current capability.

## What already checks

| Corpus | Selected statements | GC, ARC and wasm32 evidence |
| --- | ---: | --- |
| Lean core | 175 | 525 successful root checks; 1,050 rejected proof and statement mutations |
| Recent OpenAI mathematics developments | 17 supporting lemmas | 51 successful root checks; 102 rejected mutations |
| Actual string-constructor proofs | 2 | 6 successful axiom-free checks; 12 rejected mutations |

The last fully closed kernel checkpoint passed 325 cases on each of GC, ARC and wasm32. The source/export checks reproduce pinned exports byte for byte, replay the compatible modules through Lean's fresh kernel, and compare each theorem's assumptions independently.

The core count includes 172 original exports and three compatible constructive proof bodies with the exact original statements. The three original certificates remain refused at a nested-inductive dependency. An alternative proof of the same statement does not imply the original certificate is supported.

The OpenAI selection includes eight infinite-stack lemmas, three statements about a doubling loop's exact value and work cost, a finite-index injectivity lemma and five integer five-colour orientation statements. These are supporting results from the released research developments. Their headline research theorems have not been ported by this corpus.

## A concrete example: a loop's work counter

The imported theorem `OAI.LowSpaceSubsetSum.WordSeedPowerExecution.run_work` states that, for every natural-number `width`, the second component of `run(width)` equals `3 × width + 1`. That component is an abstract work counter. For example, the equation gives 1 at width 0 and 13 at width 4. It does not measure seconds or promise a machine-specific runtime.

The proof establishes the equation for every natural-number width, rather than testing those two examples. Its dependency report includes propositional extensionality (`propext`): an assumption relating logically equivalent propositions. Keep that assumption alongside the result when inspecting the theorem. The [pinned source and export](https://github.com/Nimblesite/osprey/blob/main/examples/proof/openai/corpus.json) identify the exact statement and evidence being checked.

In the [Workbench](/docs/proof-workbench/), the second-component notation and arithmetic make the statement easier to read; the fully qualified function name and raw statement preserve its identity. This supporting lemma demonstrates imported mathematics with explicit assumptions. It does not establish the headline research theorem or verify an unrelated program.

## Why bring a library across?

A proof assistant needs reusable mathematics: definitions, established lemmas and the dependencies needed to build a larger argument. Importing checked proof terms gives Osprey a concrete compatibility target and exposes missing kernel rules with real examples. It also lets the Workbench present the actual theorem and its assumptions rather than relying on a familiar theorem name.

An exported certificate is evidence to check. Osprey reconstructs its dependency environment and checks the supplied term against its statement. A corrupt proof keeps the real statement but changes the evidence; a false-statement control keeps the evidence but replaces the claim. Both must be rejected by the kernel.

## Reproduce the public checks

From the repository root after building `target/release/osprey`:

```sh
python3 scripts/proof-openai.py --compiler target/release/osprey
python3 scripts/proof-openai.py --corpus lean-core --compiler target/release/osprey
python3 scripts/test-proof-literals.py
```

The first command expects 51 successful root checks, each accompanied by corrupted-proof and false-statement rejection. The second expects 525 successful root checks and 1,050 rejected mutations. Both perform source validation and fresh Lean replay; the pinned authoring toolchain is required. The Unicode command uses the three batch binaries produced by the preceding corpus checks.

For a smaller visible example, build the compiler and run:

```sh
target/release/osprey examples/proof/kernel --run --quiet --no-warnings --memory=gc
```

The output includes `checked: Nat.add_comm (export)` followed by `rejected: Nat.add_comm (export): type mismatch` for the deliberately corrupted proof. The rejection is an expected control. `--quiet` removes progress messages; it does not remove the program's checked/rejected results. Repeat with `--memory=arc` for the other managed backend.

## Keep assumptions visible

Five of the selected OpenAI roots are axiom-free, ten use propositional extensionality (`propext`), and two use quotient soundness (`Quot.sound`). These counts concern each selected theorem's transitive dependencies, not every axiom declared elsewhere in the export. Neither a Checked badge nor an unused-axiom-free environment should obscure an assumption a theorem actually needs.

The Workbench's bundled catalogue stages thirteen approved export groups with unchanged upstream licences. A group is not a theorem count. The app reads the current selected source and asks the shared kernel to check it; the catalogue does not store an acceptance flag.

## What remains

The full pinned Mathlib library, its readable Osprey frontend translation, broader elaboration and tactics, and the remaining nested-inductive certificates are still delivery work. The five newly integrated number-theory statements preserve the exact upstream statements with compatible proof bodies and verified provenance. Original nested-inductive certificates retain their own unmet acceptance gates. Checking library exports does not automatically prove an arbitrary Osprey application correct.

The Navier-Stokes films show measured numerical flow and a propositional Workbench exercise. They do not claim a formal PDE proof, a completed analysis library or a research breakthrough. Whole-library performance comparisons also remain separate gates.

## Inspect the evidence

- [Current delivery ledger and remaining work](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md)
- [OpenAI sources, exact statements, pins and licences](https://github.com/Nimblesite/osprey/blob/main/examples/proof/openai/corpus.json)
- [Lean-core corpus](https://github.com/Nimblesite/osprey/blob/main/examples/proof/lean-core/corpus.json) and [compatible proof manifest](https://github.com/Nimblesite/osprey/blob/main/examples/proof/lean-core/compatible.json)
- [Canonical source/export/replay checker](https://github.com/Nimblesite/osprey/blob/main/scripts/proof-openai.py)
- [Bundled Workbench library inventory](https://github.com/Nimblesite/osprey/blob/main/apps/workbench/assets/proof-libraries.json)

[Use the Workbench](/docs/proof-workbench/) or [choose a proof learning path](/docs/proofs/). Osprey is actively evolving.
