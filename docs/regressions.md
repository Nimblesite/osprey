# Regressions since v0.28.2

Defects and loosened checks that arrived after the last release, v0.28.2. Bugs that shipped in a release are GitHub
issues labelled `bug`, not entries here. Delete an entry when its fix lands; history is in git.

Static review on 2026-11-08 compared v0.28.2 (`659ee827bd67362beda01c1b526e10dd48081d43`) with
`1b3ba2b16eebca0fbae7baf0823eac2b69870ae5` across seven first-parent changes. It focused on release
and CI gates, coverage policy, Windows host process I/O, and the Qwen acceptance gates. The comparison
uses committed files. Runtime performance and Windows behavior require separate tests.

The PR #373 follow-up compared `1b3ba2b16` with `1d67de091`, focusing on HTTP disconnect detection,
thread and sleep behavior, Transformers generation errors, and the Agent's real local-model lane.
The HTTP review reproduced a missed orderly close above the POSIX `select` descriptor limit. The fix
passed the real-socket regression and both HTTP C suites locally. The package coverage timeout seen on
an earlier PR head was traced to repeated tokenizer work. Exact-head CI is the integration check for
these fixes.

## Defects

| Regression | Where | Issue or status |
| --- | --- | --- |
| A generic top-level function passed as a value instantiates at its `int`-defaulted signature, not at the use's types: `flattened parts = foldList (parts, [], joined)`, with `joined (left, right) = foldList (right, left, \(all, value) => listAppend all value)`, turns `[[1.5, 2.0], [3.0]]` into `[1, 2, 3]` read back as float bits (`4.94e-324…`), under GC and ARC; a lambda `\(all, part) => joined (all, part)` is correct, and strings are unaffected | [function_types.rs](../src/toolchain/crates/osprey-codegen/src/builder/function_types.rs) `identifier_fn_type` reads `prog.functions`; [applications.rs](../src/toolchain/crates/osprey-types/src/applications.rs) elaborates generic calls only, never a generic name in value position | Open, found 2026-11-21 by spectral analysis of the synth's frequency shifter. Whether a release has it is unchecked. Needs a corpus regression and an elaborated instantiation for value-position names |
| `args` and `env` became built-ins after v0.30.0 and built-ins cannot be redefined, so a program declaring its own top-level `fn env(...)` or `fn args(...)` no longer compiles ("cannot redefine built-in function `env`") | [builtins.rs](../src/toolchain/crates/osprey-types/src/builtins.rs) `SHADOWABLE_BUILTINS` | Open. Needs a decision: add both names to `SHADOWABLE_BUILTINS` or keep them reserved |
| The WinUI host starts unbounded reads of both output streams and checks only stdout length after both reads complete. A child can exhaust host memory before the limit is applied | [Processes.cs](../src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs) | Introduced with the host in `3329a783d`. Open. Verify bounded stdout and stderr with a Windows child-process test |
| The duplication ceiling rose from 3.0% in v0.28.2 to 6.2% at HEAD. The intermediate ceiling was 2.962% before `dc879c7c7` expanded the scan to include Osprey sources and tests | [.deslop.toml](../.deslop.toml) | Open. The aggregate percentages use different file populations. Check equivalent Rust scope separately before drawing conclusions about Rust duplication |

The release workflow guard passed with all 13 release jobs included in `release-complete`. The C runtime
threshold moved to aggregate coverage, the Apple tests remain required through `test-apple`, and the
WASM job restores the compiler from `osprey-build`. Those workflow moves preserve the respective checks.

The 2026-11-20 review compared v0.30.0 with PR #417 (`bbfb80d92`) across codegen, the type checker,
the parser and the C runtime. Fixed before the merge: printing a recursive type overflowed the
compiler's stack (`11cc19017`), a union whose variant holds a function printed it as `<handle>`
(`e778ecb4e`), and every list-read miss built its `IndexOutOfBounds` record even when no arm read it
(`8620dca9a`). The one released bug found, the `structuralEquals` path order, is
[#421](https://github.com/Nimblesite/osprey/issues/421).

## By user decision

- The unfinished Qwen model left the package backend matrix (`c2cee10f0`); GPT-2 keeps its tests.
- The transformers package still reports coverage, but the allowlisted `unfinished` flag removes its
  v0.28.2 floor of 97.5% (`c1bfd366e` enforces the allowlist).
- A contract test in `scripts/ci/ci-scope.test.mjs` keeps iOS and Android casting out of CI ([CAST-MOBILE-GPUI]).

## New-feature acceptance gaps

Qwen landed after v0.28.2. CI compiles its programs and runs host Qwen fixtures. Local acceptance used
real Qwen weights and verified the Agent's command, file edit, rerun and native UI goldens with an exact
edit supplied in the task. The Pydantic lane passed six checks, including a negative control for the direct
Qwen judge. Cold prefill measurements still trail the Ollama reference, and long-context acceptance remains
open in [Plan 0049](plans/0049-local-coding-model.md).
