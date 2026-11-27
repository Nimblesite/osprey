# Plan 0058: Checked examples and video tutorials

Goal: every exported declaration shows an example that `--doctests` compiles and runs, in one or more flavors, and every tutorial video teaches repository code that still exists. The website shows both. Spec homes: [Documentation Comments](../specs/language/0026-DocumentationComments.md) and [Documentation Tools](../specs/tooling/DocumentationTools.md), including its [Tutorial videos](../specs/tooling/DocumentationTools.md#tutorial-videos) rules.

Each item closes with the test, make target or website check it names. The doctest tests live in `src/toolchain/crates/osprey-cli/tests/cases/doctests.rs`; `src/toolchain/crates/corpus_doctests.sh` runs every package's examples in CI.

## Examples on every export `[DOC-EXPORT-EXAMPLE-REQUIRED]`

- [x] 🟥 **Every export of a PACKAGE MUST have an example: a missing one is a hard RED error** [LINT-LEVELS]. `unexampled-export` defaults to `deny` for any manifest with a `[package]` table, in the CLI (`error:`, the build fails) and in the editor (Error severity) alike (`osprey-types/src/lint_level.rs`, `04f101a2b`). No package may lower it in `[lints]`. Checked: `osprey src/osprey/packages/tiktoken --check` reports 62 errors, and the language server publishes them all at severity 1.
- [x] 🚫 **Example apps (any project without `[package]`) NEVER report a missing example**: no error, no warning, not even information. `unexampled-export` defaults to `allow` there, and `dependent_roundtrip`'s `warn` opt-in is gone (`00a33ddc1`). Checked: a sweep of all 207 manifests finds zero example findings in apps.
- [ ] 🐛 **Bug: the editor keeps the bundled 10:19 server, which predates both rules above.** It shows `exported function `Tensors::of` has no example` as *information* on `src/osprey/packages/gguf/src/tensors.py`, although that export has an example. The current `target/release/osprey lsp` publishes that file clean and reports real gaps as errors. In a dev checkout the extension must use the newest server: prefer `target/release/osprey` when it is newer than the bundled binary, or warn that the bundled one is stale. A test must open a package file through the extension's server resolution and assert the current binary's severity.
- [ ] Fill the remaining gaps (each package is RED until done), worst first: uitest, signal_flow, torch, vscode, native, numpy, tokenizers, pydantic_ai, scipy, re, jinja2, chromecast, tiktoken. Done so far: synth, gguf, xdsl, math, micrograd and passthrough. Done means `osprey <package> --check` is ok and `--doctests` has 0 failed.

## Examples in several flavors `[DOC-EXAMPLE-FLAVORS]`

Not built: today an example compiles in the documenting file's flavor and its fence label is ignored, so an ML package cannot show a Default or Python example of the same call. The rule moves into both specs when it is.

- [ ] The fence label picks the flavor (`osprey` Default, `osprey-ml` or `ospml` ML, `osprey-python` Python; unlabelled keeps the file's): `docparse.rs` and `DocExample` carry a flavor per fence and the harness runs each example in its own flavor, its `output` fence applying to it alone. Check: `doctests.rs` runs one export's Default, ML and Python examples side by side in an ML package.
- [ ] Labelled examples that follow one another without an `output` fence between them are twins of one example and must print the same output. Check: a twin pair with differing output fails `doctests.rs`, naming both fences.
- [ ] The exporter writes each twin in a fence labelled with its flavor and the website shows twins as flavor tabs. Check: the Playwright test for `/docs/packages/json/` asserts every function page renders at least one example with its output.
- [ ] Relabel every ML-file example fence labelled `osprey` to `osprey-ml` in the same change, because `osprey` now means Default (a breaking change). Check: `make _check_programs` and the corpus doctest pass stay green.
- [ ] Move `[DOC-EXAMPLE-FLAVORS]` into 0026 and DocumentationTools with its ID cited in code and tests; WRAPPER-DEPTH, DOCSTRING-CLEAN and EXPORT-EXAMPLE-REQUIRED are there already.

## Video tutorials `[DOC-TUTORIAL-VIDEO-*]`

Enforced by `src/toolchain/website/lib/videos.mjs` and `src/toolchain/website/tests/videos.spec.js`; films are made in the Videos repository and copied in with `make media` (`[MEDIA-VIDEOS-REPO]`).

- [ ] Re-make `package-manager` as a tutorial: add `@osprey/json` through the VS Code panel, use `Doc` in code that compiles and runs, then remove the package and show the compile error. Check: `src/toolchain/website/src/_data/videos.json` moves it from `intro` to `tutorial`, it runs 240 to 600 seconds, and `videos.spec.js` lists it.
- [ ] Website tests for `[DOC-TUTORIAL-VIDEO-CODE]` (code-scene share and path labels) and `[DOC-TUTORIAL-VIDEO-FRESH]` (a film whose quoted code drifted fails): `videos.spec.js` cites both IDs. The Videos repository's factory must first publish each film's code scenes (file, line range, start and end time) beside its chapters.
- [ ] (verify) Film scripts in the Videos repository cite `[DOC-TUTORIAL-VIDEO-EXPLAIN]`; nothing in this repository can show it.

## FerroUI `[NATIVE-FERROUI-RETARGET]`

The `rust-ui-framework-without-rust` tutorial teaches the GPUI host (`src/osprey/examples/gpui/painting` and `flock`). Every GPUI-based Osprey Native app moves to the FerroUI host (`src/toolchain/hosts/ferroui/`), `src/toolchain/hosts/gpui` is deleted once nothing uses it, and no new GPUI work is planned.

- [ ] Retarget Painting and Flock to the FerroUI host: `make -C src/toolchain/targets/macos painting` and `flock` open them on FerroUI and their headless self-tests run there. [Plan 0037](0037-osprey-native.md) owns the engineering.
- [ ] Re-make `rust-ui-framework-without-rust` on the FerroUI host once that lands: the watch page names FerroUI and the run command it shows still exists (`[DOC-TUTORIAL-VIDEO-SOURCE]`). Check: `videos.spec.js` keeps the tutorial listed.
