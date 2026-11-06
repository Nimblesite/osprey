# Plan 0042: the proof workbench

Contract: [spec 0051](../specs/applications/0051-ProofWorkbench.md) · [spec 0052](../specs/applications/0052-WorkbenchLessons.md) · [FormulaCodeGeneration](../specs/applications/FormulaCodeGeneration.md)

Depends on: [plan 0041](0041-dependent-types-and-mathlib.md) phase 0 (delete the `.ospml`/English proof formats), phase 1 (toolchain stage: `osprey prove check`, the `Lean` capability, the `.lean` document), phase 3 (lessons in format 2).

## Prerequisites in place

- The `feat/type-disciplines` PR merged (`ca35a22a`); the `.lean`-only direction of [PROOF-SOURCE](../specs/language/0049-DependentTypesAndProofs.md#proof-source-proof-source) is in force.
- `@osprey/native` already ships `Maths::decode`, `Media` `Image`/`Video` with encodings, and the `Run`/`SaveFile` capabilities.
- The three lesson films, posters, captions and image stills exist under `src/osprey/workbench/lessons/media/` and meet the [LESSONS-MEDIA] budget; narration re-verification is the remaining media task.
- `src/osprey/workbench/assets/proof-libraries.json` and `scripts/proof/workbench-proof-assets.py` stage the bundled exports and licences.
- Visual oracle: [Paper & Slate handoff](../designs/proof-workbench/README.md) and its screens; screens that showed the deleted document are re-captured before they bind.

## Constraints

- Only `.lean` is read or written as proof text [PROOF-SOURCE-DELETED].
- The application never parses, elaborates, judges or generates proof text: Lean and the kernel decide, the application displays.
- A host draws and completes capabilities; it never filters, labels, validates or decides.
- No service exists that only the workbench can call [WORKBENCH-NO-SPECIAL].
- The React study under `docs/designs/` is a visual reference, never a second implementation.
- A lesson stores no answer key the checker did not confirm; no proof text, TeX, HTML or MathML in a lesson JSON; prerequisites are advice; films never autoplay and always carry captions; the Navier–Stokes lesson never claims the problem is solved [LESSONS-CURRICULUM].
- Formula code generation never solves an equation, relation, root, solver or binder form; never picks a precision, policy, branch or rounding on the user's behalf; never emits a placeholder, fabricated default, panic, `unwrap`-like builtin or bare integer operator; never marks a document verified because it typechecked [WORKBENCH-CODEGEN-LIMITS] [WORKBENCH-CODEGEN-EQUIVALENCE].
- Formula code generation claims no stdin `check` today; a needed compiler-side addition is specified and planned separately.
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
- [ ] Road test recorded in `../reviews/review.md`: a newcomer writes and checks `and_swap` in a fresh `.lean` file in under ten minutes.

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

## Phase 8 — formula code generation: entry gate [WORKBENCH-CODEGEN-ENTRY-GATE]

Checked on the day work begins; recorded as gate evidence before Phase 9 opens:
- [ ] `src/osprey/workbench/tests` passes under GC, ARC and wasm32, with no skips and no retained red test hidden by an exclusion.
- [ ] The `src/osprey/workbench` coverage gate in `src/osprey/tests/coverage-thresholds.json` passes at the 90% floor, not below the 90.8% baseline recorded in [plan 0041](0041-dependent-types-and-mathlib.md).
- [ ] `make ci` green on `main`; no gate in `../../AGENTS.md` red, waived or narrowed.
- [ ] The application owner records that `src/osprey/workbench` accepts new modules; the open VSIX, screenshot and imported-proof acceptance items of Phases 1–7 are closed or explicitly unaffected by a second document kind.
- [ ] The `src/osprey/packages/native` contracts this feature uses — `Run`, `SaveFile`, slices, tickets, `Math` — are the shipped ones; no host adapter needs a new framework component.

## Phase 9 — terms and the definition reader [WORKBENCH-CODEGEN-DOCUMENT] [WORKBENCH-CODEGEN-TERMS]

- [ ] `src/osprey/workbench/src/terms.ospml`: `Expr`, `Arm`, `Guard`, `Comparison`, the closed `Call` table, `Terms::show` (minimal parentheses, one precedence climber), `Terms::typeset` (builds `@osprey/native` `Math` nodes only: `Root`/`Frac`/`Sup`/`Op`/`Seq`), canonical form.
- [ ] `src/osprey/workbench/src/definition.ospml`: `Function`/`Suppose`/`Constant`/definition/piecewise sentences; `Definitions::read : string -> Reading` returns `Read Document` or `Faulted Fault` with a code-point column and fix sentence; every refusal code of [WORKBENCH-CODEGEN-REFUSALS](../specs/applications/FormulaCodeGeneration.md#refusals-are-neutral-workbench-codegen-refusals).
- [ ] `src/osprey/workbench/tests/terms.ospml`, `tests/definition.ospml`: generated-document corpus, `read (show d) = d` round trip, code-point column cases, one minimal near-miss per refusal code.

## Phase 10 — decisions and the emitter [WORKBENCH-CODEGEN-UNDERDETERMINATION] [WORKBENCH-CODEGEN-DOMAINS] [WORKBENCH-CODEGEN-NUMERICS] [WORKBENCH-CODEGEN-EMIT]

- [ ] `src/osprey/workbench/src/lower.ospml`, decision by decision with a test each, in the contract's decision-table order; no default for any row; full finite binary64 literal range (17-digit round trips, subnormals); conversion overflow/underflow follow the refusal rules.
- [ ] `src/osprey/workbench/src/generate.ospml`: `Generate::document`, ML flavor, four-space layout, no redundant annotations, no `?:`, no bare integer `+ - *`, no placeholder; the line map is produced by the same traversal that writes each line.
- [ ] `src/osprey/workbench/tests/golden/*.ospml.expected`: one byte-identical golden per fixture shape (plain real, guarded real, integer wrapping/saturating/checked, piecewise, multi-input), each generated twice per run (determinism); the `and_swap` proof golden and its emitter are untouched.

## Phase 11 — statuses, receipts, invalidation [WORKBENCH-CODEGEN-STATUS] [WORKBENCH-CODEGEN-PROVENANCE]

- [ ] `src/osprey/workbench/src/codestatus.ospml`: the four statuses, the receipt fields, hashes over canonical IR and exact bytes (the shipped hashing implementation, not a second one), invalidation on every field of the [WORKBENCH-CODEGEN-STATUS](../specs/applications/FormulaCodeGeneration.md#statuses-and-invalidation-workbench-codegen-status) table; the test enumerates every field and asserts no field clears nothing.
- [ ] Ticket comparison: a completion whose bytes hash is not current is dropped without touching state (state byte-identical before and after).
- [ ] `EquivalenceProved` is unreachable and displayed as unavailable, never an empty success [WORKBENCH-CODEGEN-EQUIVALENCE].

## Phase 12 — the validation boundary [WORKBENCH-CODEGEN-STATUS]

- [ ] Record a decision on how the exact generated bytes reach the real compiler on a host that has one (host-owned private temporary file written from a slice; a separately planned compiler-side addition; or no validation on a host with no compiler), and the authority each adds.
- [ ] Implement and test the unavailable path first: no capability → no badge, one explanation.
- [ ] If the chosen option needs a compiler-side addition, specify and plan it separately; this phase adds none silently.

## Phase 13 — application integration [WORKBENCH-CODEGEN-APP]

- [ ] Store and update: the kind discriminator, the `Definitions` and `Program` slices, the messages (`Edited`, `Moved cursor`, `Chose symbol`, `Requested check`, `Saved`, `Exported`, `Forked`), derived values rebuilt inside `update`; reading/lowering/generation run on the same keystroke; extend the region-refresh case to prove a keystroke refreshes `definition`, `preview` and `source` and nothing else.
- [ ] Workview/app: the four regions (`definition`, `preview`, `source`, `receipt`), the states table, source pane closed initially, receipt behind a disclosure [WORKBENCH-CODEGEN-STATES].
- [ ] `src/osprey/workbench/src/notation.ospml`: additive rows for arithmetic glyphs and abbreviations only; two-direction tests for every row; `Notation::expand` stays total and idempotent [WORKBENCH-NOTATION].
- [ ] Caret insertion and selection replacement over multi-byte symbols; the preview typeset from the same IR.
- [ ] Fixture cases through real `start`/`dispatch`: entry, refresh isolation, every state, export through the capability boundary and back into the document.

## Phase 14 — hosts [WORKBENCH-CODEGEN-APP]

- [ ] VS Code: the compiled application in the webview performs one accepted flow (type, preview, source, export) and one refused flow (relation, no source, text preserved); highlight roles and diagnostic-to-formula mapping asserted.
- [ ] Browser: same build as a page; `Generated` available, `Typechecked`/`Tested` unavailable with one explanation; headless smoke test.
- [ ] Native: the Apple host completes the same accepted/refused and numeric-error interactions with screenshot and exported-byte evidence; other native hosts follow only inside their existing gates; no second generator in any host language.

## Phase 15 — hardening

- [ ] The acceptance matrix below runs in full under GC, ARC and wasm32.
- [ ] Performance budgets measured and recorded as measurements [WORKBENCH-CODEGEN-PERFORMANCE].
- [ ] The refusal vocabulary is closed: a new code without a test and a contract change fails the suite.
- [ ] Spec/plan index registration is done by their owners; this plan does not edit them.

## Acceptance matrix [WORKBENCH-CODEGEN-MATRIX]

Every row is a required case. Rows marked ★ fail if the feature is built carelessly; the phase that introduces a rule owns the case that tests it.

| # | Case | Assertion |
| --- | --- | --- |
| 1 | Reading corpus | every sentence row reads; `read (show d) = d` |
| 2 | Refusal coverage | one near-miss per code; the code, column and fix are exact |
| 3 | Code-point columns ★ | diagnostics land on the right character across `π`, `≥`, `⁻¹`, `√`, superscripts and an astral-plane character |
| 4 | Decision omissions ★ | omitting mode, policy, output, branch or constant yields the named refusal, never a default |
| 5 | Byte determinism ★ | two generations byte-identical; a golden per fixture; regeneration after `show` identical |
| 6 | Presentation equivalence ★ | `typeset`, `show` and `generate` share one IR for every fixture; the preview is never parsed from generated text |
| 7 | Integer bounds | literals at `±(2^63−1)`, `-2^63` and just outside; the outside case refuses |
| 8 | Real literals | full finite binary64 range, 17-digit round trips and subnormals; conversion failures refused; exactness disclosed only with evidence, otherwise unknown |
| 9 | Exponent lowering | `x^2` is a multiplication chain; exact integer exponents preserve parity without lossy float conversion; a non-literal exponent is `pow` and the receipt records it |
| 10 | Division | literal nonzero divisor omits only the zero check; finite-result checks remain; variable divisor without `Suppose` refuses; real mode returns `Result` |
| 11 | Overflow policies | wrapping, saturating and checked produce the three distinct lowerings and the policy-appropriate result types |
| 12 | Totality of the output ★ | no golden contains `?:`, a bare integer operator, a redundant annotation, a placeholder or an empty branch |
| 13 | Piecewise order ★ | overlapping guards keep written order; removing `otherwise` refuses |
| 14 | Non-integer result | an integral-valued real definition refuses rather than emitting a coercion |
| 15 | Statuses | each fills only with its evidence; unavailable is unavailable, never green |
| 16 | Invalidation ★ | every receipt field clears the statuses it must; no field clears nothing |
| 17 | Stale ticket | a superseded completion changes no byte of state |
| 18 | Adversarial mutation ★ | mutating a guard, constant, operator, mode, policy or branch order changes the bytes and clears statuses; no mutation is a silent no-op |
| 19 | Refusal neutrality | text preserved verbatim, no source, no status, no danger styling, no false claim |
| 20 | Export provenance | exported bytes equal the pane's bytes; the receipt's hashes match; a forked copy has no receipt and no statuses |
| 21 | Application flow | accepted and refused workflows through the real `start`/`dispatch`; region refresh isolation |
| 22 | Native and VSIX interaction | actual native and installed VSIX complete both flows, source mapping, export, focus and numeric-error states; renderer fixtures alone do not qualify |
| 23 | Compiler agreement ★ | the fixture goldens are accepted by the real `osprey <generated-file> --check` where the host can run it; a rejection is a generator defect and blocks the phase |
| 24 | Coverage | all new module groups are inside the suite's `source_roots` and the project gate does not fall |
| 25 | Existing gates | `make ci`, the differential corpus, the proof suite, `src/osprey/packages/native` tests and VSIX packaging keep every existing case |

Row 23 is the only claim this plan may make about the compiler, and it is verified by running the compiler, not by reading this document.

## Boundary acceptance [WORKBENCH-CODEGEN-BOUNDARIES]

- [ ] Accept Unicode mathematical variables with deterministic hygienic code aliases and a visible name map; test NFC collisions, Greek symbols, and supplementary-plane letters.
- [ ] Cover zero-input definitions, exact large-exponent parity, the full finite binary64 literal range, 17-digit round trips and subnormals; test conversion overflow/underflow and unknown exactness without fabricating evidence.
- [ ] Enforce the finite-real approximation profile with explicit `Result` failures for nonfinite input, intrinsic domain faults and nonfinite intermediates/results, including guard evaluation. Pin lazy unselected branches and backend/libm differences.
- [ ] Permit only necessary type annotations when the numerical interface cannot be inferred (identity/unused parameters); compare actual inferred signatures with the declared domain. Do not insert arithmetic merely to constrain a type.
- [ ] Prove that an invalid edit clears `Generated` as well as validation statuses, and that previous output is visibly stale and cannot export as current.

## Later (after plan 0041 milestone 3 stage 2)

- [ ] Code actions (`cases`, `induction`, `constructor`, `exact?`) through Lean's server.
- [ ] Browser and iPadOS verdicts.
