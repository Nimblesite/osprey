# Plan 0042: the proof workbench

Contract: [spec 0051](../specs/applications/0051-ProofWorkbench.md) · [spec 0052](../specs/applications/0052-WorkbenchLessons.md)

Depends on: [plan 0041](0041-dependent-types-and-mathlib.md) phase 0 (delete the `.ospml`/English proof formats), phase 1 (toolchain stage: `osprey prove check`, the `Lean` capability, the `.lean` document), phase 3 (lessons in format 2).

## Prerequisites in place

- The `feat/type-disciplines` PR merged (`ca35a22a`); the workbench's document is `.lean` ([PROOF-SOURCE-LEAN](../specs/language/0055-ProofKernelAndLeanBridge.md#lean-source-proof-source-lean-files)).
- `@osprey/native` already ships `Maths::decode`, `Media` `Image`/`Video` with encodings, and the `Run`/`SaveFile` capabilities.
- The three lesson films, posters, captions and image stills exist under `src/osprey/workbench/lessons/media/` and meet the [LESSONS-MEDIA] budget; narration re-verification is the remaining media task.
- `src/osprey/workbench/assets/proof-libraries.json` and `scripts/proof/workbench-proof-assets.py` stage the bundled exports and licences.
- Visual oracle: [Paper & Slate handoff](../designs/proof-workbench/README.md) and its screens; screens that showed the deleted document are re-captured before they bind.

## Constraints

- The workbench reads and writes only `.lean` as proof text [WORKBENCH-AIM]; it never reads or writes English as proof [WORKBENCH-NO-ENGLISH].
- The application never parses, elaborates, judges or generates proof text: Lean and the kernel decide, the application displays.
- A host draws and completes capabilities; it never filters, labels, validates or decides.
- No service exists that only the workbench can call [WORKBENCH-NO-SPECIAL].
- The React study under `docs/designs/` is a visual reference, never a second implementation.
- A lesson stores no answer key the checker did not confirm; no proof text, TeX, HTML or MathML in a lesson JSON; prerequisites are advice; films never autoplay and always carry captions; the Navier–Stokes lesson never claims the problem is solved [LESSONS-CURRICULUM].
- No existing gate, test, threshold or check is relaxed, skipped or narrowed to fit this plan.
- This plan edits no document owned by another plan (plan 0041, spec 0051, spec 0052, the indexes).
- Delegated work goes to pi only against a skeleton and a test written here first; a checker-refused solution, a weakened test, or a framework contract bent to fit a test is thrown out.

## Phase 1 — the `.lean` document and checking [WORKBENCH-DOCUMENT] [WORKBENCH-CHECK] [WORKBENCH-VERDICT] [WORKBENCH-STORE] [WORKBENCH-REGIONS]

- [ ] `src/osprey/workbench/src/document.ospml` `Document`: text + cursor, open/save as bytes, no parsing; byte-identical round trip over every `.lean` fixture in `src/toolchain/benchmarks/proof/tools/` (`AndSwap.lean`, `OpenAIStacks.lean`, …). Accept: open→save equals the fixture bytes.
- [ ] `src/osprey/workbench/src/lean.ospml` `Lean`: `LeanCheck`/`LeanGoals` requests carrying a revision ticket; a completion for an older revision is dropped; the Unavailable completion when no toolchain.
- [ ] `src/osprey/workbench/src/verdict.ospml` `Verdicts::decide`: kernel verdict over the completion's export through `ImportedProofs`; per-declaration `Checked`/`Rejected`/`Unchecked`.
- [ ] `src/osprey/workbench/src/app.ospml` `App` and `src/main.ospml` entry: slices `Text`/`Diagnostics`/`Verdicts`/`Library`/`Layout`, the action union, pure `update`, the five regions `root`/`document`/`goal`/`read`/`library`, theme.
- [ ] Fixture tests in `src/osprey/workbench/tests/`: a keystroke refreshes `document`+`goal` only; stale ticket dropped; Lean error underlines Lean's range; corrupted export `Rejected`; `sorry` `Unchecked`; paper/chalk contrast.
- [ ] Exit: `osprey src/osprey/workbench --check` reports every export documented; no host yet.

## Phase 2 — library and the paper [WORKBENCH-LIBRARY] [WORKBENCH-MATH] [WORKBENCH-PAPER]

- [ ] `src/osprey/workbench/src/library.ospml` `Library::search`: over the bundled exports in `assets/proof-libraries.json` first (re-checked in-process by `ImportedProofs` from an empty environment, corrupted proofs refused, per-theorem axioms shown), then the checked Mathlib environment; name and statement-shape search; hover shows the statement as `Math` plus docstring; a hit inserts `exact <name>`/`apply <name>`.
- [ ] `src/osprey/workbench/src/proofterm.ospml` `ProofTerms`: present a checked statement through `Math` with standard notation only when the exact declarations behind it are present and structurally compared; otherwise the explicit constant.
- [ ] Reading view (`read` region): each declaration as a page — statement as display math, proof as tactic lines with the margin goal after each, verdict beside the heading, docstrings as prose; `Exported` writes HTML+MathML through the same `Math` tree; `Saved` writes the `.lean` file.

## Phase 3 — hosts [WORKBENCH-HOSTS] [WORKBENCH-VSIX] [WORKBENCH-DESIGN]

- [ ] VS Code: compile to WASI through `@osprey/vscode`; renderer draws `Math` (MathML) and the marked `Editor`; the `Lean` capability runs the toolchain the extension finds or installs; bundle `workbench.wasm`, lessons and staged exports; packaging rejects missing/invalid/stale/extra content; E2E opens `AndSwap.lean` and sees `Checked`; screens re-captured and compared.
- [ ] Browser: standalone page under `src/osprey/workbench/hosts/web/` reusing the same renderer; `Lean` Unavailable until plan 0041 milestone 3 stage 2; headless smoke test.
- [ ] macOS/iPadOS: SwiftUI host under `src/osprey/workbench/hosts/apple/` with a `Math` layout view; toolchain bundled on macOS, Unavailable on iPadOS until stage 2; Apple-simulator smoke test.
- [ ] Decide: `what-is-a-solver` has no prove challenge, which [LESSONS-CURRICULUM](../specs/applications/0052-WorkbenchLessons.md#the-curriculum-lessons-curriculum) requires unless noted — add one or note the exception in spec 0052's table.
- [ ] Road test recorded in [plan 0041 evidence](0041-dependent-types-and-mathlib.md#evidence): a newcomer writes and checks `and_swap` in a fresh `.lean` file in under ten minutes.

## Phase 4 — lessons as data, judged by Lean and the kernel (with plan 0041 milestone 3)

- [ ] `src/osprey/workbench/src/lesson.ospml` `Lessons::read`: `Lesson`/`Block`/`Challenge` over format 2 (`.lean` + `<slug>.json`); every refusal tested (unknown `kind`, missing field, bad slug, unsafe media path, unknown `decl`, `format` 1); `decl` references resolved against the lesson's `.lean` [LESSONS-FORMAT].
- [ ] `src/osprey/workbench/src/judge.ospml` `Judge`: `prove` builds the `.lean` document (preamble + statement signature + `by` + given + answer) and issues the check; `state` elaborates and asks `Reduce::defeq`; `judge` checks the named declaration; each compares a ticketed completion [LESSONS-PROVE] [LESSONS-STATE] [LESSONS-JUDGE] [LESSONS-FEEDBACK].
- [ ] `src/osprey/workbench/src/tutor.ospml` `Tutor`: attempts, hints after n wrong attempts, reveal marks seen-not-right, clearing on edit, the completed set [LESSONS-FEEDBACK] [LESSONS-PROGRESS].

## Phase 5 — the curriculum [LESSONS-CURRICULUM] [LESSONS-INTEGRITY] [LESSONS-MEDIA]

- [ ] `src/osprey/workbench/lessons/catalogue.json` (format 2) and seven `<slug>.lean` + seven `<slug>.json` files; each lesson ≥2 challenges and ≥1 prove challenge except where noted; teach Lean's words (`intro`, `cases … with`, `constructor`, `exact`, `left`, `right`, `absurd`) with a one-line English explanation beside each.
- [ ] Integrity suite in `src/osprey/workbench/tests/` over every shipped lesson with the toolchain present: `.json` reads; slug/`after`/challenge ids valid; every referenced `decl` exists; the `.lean` checks under `osprey prove check` with each `statement` `Unchecked` (body `sorry`), each `solution`/`expected` `Checked`, each judge `proof` `Checked` or `Rejected`; every `lean` `source` block elaborates; media paths valid and files exist.
- [ ] Re-verify the three films' narration and captions say only what the rewritten lessons say; re-record only where they diverge; keep ≤2 MB, ≤60 s, 1920×1080 H.264+AAC, WebVTT captions, JPEG posters; image stills ≤400 KB.

## Phase 6 — Learn mode in the application [LESSONS-MODE] [LESSONS-STORE]

- [ ] `Shelf`/`Tutor`/`Achieved` slices and the messages of [LESSONS-STORE] (`ToLearn`, `ToWork`, `Open slug`, `Answer id`, `FormulaPlace id`, `Key id text`, `Pick id`, `Submit id`, `Hint id`, `Reveal id`, `Carry id`); regions `catalogue`, `lesson`, `challenge:<lesson>/<id>`.
- [ ] Load `catalogue.json` and lesson files through `ReadTexts` into host-held `lessons:catalogue`/`lessons:texts`; progress through `StoreRead`/`StoreWrite` to `lessons/progress.json` [LESSONS-FILES] [LESSONS-PROGRESS].
- [ ] Fixture tests through real `start`/`dispatch`: a keystroke re-sends only that `challenge:<lesson>/<id>` region; every submit result (`Correct`/`Incorrect line`/`Unfinished reason`); stale completion dropped; keypad caret and focus epoch; progress round trip; **Open in the workbench** produces the `.lean` document [LESSONS-VERIFICATION].

## Phase 7 — lesson hosts

- [ ] Renderer draws `Image` and `Video` (never autoplay); VS Code E2E opens `what-is-a-proof` and answers its first challenge to a "Right" badge.
- [ ] Browser and Apple hosts with the same lesson flow, after Phase 3.

## Mathematics in, Osprey out

Delivered by [plan math-to-code](math-to-code.md) under [MATH-CODE](../specs/tooling/MathToCode.md): the source is a `.lean` document, the toolchain generates the module, and this application only displays it ([MATH-CODE-WORKBENCH]).

## Later (after plan 0041 milestone 3 stage 2)

- [ ] Code actions (`cases`, `induction`, `constructor`, `exact?`) through Lean's server.
- [ ] Browser and iPadOS verdicts.
