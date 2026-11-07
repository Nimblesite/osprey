# Regressions since v0.28.2

Defects and loosened checks that arrived after the last release, v0.28.2. Bugs that shipped in a release are GitHub
issues labelled `bug`, not entries here. Delete an entry when its fix lands; history is in git.

Last checked 2026-11-07 against `main` at `9bd6dd38b` plus `qwen-port` at `fcb8bb4a5`, on an Apple-silicon Mac.

## Defects

| Regression | Where | Issue or status |
| --- | --- | --- |
| The WinUI host's process output limit is checked only after `ReadToEndAsync` buffers everything, and stderr has no limit, so a chatty child can exhaust host memory | `src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs:40` | Open: needs a Windows machine to test the fix |
| The deslop duplication ceiling rose from 2.962% to 6.2% (`dc879c7c7`): `.config/deslop.toml`, ratcheted down only, became `.deslop.toml`, which now also counts Osprey sources and tests, so Rust product code can pass at double its old cap | `.deslop.toml:4` | Open: ratchet back down as dedupe lands |

## By user decision

- The unfinished Qwen model left the package backend matrix (`c2cee10f0`); GPT-2 keeps its tests.
- A contract test in `scripts/ci/ci-scope.test.mjs` keeps iOS and Android casting out of CI ([CAST-MOBILE-GPUI]).
