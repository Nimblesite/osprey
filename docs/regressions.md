# Regressions since v0.28.2

Defects and loosened checks that arrived after the last release, v0.28.2. Bugs that shipped in a release are GitHub
issues labelled `bug`, not entries here. Delete an entry when its fix lands; history is in git.

Last checked 2026-11-06 against `main` at `3329a783d` plus this branch, on an Apple-silicon Mac.

## Defects

| Regression | Where | Issue or status |
| --- | --- | --- |
| VS Code native-editor focus fails on macOS (2 of 881 tests: the challenge's first request, and focus escaping a modal). CI runs the suite only on Linux xvfb, where it passes. Every other Linux-only suite passed on macOS on 2026-11-06 | `src/toolchain/vscode-extension/test/suite/native-editor.test.ts`; `ci.yml` `test-extension` | #344 |
| The pydantic-ai HTTP provider reports success when the body read or cleanup fails: `result if closed == 0 else result` has identical branches, and a failed read returns `Success` with an empty body | `src/osprey/packages/pydantic_ai/src/providers_http.py:26` | Open |
| The WinUI host's process output limit is checked only after `ReadToEndAsync` buffers everything, and stderr has no limit, so a chatty child can exhaust host memory | `src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs:40` | Open |
| HTTP externs changed from `int` to `Result` with no migration path | #337 | #353 |
| Website CSS and JS URLs carry no version, so iPhone Safari kept the old `prose.css` after a deploy and the docs menu vanished on phones | `src/toolchain/website` | Fix in #354 |

## Loosened checks

| Regression | Where | Issue or status |
| --- | --- | --- |
| `"unfinished": true` removes a package's 90% coverage floor. `transformers` went from 97.5% to no floor (user-approved), and nothing stops another package from setting the flag | `scripts/packages/package_coverage_projects.py`; `src/osprey/tests/coverage-thresholds.json` | Open: restrict it to a tested allowlist |
| The GPT-2 full-checkpoint parity test is `#[ignore]`d, so no CI job runs it, and the Python flavor has no equivalent | `src/toolchain/crates/osprey-cli/tests/cross_flavor_equiv.rs:184` | Open |
| Hunt CI skips `clean.osp` by name, the one program that proves no false positives on the hardest case | `scripts/make/tests.mk` (`HUNT_KNOWN_BUGS ?= clean`) | #306 |
| A hover check compared with itself, a 5 s mask poll, a deleted profiler provenance test, 30 `rejected_somehow` sites | various tests | #352 |

## By user decision

- The unfinished Qwen model left the package backend matrix (`c2cee10f0`); GPT-2 keeps its tests.
- A contract test in `scripts/ci/ci-scope.test.mjs` keeps iOS and Android casting out of CI ([CAST-MOBILE-GPUI]).
