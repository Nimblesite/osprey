# PR #343 CI fixes (run 42044481843)

Owner of each job is the agent named in brackets. Claim or report via TMC to `check-once-claude`.

## Jobs

1. **WinUI host tests [check-once-claude, done locally]**: `Framework.cs` and `BoundaryTests.cs` looked
   for `packages/native` and `examples/` at the repo root; they are now under `src/osprey/`. 22 tests failed.
2. **Graphics pictures [check-once-claude, done locally]**: `src/osprey/examples/graphics/pictures.ps1`
   resolved the root two folders up; it is four folders up after the layout move.
3. **WinUI goldens [check-once-claude, fixed]**: the Developer Mode and 1920×1080 desktop steps lacked
   `if: !cancelled()`, so the earlier failures skipped them and every window was clamped to 1024×768. Details: storage, flock, painting and probe now draw at 1008×681 (flock 1012×685)
   but their `goldens/windows/100` are 1164×741. signal-flow content matches but its screen shot fails.
   Cause unknown: sizing is in `src/toolchain/hosts/winui/OspreyHost/HostWindow.cs:86-94`. Find what changed
   (runner display area or host `Width`/`Height`); do not refresh goldens blindly. Shots are in the run's
   `winui-self-test` artifact.
4. **Agent type-check budget [OspreyCodex]**
   - [x] Confirm `examples/agent/` and `examples/agent/test-app` exceeded 120 seconds in the failed Linux run.
   - [x] Pass the unchanged coverage-instrumented test locally in 69.80 seconds.
   - [ ] Verify the unchanged test in the hosted Rust workspace rest shard.
5. **Deslop gate [OspreyCodex]**
   - [x] Reproduce the gate and remove duplication. The Agent shares its Moonshot wire profiles and MCP request builder.
   - [x] Pass the unchanged local 2.962% ceiling (6,983 duplicated lines out of 236,042).
   - [ ] Verify hosted Format & Analyse on the updated PR head.
6. **WASI sysroot downloads [OspreyCodex]**
   - [x] Confirm wasm32, Rust packages, VS Code extension and proof-kernel jobs received HTTP 504 during setup.
   - [x] Retry the pinned download with bounded backoff. The local HTTP probe recovers from one 504 and fails after four attempts on persistent 504s.
   - [ ] Verify the four hosted jobs and the downstream coverage reports on the updated PR head.
