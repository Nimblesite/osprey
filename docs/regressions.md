# Regressions since v0.28.2

Defects and loosened checks that arrived after the last release, v0.28.2. Bugs that shipped in a release are GitHub
issues labelled `bug`, not entries here. Delete an entry when its fix lands; history is in git.

Static review on 2026-11-08 compared v0.28.2 (`659ee827bd67362beda01c1b526e10dd48081d43`) with
`1b3ba2b16eebca0fbae7baf0823eac2b69870ae5` across seven first-parent changes. It focused on release
and CI gates, coverage policy, Windows host process I/O, and the Qwen acceptance gates. The comparison
uses committed files. Runtime performance and Windows behavior require separate tests.

## Defects

| Regression | Where | Issue or status |
| --- | --- | --- |
| The WinUI host starts unbounded reads of both output streams and checks only stdout length after both reads complete. A child can exhaust host memory before the limit is applied | [Processes.cs](../src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs) | Introduced with the host in `3329a783d`. Open. Verify bounded stdout and stderr with a Windows child-process test |
| The duplication ceiling rose from 3.0% in v0.28.2 to 6.2% at HEAD. The intermediate ceiling was 2.962% before `dc879c7c7` expanded the scan to include Osprey sources and tests | [.deslop.toml](../.deslop.toml) | Open. The aggregate percentages use different file populations. Check equivalent Rust scope separately before drawing conclusions about Rust duplication |

The release workflow guard passed with all 13 release jobs included in `release-complete`. The C runtime
threshold moved to aggregate coverage, the Apple tests remain required through `test-apple`, and the
WASM job restores the compiler from `osprey-build`. Those workflow moves preserve the respective checks.

## By user decision

- The unfinished Qwen model left the package backend matrix (`c2cee10f0`); GPT-2 keeps its tests.
- The transformers package still reports coverage, but the allowlisted `unfinished` flag removes its
  v0.28.2 floor of 97.5% (`c1bfd366e` enforces the allowlist).
- A contract test in `scripts/ci/ci-scope.test.mjs` keeps iOS and Android casting out of CI ([CAST-MOBILE-GPUI]).

## New-feature acceptance gaps

Qwen landed after v0.28.2. CI compiles its programs and runs the offline download tests. Its Metal model
lane runs locally, while real-weight baseline, cold prompt throughput, and long-context acceptance remain
open in [Plan 0049](plans/0049-local-coding-model.md).
