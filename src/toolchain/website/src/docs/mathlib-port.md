---
layout: page
title: Bringing Lean's proof checking to Osprey
description: "Osprey is building a Lean-compatible proof kernel, written in Osprey, so Lean proofs and Mathlib can be checked inside Osprey. 194 selected Lean statements check today; the first full Mathlib theorem does not yet."
permalink: /docs/mathlib-port/
---

{% from "native-capture.njk" import capture %}

**The goal is to give Osprey Lean's proof checking.** [Lean](https://lean-lang.org/) is a proof assistant: you state a theorem, write a proof, and Lean's small trusted *kernel* checks every step. [Mathlib](https://leanprover-community.github.io/mathlib-overview.html) is Lean's library of checked mathematics, from arithmetic to analysis. Osprey is building its own Lean-compatible kernel, written in Osprey, that checks the same proof terms Lean's kernel checks. When the work is complete you will be able to:

- check a Lean proof, or all of Mathlib, with Osprey's kernel on native (GC or ARC) and WebAssembly targets;
- author `.lean` proofs in the [Proof Workbench](/docs/proof-workbench/) and VS Code and get the kernel's verdict;
- use checked theorems alongside ordinary Osprey programs, with every assumption, such as `propext` or `Classical.choice`, kept visible.

The Mathlib port measures how far that has got: each Lean or Mathlib proof that Osprey's kernel accepts is a proof Osprey can check for itself.

## Where it stands

| Stage | Status |
| --- | --- |
| A Lean-compatible kernel in Osprey | Working. About 4,400 lines of Osprey under `src/osprey/examples/proof/kernel/`: terms, universes, inductive types, recursors, quotients, literals and reduction. |
| Selected Lean core and research statements | **194 check** on GC, ARC and wasm32, and every corrupted proof and false statement is rejected (table below). |
| The first full Mathlib theorem (Euclid's infinitude of primes) | **Not yet.** It imports, but the check does not finish (details below). |
| All of Mathlib | Not started. It follows once single theorems check. |
| Writing `.lean` proofs and checking them through Osprey (`osprey prove check`) | Not built yet. Stage 1 uses Lean to elaborate and Osprey's kernel to check. Stage 2 removes the Lean executable. |
| Running with no Lean installation at all | Not yet. Today Lean produces the exports, and fresh Lean replay is a development cross-check, not the source of the verdict. |

The [delivery plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md) lists each milestone and its acceptance test.

[Watch the programmer introduction](/docs/proofs/videos/mathlib-bridge/) or [the mathematics version](/docs/proofs/videos/mathlib-bridge-math/). The films show real imported proofs in the native and VS Code Workbench, including natural-number addition and the exact work counter of a doubling loop. The dated corpus totals below describe the wider repository checkpoint; the films do not claim Mathlib coverage.

## Watch: for programmers

{% videoPlayer "mathlib-bridge", true %}

[Transcript and chapters](/docs/proofs/videos/mathlib-bridge/)

## Watch: for mathematicians

{% videoPlayer "mathlib-bridge-math", true %}

[Transcript and chapters](/docs/proofs/videos/mathlib-bridge-math/)

## How a proof gets checked

Lean elaborates a proof (resolving tactics, notation and implicit arguments) into a plain proof term and exports it. Osprey rebuilds the theorem's dependency environment from an empty start, then checks that the term has exactly the stated type. The kernel decides; Lean's verdict is never copied in.

Every check runs with two controls. A **corrupted proof** keeps the real statement but changes the evidence. A **false statement** keeps the evidence but changes the claim. The kernel must reject both, so a kernel that accepted everything would fail. Known unsound behaviour in any checker becomes a mandatory Osprey rejection test.

{{ capture("/assets/images/guides/mathlib-check.png", "Recorded Mathlib tutorial: inspect a real check and its result; the surrounding guide distinguishes imported certificates from independent operation.", "Recorded Mathlib tutorial: inspect a real check and its result; the surrounding guide distinguishes imported certificates from independent operation.", 1920, 1080) }}

## What already checks

| Corpus | Selected statements | GC, ARC and wasm32 evidence |
| --- | ---: | --- |
| Lean core | 175 | 525 successful root checks; 1,050 rejected proof and statement mutations |
| Recent OpenAI mathematics developments | 17 supporting lemmas | 51 successful root checks; 102 rejected mutations |
| Actual string-constructor proofs | 2 | 6 successful axiom-free checks; 12 rejected mutations |

Together these are 194 selected statements: 582 successful root checks and 1,164 rejections, verified at the **30 October 2026** checkpoint, when the checker suite also passed all 400 kernel cases on each backend. Source changes since then need fresh validation.

- The Lean-core count includes three compatible constructive proofs of the original statements. The three original certificates are still refused at a nested-inductive dependency (`Lean.Syntax`), and an alternative proof does not count as support for the original.
- The OpenAI lemmas are supporting results (infinite stacks, a doubling loop's exact work cost, finite-index injectivity, five-colour orientation), not the headline research theorems. Five use no axioms, ten use `propext` and two use `Quot.sound`.

## Where the Mathlib import stops

The pinned Mathlib commit is `1ccd71f89cbbd82ae7d097723ce1722ca7b01c33` on Lean 4.25.0. The first target is `Nat.exists_infinite_primes`: for every natural number `n` there is a prime `p` with `n ≤ p`.

These figures were measured on 30 October 2026. Their receipt files were working-tree files under `target/proof/` and are not kept in the repository.

- The export is 12,027,058 bytes: 230,909 rows, 19,518 names and 1,551 theorems in the dependency closure.
- Osprey's importer parses all 2,833 declarations in 11.1 seconds.
- Fresh Lean replay of the same module passes in 424 seconds.
- **Osprey's own check does not finish.** GC and ARC both hit the 180-second limit. ARC first crashed on stack depth during recursive weak-head reduction, and a bounded-stack repair removed the crash. The trace stops at `Nat.sqrt.iter.eq_def`.
- That declaration on its own (a 440,178-byte export) checks under ARC in 45.4 seconds, reporting `propext` and `Quot.sound`.

The next step is to make reduction fast enough for the whole Euclid closure on all three backends. After that come the rest of the pinned library, the three refused nested-inductive certificates, and stage 1 authoring.

## Try it

For a small visible example, build the compiler and run:

```sh
target/release/osprey src/osprey/examples/proof/kernel --run --quiet --no-warnings --memory=gc
```

The output includes `checked: Nat.add_comm (export)`, then `rejected: Nat.add_comm (export): type mismatch` for the deliberately corrupted control. Repeat with `--memory=arc` for the other managed backend.

To reproduce the corpus checks from the repository root (they need the pinned Lean toolchain for source validation and fresh replay):

```sh
python3 scripts/proof/proof-openai.py --compiler target/release/osprey
python3 scripts/proof/proof-openai.py --corpus lean-core --compiler target/release/osprey
python3 scripts/proof/test-proof-literals.py
```

The first command expects 51 successful root checks, each accompanied by corrupted-proof and false-statement rejection. The second expects 525 successful root checks and 1,050 rejected mutations. The Unicode command uses the three batch binaries produced by the preceding corpus checks and expects six successful root checks with twelve rejections.

{{ capture("/assets/images/guides/mathlib-library.png", "Recorded Mathlib tutorial: browse the library and retain the displayed assumptions and verification context.", "Recorded Mathlib tutorial: browse the library and retain the displayed assumptions and verification context.", 1920, 1080) }}

## What this does not claim

- Checking imported proofs does not prove an arbitrary Osprey application correct, and an imported certificate is not a proof written in Osprey.
- The kernel's small built-in proof surface (identity, implication transitivity, a universal statement) is an exercise. Authored proofs are planned as `.lean` files.
- The Navier-Stokes films show measured numerical flow, the OpenAI solution published on 8 September 2026 and a propositional Workbench exercise. OpenAI's internal system produced an analytical proof, formalized in Lean, that a smooth fluid initially at rest under a smooth force develops a finite-time singularity with finite energy ([source](https://openai.com/index/navier-stokes-solution/)). It is a forced result, not a general unforced blowup claim. Osprey checks seventeen supporting lemmas from pinned `openai/math` sources, not that proof.

## Inspect the evidence

- [Delivery plan and milestones](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md)
- [OpenAI sources, exact statements, pins and licences](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/proof/openai/corpus.json)
- [Lean-core corpus](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/proof/lean-core/corpus.json) and [compatible proof manifest](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/proof/lean-core/compatible.json)
- [Source/export/replay checker](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/proof-openai.py)
- [Kernel source](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/proof/kernel/src)

[Use the Workbench](/docs/proof-workbench/) or [choose a proof learning path](/docs/proofs/).
