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
   - [x] Pass the unchanged coverage-instrumented test locally in 69.80 seconds. Run 42125986216 still exceeded 120 seconds on Linux.
   - [x] Reuse free-variable sets across obligation-closure passes. The regression records two relation scans instead of six, and the unchanged local budget test passed in 61.27 seconds.
   - [x] Collect monomorphic free variables directly. The regression uses one scratch set instead of 129, and the unchanged local budget test passed in 57.40 seconds.
   - [x] Verify the unchanged test in the hosted Rust workspace rest shard. Run 42142978360 passed in 117.30 seconds with the early environment snapshot drop.
   - [x] Confirm the unchanged test passed again in run 42145657288 in 118.55 seconds.
5. **Deslop gate [OspreyCodex]**
   - [x] Reproduce the gate and remove duplication. The Agent shares its Moonshot wire profiles and MCP request builder.
   - [x] Pass the unchanged local 2.962% ceiling (6,983 duplicated lines out of 236,042).
   - [x] Verify hosted Format & Analyse on run 42125986216, job 127592574122.
6. **WASI sysroot downloads [OspreyCodex]**
   - [x] Confirm wasm32, Rust packages, VS Code extension and proof-kernel jobs received HTTP 504 during setup.
   - [x] Retry the pinned download with bounded backoff. The local HTTP probe recovers from one 504 and fails after four attempts on persistent 504s.
   - [x] Verify hosted Rust packages, proof kernel and VS Code extension passed on run 42125986216.
   - [x] Verify WebAssembly goldens. The pinned download and runtime build passed on run 42125986216, then WebAssembly passed all 236 goldens on run 42129545423 and passed again on run 42131518920.
   - [ ] Verify the aggregate coverage gate after the Rust rest shard produces its report.
7. **macOS Agent UI image paste [check-once-claude]**
   - [x] Verify all 14 image-paste steps passed on the first attempt in run 42131518920, job 127613956956.
   - [x] Verify the paste-focus fix on run 42147919564, job 127666202881. All 14 image-paste steps and the full Agent UI job passed on the first attempt.
8. **Website guide illustrations [claude-docs-effects]**
   - [x] Reproduce the illustration requirement failure from run 42131518920, job 127611007495.
   - [x] Verify the replacement effects illustrations with the exact Playwright check and the full local website suite (318 passed).
   - [x] Verify hosted Website E2E after the replacement illustrations are pushed. Run 42145657288, job 127656379768, passed.
9. **Mapped-file tensor fixture [OspreyCodex]**
   - [x] Reproduce the OpenCL driver refusal from run 42145657288 with that run's compiler and runtime archives in Ubuntu 24.04 amd64. The host product and all mapped-file diagnostics matched.
   - [x] Check the typed refusal on Linux and preserve exact parity and dispatch traces when a device runs. Host and Metal both passed locally.
   - [x] Verify the complete hosted Rust rest shard and its coverage artifact. Run 42150529203, job 127674155353, passed the mapped-file test and uploaded artifact 13473265612. The Agent budget passed in 117.79 seconds.

10. **VS Code working-session capture race [check-once-claude]**
   - [x] Reproduce: run 42150529203, job 127674155162, timed out after 1000ms waiting for `aria-busy` in `capture` because typing re-runs the WASM program.
   - [x] Fix in `06eb5dcb7`: `workingSession` waits until the re-run settles, as every other scene does. The VS Code extension job passed on run 42153399114.
11. **Qwen and transformers scope (user decision, 2026-11-06)**
   - [x] `c2cee10f0` removes the unfinished qwen model from the package backend matrix; the existing GPT2 tests stay.
   - [x] `98eb859d8` marks transformers `unfinished`, so it still reports coverage but has no 90% floor. Package coverage passed 28 of 28 on run 42153399114.
12. **Agent type-check budget [OspreyCodex]**
   - [x] Reproduce the timeout. The three-project check took 117.79 seconds in run 42150529203, then `test-app` exceeded the shared 120-second deadline in run 42153399114. A local profile showed repeated full inference in the annotation lint.
   - [x] Index numeric literal defaults by variable ID and reuse the free-variable set after defaulting. All 958 type tests and strict Clippy passed, and the unchanged coverage-instrumented Agent test finished in 41.97 seconds locally. Paired `test-app` CPU time was 41.59 versus 42.90 seconds with identical warnings.
   - [x] Run 42157768962 still hit the 120-second deadline in `test-app`. A local trace found 40 rejected annotation trials across 39 single-site groups, so batching groups could not help this workload.
   - [x] Defer the baseline effect proof until a candidate could produce a warning. The regression records no additional effect proof for a pinned annotation and verifies that a warning still needs an exact proof. All 959 type tests and strict Clippy passed. The paired `test-app` check improved from 40.83 to 36.14 seconds with byte-identical warnings, and the unchanged local Agent budget passed in 37.90 seconds.
   - [x] Stop a singleton trial after its declaration when the inferred signature and baseline are both concrete and differ. The regression counts two visits to an unrelated later body before the fix and one after. Unresolved signatures still visit it twice. All 960 type tests and strict Clippy passed. The unchanged local Agent gate passed in 34.90 seconds, and the paired `test-app` check improved from 36.59 to 33.52 seconds with identical warnings.
   - [ ] Verify the unchanged Agent budget and full Rust rest shard on hosted Linux.
13. **Integration job timeout (external)**
   - [x] Run 42153399114 hit the 18-minute limit because the Azure Ubuntu mirror served the Playwright `--with-deps` font packages at 49.6 kB/s (7 min 20 s for 21.9 MB). The previous run installed them in 19 s. No code change; the next push reruns the job.

## Status (run 42138521783)

- Fixed and green: WinUI paths and setup steps, the GPUI release gate's Makefile lookup, deslop,
  the WASI download retry and wasm-only setup (OspreyCodex `45a12a2d`), and the website media gate.
  The website images were replaced with new captures by claude-docs-effects in `c35de647a`, on the
  user's order not to use the old Log screenshots.
- Agent UI `paste-image` (run 42145657288): the self-test posted Cmd-V once after the model picker
  and timed out waiting for the image marker. Commit `0d20d9895` waits for a text responder before
  posting Cmd-V. The next hosted run passed both the marker and model-request checks.
- Runs 42142978360 and 42145657288 passed the Agent budget. The mapped-file fixture failure prevented
  the Rust rest shard from uploading coverage in both runs.
