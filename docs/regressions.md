# Regressions since v0.28.2

Defects and loosened checks that arrived after the last release, v0.28.2. Bugs that shipped in a release are GitHub
issues labelled `bug`, not entries here. Delete an entry when its fix lands; history is in git.

Last checked 2026-11-07 against `main` at `3329a783d` plus this branch, on an Apple-silicon Mac.

## Defects

| Regression | Where | Issue or status |
| --- | --- | --- |
| The WinUI host's process output limit is checked only after `ReadToEndAsync` buffers everything, and stderr has no limit, so a chatty child can exhaust host memory | `src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs:40` | Open: needs a Windows machine to test the fix |

## By user decision

- The unfinished Qwen model left the package backend matrix (`c2cee10f0`); GPT-2 keeps its tests.
- A contract test in `scripts/ci/ci-scope.test.mjs` keeps iOS and Android casting out of CI ([CAST-MOBILE-GPUI]).
