# Shared profiler integration

Osprey keeps its runtime and compiler layer: optimized native builds, DWARF symbolization, the platform sampler, the live-attach endpoint every native program serves ([PROF-ATTACH-ENDPOINT] in [0028-Profiler](../specs/tooling/0028-Profiler.md)), compiler invocation, cancellation and source classification (`isOspreySource`). lspkit owns everything above that: the `lsp-profiler` service mounted by `osprey lsp` through `crates/lsp-profiler-osprey`, and the `@nimblesite/lsp-profiler-ui` package whose views, palette, debugger links and VSIX contracts both Osprey and SharpLsp mount. The behaviour is specified in lspkit's [LSP-PROFILER-SPEC](../../lspkit/docs/specs/LSP-PROFILER-SPEC.md).

## Remaining

- [ ] Merge [lspkit #27](https://github.com/Nimblesite/lspkit/pull/27) (`feature/shared-profiler-ui`, green on every check) into lspkit `main`, repin Osprey's and SharpLsp's `lspkit` submodules to the merged commit, and delete this plan. With a merge commit the pinned `10e2a37` is already on `main`; a squash merge needs both repins.

## Done

- [x] The review of 2026-10-24: malformed speedscope exports are classified errors; search is one sort and sweep; labels fit their narrowest width; frame ranking, search and the palette's names and defaults have one definition each; every memory and retention colour reads the palette; malformed webview posts never reach a handler; Osprey's flame panel is lspkit's `SingletonWebviewPanel`.
- [x] Every view draws in the host palette, pinned by a contract both hosts run (`testing/webview-contract-palette.ts`): the dashboard's warm and base bars and its toggle labels, the retention graph's roots and edges, the editor's leak decorations at every confidence, and text on an accent, which takes the accent's ink on a dark palette.
- [x] No production module or shipped contract in the UI package has a function of 20 lines or more, and no file is over 500 lines. `profiler.ts` and `memory-profiler.ts` are split; error reporting and server commands have one definition each; `buildFlameHtml`, a document builder no product code called, is deleted, and Osprey's `profiler-html.test.ts` tests the document `showFlamePanel` renders. The suites' own long bodies are recorded in lspkit's plan.
- [x] The UI package lints with SharpLsp's ESLint rules, rule for rule, in `make lint`. Meeting them replaced every type assertion with a check where the value is read.
- [x] The UI suite's host manifest is generated before every `npm test` instead of committed; the committed copy had drifted.
- [x] The UI package's own suite runs against a mock language server, an exception lspkit's spec records: server-backed flows are tested in each host against its real runtime.
- [x] SharpLsp's `lspkit` submodule follows this branch, and its tests read SharpLsp's palette and advertise CPython's profilers the way the capability rules require.
- [x] The live profiler e2e tests wait for samples, not seconds.
- [x] `tests/effects/scope/finalizers_masks_instances` passes in both flavors.

## Moved

- Windows live attach and a native heap backend, with debugger type retention: [plan 0035](0035-native-profiler-backends.md).
- npm publication of the UI package: lspkit's plan, Phase 4.
- The integrated-terminal debug e2e failures: Xcode's lldb-dap cannot complete a terminal launch, now the adapter the resolver finds once Homebrew's LLVM 23 dropped lldb-dap from the `llvm` keg: [plan 0012](0012-osprey-debugger.md).
