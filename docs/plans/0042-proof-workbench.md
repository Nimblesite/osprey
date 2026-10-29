# Plan 0042: the proof workbench

Contract: [spec 0051](../specs/applications/0051-ProofWorkbench.md). Status: **phase 1 complete; shared application and VSIX delivery under stabilization.** The application lives at `apps/workbench/`; every phase lands with tests in `apps/workbench/tests/` and must leave `make ci` green. The earlier 87.4% application coverage failure was retained and repaired. The frozen 2026-10-30 application now passes all 108 tests under GC, ARC and wasm32 with 90.8% own-source coverage (2,053 of 2,261 lines), above the unchanged 90% gate. The shared proof kernel separately passes all 325 tests under those three targets. Final imported-proof host, installed-VSIX and screenshot acceptance remain open.

An agent picking this up reads spec 0051 and the [`@osprey/native` README](../../packages/native/README.md) first, then builds phase 1 exactly as laid out below. The phase order is the dependency order; nothing in a later phase is started before the earlier phase's exit holds.

Design implementation reference: [Paper & Slate handoff](../designs/proof-workbench/README.md), [offline screen gallery](../designs/proof-workbench/index.html) and [playable workflow study](../designs/proof-workbench/prototype/index.html). These are reviewed design artifacts, not production phase completion. The canonical tokens, type and motion contract are in [WORKBENCH-DESIGN](../specs/applications/0051-ProofWorkbench.md#beautiful-or-not-shipped-workbench-design).

## Checklist

Ticked as the work lands; a box is ticked only when its test exists in `apps/workbench/tests/`.

### Phase 1 — the proof language
- [x] `apps/workbench/osprey.toml`, `tests/osprey.toml`, coverage threshold registered at 90%
- [x] `src/notation.ospml` with `Notation::expand` and `Notation::palette`, tested against the table
- [x] `src/formula.ospml`: `read`, `show`, `equal`; symbol and sentence forms; round-trip corpus
- [x] `src/statement.ospml` and `src/step.ospml`: headings, phrase table, `Unknown`; ten near-miss sentences
- [x] `src/goals.ospml`: `start`, `apply`, `Mismatch`; the 50-step timing case
- [x] `src/emit.ospml`: `tests/golden/and_swap.ospml.expected` byte-identical
- [x] Every function under 20 lines, every file under 500; suite at 90% line coverage

### Phase 2 — the application
- [ ] Framework additions with tests in `packages/native/tests/`: `Math` node, `Field` marks and cursor, `Board`, `Recognise`, `board` token; native README updated
- [ ] `src/app.ospml`: slices, action union, `update`, six regions, theme
- [ ] `src/kernel.ospml` and `src/verdict.ospml`: the in-process kernel verdict over a kernel-checked prelude (replaces the ticketed `Run`; see the deviation below)
- [ ] `src/main.ospml`: `Runtime::start`, `Runtime::dispatch`, `Webview::respond`
- [ ] Fixture tests: region refresh, stale ticket, rejection underline, stroke and lasso, contrast

### Phase 3 — hosts
- [ ] VS Code: WASI build, `Math`, marked `Field`, `Board` in the renderer; E2E test; `and_swap` screenshot
- [ ] Browser: `hosts/web/`, wasm32 page, handwriting where present; headless smoke test
- [ ] macOS and iPadOS: `hosts/apple/` SwiftUI host, PencilKit board and recognition; simulator smoke test

### Phase 4 — verdicts, library, paper
- [ ] Badges from `osprey prove check`; goal engine agrees with the kernel over every fixture step
- [ ] Library slice from an imported environment; search, hover, `Apply <name>.`
- [ ] Rendered page and HTML export with MathML
- [ ] Road test recorded in `docs/reports/`

## Phase 1 — the proof language without a window [WORKBENCH-FORMULA] [WORKBENCH-STATEMENT] [WORKBENCH-STEPS] [WORKBENCH-GOALS] [WORKBENCH-EMIT]

Pure modules, no host, no framework yet. Tests first for every table in the spec.

1. `apps/workbench/osprey.toml` (`[project] name = "workbench"`, `source_roots = ["src", "../../packages/native/src"]`, `default_namespace = "workbench"`, `[modules] allow_wildcard_imports = false`) and `apps/workbench/tests/osprey.toml` (`source_roots = [".", "../src", "../../../packages/native/src"]`, `entry = "main.ospml"`). Register the project in `coverage-thresholds.json` at 90%.
2. `src/notation.ospml`: the table as one list, `Notation::expand`, `Notation::palette`.
3. `src/formula.ospml`: the union, `Formula::read` (symbols and sentences, one precedence climber), `Formula::show`, `Formula::equal`.
4. `src/statement.ospml` and `src/step.ospml`: the headings, the phrase table, `Unknown`.
5. `src/goals.ospml`: `Goals::start`, `Goals::apply`, `Mismatch`.
6. `src/emit.ospml`: the canonical `.ospml`, binders lifted from leading `Assume`, `sorry` for `Unknown`.
7. `tests/`: one file per module, the `and_swap` golden under `tests/golden/and_swap.ospml.expected` (every `.ospml` under `tests/` is compiled as suite source, and the ML frontend has no `theorem` form until the discipline is registered), a generated-formula corpus for the round trip, the ten near-miss sentences, the 50-step timing case.

Exit: the suite passes with 90% line coverage; the emitter writes the golden byte for byte; every function under 20 lines, every file under 500. The golden is checked by `osprey prove check` in phase 4, once plan 0041 registers the `lean4` discipline; before that no frontend parses `theorem`.

Deviations from the spec's signatures, recorded in spec 0051: a `Result`'s error channel holds only a string, so `Formulas::read` returns `Reading` (`Read` or `Faulted`) and `Goals::apply` returns `Outcome` (`Advanced` or `Stuck`). The suite has no clock, so the 50-step case asserts the run closes; the under-a-millisecond bound needs a benchmark under `benchmarks/` and is open. The emit golden case compares against a literal byte-identical to `tests/golden/and_swap.ospml.expected`; reading the file itself waits on a file builtin the suite can use.

## Phase 2 — the application over `@osprey/native` [WORKBENCH-STORE] [WORKBENCH-REGIONS] [WORKBENCH-VERDICT]

**One Osprey application for every host.** `apps/workbench/src/` owns the store, updates, parsing, goals, source generation, verification state and typed `@osprey/native` component trees. Compile that same logic for the browser and native app. Browser and Apple hosts only render those trees, preserve native control state and execute capabilities. The React study under `docs/designs/` is a visual reference; it must not become a second application implementation.

1. The framework additions, each with its own test in `packages/native/tests/`: the `Math` content node and its encoding; per-line marks and a cursor position on the multiline `Field`; the `Board` control and its stroke binding; the `Recognise` capability; the `board` theme token. Document each in the native README under [UI-COMPONENTS], [UI-EVENTS], [UI-EFFECTS] and [UI-THEME].
2. `src/app.ospml`: the slices, the action union (one union, grouped by the value each binding delivers), `update`, the six regions, the default theme; `src/kernel.ospml` and `src/verdict.ospml` with the in-process kernel verdict of [WORKBENCH-VERDICT].
3. `src/main.ospml`: `Runtime::start` and `Runtime::dispatch` exports and the `Webview::respond` entry.
4. Fixture tests through the real `start`/`dispatch`: the region refresh case, the stale ticket case, the rejection underline case, the stroke and lasso cases, the contrast cases.

Exit: the suite drives the whole application with fixture messages; `osprey apps/workbench --check` reports every export documented; no host yet.

Deviations, recorded in spec 0051: the verdict comes from the proof kernel of spec 0049 called in-process, not from a ticketed `Run` of `osprey prove check`, because the kernel is an Osprey library and the workbench is its interface; it works on every host, a tablet included, and needs nothing from plan 0041. The connectives are impredicative encodings over a prelude the kernel admits constant by constant. The multiline editor is a new `Editor` component rather than a change to `Field`; `Math`'s `Row` is `Seq`; the board keeps one host-held slice per stroke and `Recognise` names slices. Learn mode is [plan 0043](0043-workbench-lessons.md).

## Phase 3 — hosts [WORKBENCH-HOSTS] [WORKBENCH-MATH] [WORKBENCH-BOARD] [WORKBENCH-DESIGN]

1. **VS Code.** Compile the application to WASI through `@osprey/vscode`; teach the renderer under `vscode-extension/client/src/osprey-ui/` the `Math` node (MathML), the marked `Field` and the `Board` (a canvas with the chalk stroke shader described in the spec). Add the workbench to the extension's `compiled SDK renderer E2E` test. Screenshot `and_swap` for the review.
2. **Browser.** Import the same host-neutral renderer modules used by the VSIX and serve the wasm32 build as a page; the browser's handwriting API where present, `Unsupported` where not; a headless smoke test. Never copy the renderer or the Osprey application into a second implementation.
3. **macOS and iPadOS.** A SwiftUI host in `hosts/apple/` on the model of `examples/macos`, with a `Math` layout view, a PencilKit board and PencilKit recognition; a simulator smoke test; the `chalk` theme reviewed on both appearances.

Exit per host: the smoke test passes in CI where the platform is available; the release review screenshot beside a printed page is in `docs/reports/`.

The proof-library checkpoint now covers twelve OpenAI supporting lemmas and all 175 selected core statements (172 original exports and three exact-statement compatible proof bodies). The expanded core public gate passes 525 checks and 1,050 corruption controls with fresh Lean verification. A single canonical twelve-group catalogue and shared asset stager supply the same exports and original licenses to native and VSIX hosts. Application catalogue tests, final bundled interactions, coverage and reference-screen acceptance still govern delivery; these proof receipts do not complete those host gates.

### VSIX delivery [WORKBENCH-VSIX]

The contract is [WORKBENCH-VSIX](../specs/applications/0051-ProofWorkbench.md#vsix-adapter-workbench-vsix). Extend the package manager's existing host by sharing its panel/session/rendering machinery. `apps/workbench/src/` remains the single application source for VS Code, browser and native hosts; all maths, proof and lesson decisions stay there.

The 30 October checkpoint has a sealed historical VSIX receipt with 806 extension tests, 169 installed-extension tests and every extension coverage gate above 95%. It proves the bundled Nat, OpenAI and large List proof interactions on that source, but does not sign off the later UI changes or the user's existing VS Code profile. Root screenshot comparisons in `docs/designs/proof-workbench/design-qa.md` remain blocked on recorded geometry, styling and icon differences. The shared imported-theorem display now uses its verified term and the existing mathematics tree, preserving the raw statement and assumptions. Regression tests pin the selected result frame, exact `Nat.add_comm` notation, overloaded-instance refusal, grouping, de Bruijn binding and collisions with global constants and sort names. The frozen application passes all 108 tests under GC, ARC and wasm32 with 90.8% measured own-source coverage (2,053/2,261 lines); the final ARC/WASM source/compiler seals are unchanged. Revised host captures and host/package gates remain open. Keep the delivery boxes open until those current receipts and visual comparisons pass.

- [x] Pin and fix the compiler failures preventing the shared app from compiling; retain the failing regressions until the compiler passes them. Compiler `6f5c55438f39162831fc9c2ccf3dcdb5d45dcc1a9a984deec4c73de768831dae` passes the full Rust workspace, all twelve crate coverage floors, the 325-case three-target kernel gate, and the 108-case three-target application suite. The application keeps its full shared source; host ABI and foreign-call regressions remain in the tests. Host/image delivery is tracked separately below.
- [ ] Register `osprey.workbench.open`; open without a workspace, reuse the panel and release its session on disposal.
- [ ] Share the package host's generic panel, event transport, WASI runner and renderer; preserve existing package behavior.
- [ ] Complete bounded bundled-lesson reads and isolated progress storage through framework capabilities; forward host-held slices and resolve only approved media assets.
- [ ] Bundle `target/workbench.wasm` and lesson assets locally, in CI and in every release VSIX; fail packaging on missing, invalid or stale content.
- [ ] Run the real compiled application through the VS Code browser harness: lessons, Maths/MathML, field editing, board and progress round trip. Run package regressions after host extraction.
- [ ] Run a real common Lean proof through the shared kernel in the workbench and show the current checked result; corrupt its proof and show the actual rejection. Plan 0041 targets the complete `Nat.add_comm` dependency export after the recorded And/Eq/Iff controls. Plan 0041 now also carries the pinned recent OpenAI proof corpus: eight stack-prefix lemmas, three exact doubling-loop value/work lemmas and a finite-index injectivity lemma, with 175 core selections and blocked NumberTheory ports tracked separately. Expose those same checked artifacts and axiom reports through the shared app rather than copying proof code into a platform adapter. Navier–Stokes is a future high-water mark; importing screens alone does not satisfy this proof gate.
- [ ] Record the exact build, verification and test commands under AGENTS.md Commands; tick completion only with passing evidence and a reviewed screenshot.
- [ ] Match the production VSIX to `docs/designs/proof-workbench/screens/` at identical viewport and state. Review paired reference/implementation captures for initial board, proof, source, mismatch, reading, slate and responsive layouts. Only unavailable host controls may be omitted; the remaining screens must be pixel perfect. Functional checks alone do not complete this gate.

## Phase 4 — verdicts, library and the paper [WORKBENCH-VERDICT] [WORKBENCH-LIBRARY] [WORKBENCH-PAPER]

Needs plan 0041 M3 (the `lean4` discipline checking `and_swap`) and M1 (`osprey prove import`).

1. Badges fill: `osprey prove check` on the generated file, the agreement test between the goal engine and the kernel over every fixture step.
2. The library slice loaded from an imported environment; name and shape search; hover; `Apply <name>.` insertion.
3. The rendered page and the HTML export with MathML.

Exit: the road test of plan 0041 run in the workbench and recorded in `docs/reports/`: a mathematician or scientist new to Osprey writes and checks `and_swap` in under ten minutes without documentation.

## Later fragments

Equality and rewriting, natural numbers and induction, quantifiers over types, structures: each extends `Formula`, the sentence table, the step table and the goal table, with the same tests, and nothing else. Default and Python flavors of the generated file follow [PROOF-SURFACE-FLAVORS] when their proof frontends exist; the emitter gains a flavor parameter and the goldens a twin.

## Pi delegation

Under the AGENTS.md rules: the notation table and its tests, the near-miss corpus, the formula corpus generator, MathML rendering in the web renderer and the screenshot runs go to pi, each against a skeleton and test written here first. The formula reader, the goal engine, the emitter, the store and the framework additions are written and reviewed here.

## What this plan never does

- Put a tactic, a theory or a verdict in the application: the kernel decides, the engine only displays.
- Let a host filter, label, validate or decide; a host draws and completes capabilities.
- Ship a monospaced formula, a vector-line "chalk" or a flickering check.
- Add a service only the workbench can call.
