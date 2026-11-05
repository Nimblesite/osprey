# Zed integration verification

The thin client lives in `src/toolchain/zed-extension` and launches `osprey lsp`.
Its scope is the shared editor contract [EDITOR-ZED] and the checklist in
[plan 0012](../plans/0012-developer-tools.md#zed-integration-editor-zed).

## Verified on 2026-11-05

- The intentionally unimplemented launch policy failed its positive test first.
  Both loaded launch-policy tests now pass: configured/PATH paths, spaces,
  argument overrides, environment forwarding and exact missing/blank-path errors.
- `make -C src/toolchain/zed-extension build lint` passes: `wasm32-wasip2` builds,
  Rust formatting passes and Clippy denies warnings. The SDK registration macro
  stays behind a private host boundary; no lint suppression was added.
- The real stdio process passes Default, ML and Python-flavor hover, completion,
  definition, formatting, error introduction/removal, shutdown and UTF-16
  navigation after an astral character. Manifest checks compare the version,
  language registration, marker controls and pinned canonical grammar sources.
- Zed 1.27.0 installs the dev extension, builds its WASM component and pinned
  grammar, highlights Default source and displays live inferred hover signatures
  for Default, ML and explicitly selected Osprey Python. Ordinary Python stays
  Python. The acceptance window was closed afterward; its LSP process exited.
- The existing CI scope suite passes 57 tests. The added phase/wiring contract
  passes with the existing lint-contract suite (10 tests). CI YAML parses.

## Limits and remaining verification

Zed's built-in `.py` association wins over the first-line matcher. Select Osprey
Python explicitly or configure exact project-specific `file_types`; a marker
alone does not switch an existing `.py` buffer. ML/Python have no supplied
structural grammar, so Default grammar highlighting must not be claimed for them.

Zed sends workspace settings after initialization. Diagnostic scope configured
only in initialization options can therefore be reset; put ongoing diagnostic
configuration in `lsp.osprey.settings`. This was observed in the actual editor,
then the corrected settings produced working live hover.

An added mixed-flavor test found that an unsaved unknown identifier in an ML
helper was reported against the entry file while the helper received no error.
The checker dropped its available expression position. The minimal position fix
passes the retained real-process assertion and the current-source regression
across Default, ML and Python helpers. All four manifest/stdio tests pass against
the freshly built debug compiler, all 250 LSP tests pass, and scoped strict
Clippy passes for the extension, LSP and types crates.

Dependency audit found existing high-severity RUSTSEC-2026-0433 in
`pulldown-cmark` 0.13.5. The version floor and lock now select patched 0.13.6;
audit is clean and all 12 existing Markdown renderer tests pass. The product
renderer does not enable the advisory's wikilink option. The compiler owner was
notified through TMC. No audit or coverage gate was weakened.

The broader types suite passed 942 of 943 tests; the remaining GPU documentation
example references undefined `xs` in `gpuMatMulQuant`. That file was already dirty
before Zed work, and its GPU owner was notified. This failure is retained. Full
shared CI and publication are not claimed by these local checks.
