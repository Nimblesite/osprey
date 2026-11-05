# PR #343 CI fixes (run 42044481843)

Owner of each job is the agent named in brackets. Claim or report via TMC to `check-once-claude`.

## Jobs

1. **WinUI host tests [check-once-claude, done locally]**: `Framework.cs` and `BoundaryTests.cs` looked
   for `packages/native` and `examples/` at the repo root; they are now under `src/osprey/`. 22 tests failed.
2. **Graphics pictures [check-once-claude, done locally]**: `src/osprey/examples/graphics/pictures.ps1`
   resolved the root two folders up; it is four folders up after the layout move.
3. **WinUI goldens [OspreyCodex]**: storage, flock, painting and probe now draw at 1008×681 (flock 1012×685)
   but their `goldens/windows/100` are 1164×741. signal-flow content matches but its screen shot fails.
   Cause unknown: sizing is in `src/toolchain/hosts/winui/OspreyHost/HostWindow.cs:86-94`. Find what changed
   (runner display area or host `Width`/`Height`); do not refresh goldens blindly. Shots are in the run's
   `winui-self-test` artifact.
4. **Agent type-check budget [OspreyCodex]**: `agent_check_budget.rs:57`, `examples/agent/` and
   `examples/agent/test-app` still checking after 120s on CI Linux.
5. **Deslop gate [OspreyCodex]**: `Format & Analyse` exits 3, so duplication is over the
   `.config/deslop.toml` ceiling. Run `make _deslop`, remove real duplication, and never raise the ceiling.
6. **Transient 504s**: wasm32, Rust packages, VS Code extension and proof-kernel jobs all failed on a curl
   504 during setup; rerunning should clear them. Coverage thresholds failed only because those jobs left
   no reports.
