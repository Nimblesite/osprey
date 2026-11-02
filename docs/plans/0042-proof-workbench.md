# Plan 0042: the proof workbench

> **STOP — NOT YET IN FORCE.** This plan was rewritten on 2026-10-31 for the `.lean`-only direction of [PROOF-SOURCE](../specs/language/0049-DependentTypesAndProofs.md#proof-source-proof-source) while the `feat/type-disciplines` PR is still open. **Any agent working on that PR ignores this rewrite until the PR has merged.** Do not delete, rename, rewrite or stop maintaining any code, test, fixture or screen because of this text before then; the previous revision in git history is the plan for that PR. After the merge, [plan 0044](0044-lean-source-compatibility.md) runs first and this plan resumes behind it.

Contract: [spec 0051](../specs/applications/0051-ProofWorkbench.md). Status: **the application exists at `apps/workbench/` and still carries the deleted formats; plan 0044 phase 0 removes them and phase 1 installs the `.lean` document and checking services. This plan picks up after that.** Every phase lands with tests in `apps/workbench/tests/` and must leave `make ci` green with the 90% coverage floor.

Design reference: [Paper & Slate handoff](../designs/proof-workbench/README.md). Screens that showed the deleted English document or its generated pane are re-captured in phase 3 before they bind; the remaining screens are the oracle as before.

## Deleted

Phase 1 of this plan as written before 2026-10-31 ("the proof language": `formula`, `statement`, `step`, `goals`, `emit`, the `and_swap` golden, the in-process prelude and step elaborator) built the formats that [PROOF-SOURCE-DELETED] now bans. Those modules and their tests are contamination and are removed by plan 0044 phase 0. Nothing from that phase is reused.

## Checklist

A box is ticked only when its test exists in `apps/workbench/tests/`.

### Phase 1 — the `.lean` document (with plan 0044 phase 1)
- [ ] `src/document.ospml`: open, edit, cursor, save; byte-identical round trip over every `.lean` fixture
- [ ] `src/lean.ospml`: `LeanCheck`/`LeanGoals` requests with revision tickets; stale completions dropped; the Unavailable path
- [ ] `src/verdict.ospml`: kernel verdict over the completion's export; `Checked`/`Rejected`/`Unchecked` per declaration
- [ ] `src/app.ospml`: slices, action union, `update`, the five regions, theme; `src/main.ospml` entries
- [ ] Fixture tests: region refresh, stale ticket, Lean error underline, corrupted export rejected, `sorry` unchecked, contrast

### Phase 2 — library and the paper
- [ ] `Library` over the bundled exports, then the checked Mathlib environment; name and shape search; hover; `exact`/`apply` insertion
- [ ] `ProofTerms` presentation of checked statements through `Math`, with the notation-identity rules of [WORKBENCH-LIBRARY]
- [ ] The reading view from Lean's pretty-printed statements and goals; HTML export with MathML

### Phase 3 — hosts
- [ ] VS Code: WASI build, the `Lean` capability over the toolchain the extension finds; E2E opens `AndSwap.lean` and sees `Checked`; screens re-captured and compared
- [ ] Browser: `hosts/web/`, wasm32 page, Unavailable until plan 0044 phase 2; headless smoke test
- [ ] macOS and iPadOS: `hosts/apple/` SwiftUI host, toolchain bundled on macOS; simulator smoke test
- [ ] ❓ Decide: `what-is-a-solver` has no prove challenge, which [LESSONS-CURRICULUM] requires unless noted — add one or note the exception in 0052's table
- [ ] Road test recorded in `docs/reports/`: a newcomer writes and checks `and_swap` in a fresh `.lean` file in under ten minutes

## Phase 1 — the `.lean` document [WORKBENCH-DOCUMENT] [WORKBENCH-CHECK] [WORKBENCH-VERDICT] [WORKBENCH-STORE] [WORKBENCH-REGIONS]

**One Osprey application for every host.** `apps/workbench/src/` owns the store, updates, the document, the toolchain adapter, verdicts and the typed `@osprey/native` trees. Hosts render those trees, preserve native control state and execute capabilities. The React study under `docs/designs/` is a visual reference, never a second implementation.

1. `Document`: the text and cursor; open and save as bytes; no parsing.
2. `Lean`: the two requests, their completions and the revision ticket; the Unavailable completion.
3. `Verdicts`: `Verdicts::decide` on the export carried by a completion, through `ImportedProofs`' kernel path; the per-declaration table of spec 0051.
4. `App` and `main`: slices, messages, regions, theme, entries.

Exit: the suite drives the whole application with fixture messages and recorded completions; `osprey apps/workbench --check` reports every export documented; no host yet.

## Phase 2 — library and the paper [WORKBENCH-LIBRARY] [WORKBENCH-PAPER]

The bundled-library flow stands: `assets/proof-libraries.json` lists the pinned export groups; `scripts/workbench-proof-assets.py` stages exact exports and licences; the application re-checks the selected export in-process from an empty environment on every frame or source change; saved state never supplies a verdict. Records for this flow (the `Nat.add_comm` closure, the OpenAI corpus, the identity-guarded notation of `ProofTerms`, the Euclid `Nat.exists_infinite_primes` probe that still exceeds the 180-second diagnostic limit) live in [plan 0041](0041-dependent-types-and-mathlib.md); they are kernel evidence, not host delivery.

## Phase 3 — hosts [WORKBENCH-HOSTS] [WORKBENCH-VSIX] [WORKBENCH-DESIGN]

1. **VS Code.** Compile to WASI through `@osprey/vscode`; the renderer draws `Math` (MathML) and the marked `Editor`; the `Lean` capability runs the toolchain. Share the package host's panel, event transport, WASI runner and renderer. Bundle `workbench.wasm`, lessons and staged exports; packaging fails on missing, invalid, stale or extra content.
2. **Browser.** The same renderer as a page under `hosts/web/`; never a second implementation.
3. **macOS and iPadOS.** The SwiftUI host under `hosts/apple/` with a `Math` layout view; the toolchain bundled in the macOS app.

Exit per host: the smoke test passes in CI where the platform is available; the screenshot beside a printed page is in `docs/reports/`; the production VSIX matches the surviving reference screens pixel for pixel at identical viewport and state.

## Later

Statement-shape search over the full checked Mathlib, code actions (`cases`, `induction`, `constructor`, `exact?`) through Lean's server, and the browser and iPadOS verdicts, follow plan 0044 phase 2.

## What this plan never does

- Parse, elaborate, judge or generate proof text in the application: Lean and the kernel decide, the application displays.
- Read or write any proof format but `.lean`.
- Let a host filter, label, validate or decide; a host draws and completes capabilities.
- Ship a monospaced formula in the reading view, or a flickering check.
- Add a service only the workbench can call.
