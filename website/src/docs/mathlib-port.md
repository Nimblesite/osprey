---
layout: page
title: The Mathlib port is underway
description: "Where Osprey's proof-library port actually stands: 194 selected Lean statements checked, an actual Mathlib module being imported, and the reductions that still fail."
permalink: /docs/mathlib-port/
---

Mathlib is the Lean community's library of formalized mathematics: definitions, lemmas and proofs that later work can reuse instead of rebuilding them. Think of it as reusable mathematics with evidence attached, before any notation or tooling matters.

Osprey is a functional language built for safety, elegance and performance: algebraic effects and `Result` instead of panics, inferred types, and native LLVM compilation. It already checks real imported proofs with a kernel written in Osprey, and the library work is underway. This page keeps two things apart: what passed at the verified checkpoint of **30 October 2026**, and what the current Mathlib import work has and has not achieved.

[Watch the programmer introduction](/docs/proofs/videos/mathlib-bridge/) or [the mathematics version](/docs/proofs/videos/mathlib-bridge-math/). The films show real imported proofs in the native and VS Code Workbench, including natural-number addition and the exact work counter of a doubling loop. The loop proof names its `propext` assumption. The dated corpus totals below describe the wider repository checkpoint; these demonstrations do not claim full Mathlib coverage.

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

Together these three published commands cover 194 selected statements: 582 successful root checks and 1,164 rejections, since every root is also run against a corrupted proof and a false statement on each backend.

The 30 October checker-suite checkpoint passes all 400 kernel cases on each of GC, ARC and wasm32, with no failed or skipped cases and unchanged input hashes. It includes the standard-choice guard and numeric short-circuit correction. Fresh public-corpus replay also passes all 582 root checks and their 1,164 proof/statement corruption controls across the 194 selected statements. Later source changes require fresh validation. Acceptance of the complete Euclid export below remains separate. The [plan ledger](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md) records the dated checkpoints and their receipts.

The source/export checks reproduce pinned exports byte for byte, replay the compatible modules through Lean's fresh kernel, and compare each theorem's assumptions independently.

The core count includes 172 original exports and three compatible constructive proof bodies with the exact original statements. The three original certificates remain refused at a nested-inductive dependency. An alternative proof of the same statement does not imply the original certificate is supported.

The OpenAI selection includes eight infinite-stack lemmas, three statements about a doubling loop's exact value and work cost, a finite-index injectivity lemma and five five-colour orientation statements from the existing NumberTheory project. For those five, the original statements are compared against the retained certificate and the three `omega` proof bodies are replaced with core integer lemmas; each reports `propext`. These are supporting results from the released research developments. Their headline research theorems have not been ported by this corpus.

## Five things this page keeps apart

- **Lean-core statements (175).** Lean's own definitions and proofs, checked from an empty Osprey environment on all three backends.
- **Recent OpenAI supporting lemmas (17).** Supporting statements from released research developments, checked the same way; not their headline theorems.
- **Actual Mathlib source exports (in progress).** One selected Mathlib theorem is being imported at library scale. It is not accepted yet; the section below says exactly where it stops.
- **Readable source translation.** Imported-term checking and translation are separate. A small Osprey proof surface exists, but no Mathlib source file has a readable, checked Osprey translation yet; the figures on this page are imported certificates, not translations.
- **Proofs written in Osprey.** Imported-term checking and source elaboration are separate. The kernel accepts small proofs written in Osprey's own ML proof surface (identity, implication transitivity, a universal statement) and refuses a deliberately wrong one. That is authored proof checking at exercise scale, not library coverage.

## Where the actual Mathlib import stands

Everything before this section is checked evidence. This section is current work, recorded so the gap is visible rather than implied.

The pinned Mathlib commit is `1ccd71f89cbbd82ae7d097723ce1722ca7b01c33` on the Lean 4.25.0 toolchain. The selected theorem is `Nat.exists_infinite_primes`: for every natural number `n` there is a prime `p` with `n ≤ p` — Euclid's theorem that the primes never run out.

What works today:

- The export is pinned: 12,027,058 bytes with SHA-256 `97a2e930…`, produced by the pinned exporter from the pinned Mathlib commit. It holds 230,909 rows, 19,518 names and 1,551 theorems in the dependency closure, with `propext`, `Quot.sound` and `Classical.choice` declared.
- Osprey's importer parses all 2,833 declarations in 11.1 seconds under GC.
- Fresh Lean replay of the same module passes in 424 seconds.

What does not work yet: Osprey's own check of that export does not finish. The original GC probe hit the 180-second limit and ARC exhausted the machine stack after 36.7 seconds. After the bounded-stack repair, a fresh ARC probe also reaches the 180-second limit without crashing (`euclid-fixed-arc-receipt.json`). The diagnostic trace reaches `Nat.sqrt.iter.eq_def` last, and the original crash frames were recursive weak-head reduction, not parsing. The importer works; completing the full check remains active work.

The work has been narrowed. A focused export of `Nat.sqrt.iter.eq_def` alone — 440,178 bytes — now checks under ARC in 45.4 seconds, reporting `propext` and `Quot.sound`, with its own fresh Lean replay in 70 seconds. A bounded-stack reduction machine is in the tree and under validation. This establishes a focused ARC check. Acceptance of the full Euclid certificate and a closed three-backend matrix remain pending.

Full-library coverage, readable source translation and the complete independent authoring workflow remain the milestones linked above. The receipts for these measurements are working-tree files under `target/proof/mathlib-v4.25.0/` (not committed), and their pins and status are recorded in the plan ledger.

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

The first command expects 51 successful root checks, each accompanied by corrupted-proof and false-statement rejection. The second expects 525 successful root checks and 1,050 rejected mutations. Both perform source validation and fresh Lean replay; the pinned authoring toolchain is required. The Unicode command uses the three batch binaries produced by the preceding corpus checks and expects six successful root checks with twelve rejections.

For a smaller visible example, build the compiler and run:

```sh
target/release/osprey examples/proof/kernel --run --quiet --no-warnings --memory=gc
```

The output includes `checked: Nat.add_comm (export)` followed by `rejected: Nat.add_comm (export): type mismatch` for the deliberately corrupted proof. The rejection is an expected control. `--quiet` removes progress messages; it does not remove the program's checked/rejected results. Repeat with `--memory=arc` for the other managed backend.

## Keep assumptions visible

Five of the selected OpenAI roots are axiom-free, ten use propositional extensionality (`propext`), and two use quotient soundness (`Quot.sound`). These counts concern each selected theorem's transitive dependencies, not every axiom declared elsewhere in the export. Neither a Checked badge nor an unused-axiom-free environment should obscure an assumption a theorem actually needs. The five five-colour orientation statements each report `propext` and are included in that count.

The Workbench's bundled catalogue stages thirteen approved export groups with unchanged upstream licences. A group is not a theorem count. The app reads the current selected source and asks the shared kernel to check it; the catalogue does not store an acceptance flag.

## What remains

The actual Mathlib import described above must be checked rather than parsed and replayed, beginning with the reduction depth that stops `Nat.exists_infinite_primes`. Then come the full pinned library, a readable Osprey frontend translation, broader elaboration and tactics, and the three original nested-inductive certificates that remain refused even though compatible proofs of their statements check. Whole-library performance comparisons remain separate gates.

Checking library exports does not automatically prove an arbitrary Osprey application correct, and an imported certificate is not a proof written in Osprey.

The Navier-Stokes films show measured numerical flow, the OpenAI solution published on 8 September 2026 and a propositional Workbench exercise. OpenAI's internal system produced an analytical proof, formalized in Lean, that a smooth fluid initially at rest under a smooth force develops a finite-time singularity with finite energy ([source](https://openai.com/index/navier-stokes-solution/)); it is a forced result, not a general unforced blowup claim. Separately, Osprey checks seventeen selected lemmas about stacks, loop work, finite indices and number theory from pinned `openai/math` sources, not the headline Navier–Stokes proof or a complete analysis library.

## Inspect the evidence

- [Current delivery ledger and remaining work](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md)
- [OpenAI sources, exact statements, pins and licences](https://github.com/Nimblesite/osprey/blob/main/examples/proof/openai/corpus.json)
- [Lean-core corpus](https://github.com/Nimblesite/osprey/blob/main/examples/proof/lean-core/corpus.json) and [compatible proof manifest](https://github.com/Nimblesite/osprey/blob/main/examples/proof/lean-core/compatible.json)
- [Canonical source/export/replay checker](https://github.com/Nimblesite/osprey/blob/main/scripts/proof-openai.py)
- [Bundled Workbench library inventory](https://github.com/Nimblesite/osprey/blob/main/apps/workbench/assets/proof-libraries.json)

[Use the Workbench](/docs/proof-workbench/) or [choose a proof learning path](/docs/proofs/). Osprey is actively evolving.
