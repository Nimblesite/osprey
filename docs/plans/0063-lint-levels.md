# Plan 0063: configurable lint levels

Implements [LINT-LEVELS] ([spec](../specs/language/0026-DocumentationComments.md)).

## Goal

Every analyzer rule has one level, `allow`, `info`, `warn` or `deny`, which a project sets in the `[lints]` table of its `osprey.toml`. The CLI and the language server read the same levels. The defaults depend on the project kind: a package (its manifest has `[package]`) must document its API, so a missing doc comment is an error there; an application's exports are its own, so documentation rules are silent. Every other way of switching a lint on or off is deleted.

## Checklist

- [x] `LintLevel` and `LintLevels` live in the analyzer (`osprey-types/src/lint_level.rs`); the old `is_info` special case is deleted
- [x] `osprey.toml` `[lints]` parsing; `[package]` marks a package; an unknown level is a manifest error
- [x] CLI: `allow` hides, `info` shows uncounted, `warn` fails only under `--warnings-as-errors`, `deny` prints `error:` and always fails, even under `--no-warnings`
- [x] Language server: the same levels re-level every advice diagnostic; real errors are never lowered
- [x] Defaults: `undocumented-export` is `deny` in a package and `allow` in an application; `unexampled-export` is `info` in a package and `allow` in an application
- [x] Loaded tests: manifest permutations, CLI modes × flags × project kinds, editor severities per kind and override
- [x] One rule registry (`LINT_RULES`) names every analyzer rule; an unknown rule in `[lints]` is a manifest error; the editor only re-levels registered rules, so a project cannot silence a non-lint diagnostic such as `test-skipped`
- [x] Delete `[modules].published_library`: `module-deep-hierarchy` and `namespace-reverse-domain` default to `allow` in a package, and a project that wants them off says so in `[lints]`
- [x] The Osprey package manager's manifest reader accepts `[lints]` and refuses `published_library`
- [x] Specs: [MODULES-STYLE] and the manifest example in 0025 describe `[lints]`
- [x] Every package documents every export, so `deny` holds in CI (GLM pi, reviewed)
