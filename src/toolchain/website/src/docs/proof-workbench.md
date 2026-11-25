---
layout: page
title: Use the Osprey Proof Workbench
description: Open the Proof Workbench in VS Code or on macOS, follow a small proof, inspect hypotheses and goals, and complete guided lessons.
permalink: /docs/proof-workbench/
---

The Proof Workbench lets you build an argument and see what each step establishes. The document holds your statement and working; the goal pane shows your assumptions and what is still needed. A Checked verdict requires the shared Osprey kernel to accept the proof.

[Watch the programmer walkthrough](/docs/proofs/videos/proof-workbench/) or [the mathematics walkthrough](/docs/proofs/videos/proof-workbench-math/). Both start with the problem before introducing symbols.

The films show the layout recorded during production, including some freehand board controls. The supported proof workflow uses typed formulas and proof steps; these videos do not demonstrate mathematical handwriting recognition.

Lean is the [development oracle](/docs/proofs/#lean-as-an-oracle) for compatible proof behaviour; Osprey's own kernel makes the Workbench's decisions. A known unsound certificate must be refused. Changing a discipline or its assumptions requires fresh checking, so a saved Checked verdict cannot transfer automatically to a different theory.

## Watch: for programmers

{% videoPlayer "proof-workbench", true %}

[Transcript and chapters](/docs/proofs/videos/proof-workbench/)

## Watch: for mathematicians

{% videoPlayer "proof-workbench-math", true %}

[Transcript and chapters](/docs/proofs/videos/proof-workbench-math/)

## Open the Workbench

In a build of the Osprey VS Code extension containing the Workbench, open the Command Palette and run **Osprey: Open Workbench**. No project is required. This hosts the shared Osprey application inside VS Code; it is not a screenshot viewer. The command identifier is `osprey.workbench.open`.

From the repository on an Apple silicon Mac with the compiler and Xcode toolchain installed, build and open the native host:

```sh
OSPREY_BIN="$PWD/target/release/osprey" bash src/osprey/workbench/hosts/apple/run.sh
```

The native and VS Code hosts share the Osprey proof model and kernel. Their rendering and platform capabilities have separate acceptance checks. Follow [installation](/docs/installation/) to build the compiler; consult the [Workbench source guide](https://github.com/Nimblesite/osprey/tree/main/src/osprey/workbench) for current host build and packaging status.

## Write mathematics, never English

The Workbench reads mathematics in notation only. Type `p ∧ q → q ∧ p`, or use the formula keys and backslash abbreviations such as `\and` and `\to`. An English sentence such as `p implies q` is not read as mathematics: the kernel reports it as unfinished. The Workbench may show a proof in words on screen, but it never reads, saves or exports English as a proof.

## Work through lessons

Open **Lessons**, choose a lesson and select **Open**. Read its introduction before answering a challenge. Write the requested formula in notation and submit it; the kernel decides whether it is the same proposition. Hints appear after unsuccessful submissions.

The Navier-Stokes lesson introduces velocity, pressure and incompressibility before showing the equations, then credits OpenAI's published solution: on 8 September 2026 an internal system produced an analytical proof, formalized in Lean, that a smooth fluid initially at rest under a smooth force develops a finite-time singularity while its energy stays finite ([source](https://openai.com/index/navier-stokes-solution/)). That is a forced finite-time blowup, not a claim about every unforced flow. Separately, Osprey checks seventeen selected lemmas about stacks, loop work, finite indices and number theory from the pinned `openai/math` release, not the headline Navier–Stokes proof. The lesson's exercise concerns explicit propositions and does not formalize the fluid analysis.

## Read an imported theorem

**Library** shows the bundled proof libraries. Choose one, then select a theorem. The app checks its current export through the shared kernel and displays the actual mathematical statement and the assumptions used by that theorem. For example, `Nat.add_comm` states universal natural-number addition commutativity and has no axioms in its recorded dependency closure.

The [Mathlib progress guide](/docs/mathlib-port/) explains the corpus, provenance and rejection checks. An imported proof and a newly authored Workbench proof are different workflows.

Continue with the [complete proof learning paths](/docs/proofs/) or read the [Workbench contract](/spec/0051-proofworkbench/). Osprey is actively evolving.
