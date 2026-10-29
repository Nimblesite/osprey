# Plan 0042: the proof workbench

Contract: [spec 0051](../specs/applications/0051-ProofWorkbench.md). Status: **phase 1 done; phase 2 next.** The proof language (notation, formula, step, statement, goals, emit) runs 35 cases in five seconds and covers 93.9% of its own lines. The workbench needs no change to the compiler and nothing from plan 0041 until phase 4, so phases 1 to 3 start now. The application lives at `apps/workbench/`; every phase lands with its tests in `apps/workbench/tests/` and leaves `make ci` green.

An agent picking this up reads spec 0051 and the [`@osprey/native` README](../../packages/native/README.md) first, then builds phase 1 exactly as laid out below. The phase order is the dependency order; nothing in a later phase is started before the earlier phase's exit holds.

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
- [ ] `src/app.ospml`: slices, action union, `update`, six regions, theme; `src/verdict.ospml` with ticketed `Run`
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

1. The framework additions, each with its own test in `packages/native/tests/`: the `Math` content node and its encoding; per-line marks and a cursor position on the multiline `Field`; the `Board` control and its stroke binding; the `Recognise` capability; the `board` theme token. Document each in the native README under [UI-COMPONENTS], [UI-EVENTS], [UI-EFFECTS] and [UI-THEME].
2. `src/app.ospml`: the slices, the action union (one union, grouped by the value each binding delivers), `update`, the six regions, the default theme; `src/verdict.ospml` with the ticketed `Run` of `osprey prove check` and the `Unchecked (no checker on this host)` path.
3. `src/main.ospml`: `Runtime::start` and `Runtime::dispatch` exports and the `Webview::respond` entry.
4. Fixture tests through the real `start`/`dispatch`: the region refresh case, the stale ticket case, the rejection underline case, the stroke and lasso cases, the contrast cases.

Exit: the suite drives the whole application with fixture messages; `osprey apps/workbench --check` reports every export documented; no host yet.

## Phase 3 — hosts [WORKBENCH-HOSTS] [WORKBENCH-MATH] [WORKBENCH-BOARD] [WORKBENCH-DESIGN]

1. **VS Code.** Compile the application to WASI through `@osprey/vscode`; teach the renderer under `vscode-extension/client/src/osprey-ui/` the `Math` node (MathML), the marked `Field` and the `Board` (a canvas with the chalk stroke shader described in the spec). Add the workbench to the extension's `compiled SDK renderer E2E` test. Screenshot `and_swap` for the review.
2. **Browser.** Move the renderer's host-neutral part to `hosts/web/` and serve the wasm32 build as a page; the browser's handwriting API where present, `Unsupported` where not; a headless smoke test.
3. **macOS and iPadOS.** A SwiftUI host in `hosts/apple/` on the model of `examples/macos`, with a `Math` layout view, a PencilKit board and PencilKit recognition; a simulator smoke test; the `chalk` theme reviewed on both appearances.

Exit per host: the smoke test passes in CI where the platform is available; the release review screenshot beside a printed page is in `docs/reports/`.

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
