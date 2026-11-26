# CI reliability and coordinated pushes

Goal: one exact, fully verified commit reaches each branch push, in-flight CI is never cancelled by a parallel agent, and platform test suites produce deterministic results without retries or hidden timing races.

## Push protocol

- **[CI-PUSH-LOCK]** Only the agent holding the TMC lock `PUSH-<branch>` pushes. Every other agent commits locally and tells the holder on TMC which commits to include.
- **[CI-PUSH-GREEN]** A push follows a `make ci` pass on the exact commit being pushed. CI checks for a clean checkout at both ends of the run and verifies that HEAD still names the starting commit before writing approval. Source validation and the dependency audit finish before lint and test even with parallel Make.
- **[CI-PUSH-ONE-RUN]** There is no push while a CI run on the branch is queued or in progress. The only exception is the fix for that run's red, pushed after it finishes.
- **[CI-PUSH-RED-OWNER]** The lock holder owns the run until it finishes. If it is red, they fix it locally, run `make ci` again and push once.
- The lock holder verifies that `target/ci-green` matches `git rev-parse HEAD`, checks GitHub for queued or running CI on the branch, and pushes the verified SHA with `git push origin "$tested:refs/heads/$branch"`.
- `cancel-in-progress: false` remains in `ci.yml` and its called Windows, GPUI and WinUI workflows so a new push waits instead of destroying an active run.

## Verification before pushing

- [ ] **Build and coverage.** Verify hosted build and artifact producers on the intended push commit. Local all-target workspace Clippy must pass with warnings treated as errors, the wide-record regression must verify both selected callbacks, a clean Talon web build must pass with ABI generation, and the exact synth coverage gate must pass.
- [ ] **Duplication.** Run `make _deslop` on the intended push commit after removing remaining duplication. The shared pinned Deslop gate must stay within the ceiling in `.deslop.toml`; the arrangement cases must pass in GC and ARC with shared fixture setup, and the focused transport journey must retain its marker and zoom assertions.
- [ ] **The full `make ci` was not run before the 88dc8d98a push.** The failing jobs were reproduced and fixed locally one by one instead. Run the complete gate on the exact commit rather than treating focused reproductions as its replacement.
- [x] **Dependency audit.** FerroUI revision `0cdbd6f73` includes both upstream merges (#1029 and #1030). The release-tree audit at `4f3215fa8` scanned all 196 packages in the tracked FerroUI lock and passed. [Hosted audit](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602113552)
- [x] **GPUI time limits.** Both hosted replays passed at `4f3215fa8` within the 20-minute limit. Linux completed in 11m13s and macOS in 12m29s. [Linux](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209614) · [macOS](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209495)
- [x] **DAW suite on Linux.** The hosted DAW suite at `4f3215fa8` completed all 39 tests with zero failures or skips. [Hosted suite](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209567)
- [x] **WinUI Prism replay.** Prism completed its 62-step Windows journey at `4f3215fa8`, including the module library and inspector. All 11 screenshot goldens passed. [Windows replay](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602217783)

Run `make _deslop` and the release audit before the app builds. To locate duplication in a change, run `git diff --no-ext-diff origin/main -- | target/ci-tools/deslop . --diff - --only-changed --nohtml --nojson --output target/deslop-pr`; `make _deslop` remains the whole-repository gate. Use `node scripts/ci/preflight.mjs --list --only='gpui\.yml:|winui\.yml:'` to locate desktop replay commands, running each on its native host. A diagnostic build must surface the first source-proof specialization error [TYPE-EFFECT-PROOF-COUNT].

### Duplication design constraints

These approaches remain available for clones that cannot yet be merged without new language or test support:

- Operation-polymorphic handler factories [EFFECTS-HANDLER-VALUE] would share one audited arm shape across operations instead of repeating count/log/resume/log/transform arms.
- An importable `tests/support` module or more helpers in the prelude's `testing.osp` would remove copied corpus helpers while preserving standalone test programs.
- TypeDiagram-derived JSON codecs would replace field-by-field save/restore and `*Json`/`*Of` mirrors.
- A generic `uitest::Bench S A` would replace each app's duplicate `Bench` effect and wrappers; it needs effect type parameters across packages in ML.

## Flake-fix contract

Use root-cause fixes only: no retries, wider timeouts used to hide a race, or skipped tests. Replace sleep-then-assert with an observable signal. Tick an item only after its suite passes twice in a row under `make -j9`, beside its AddressSanitizer twin.

Loopback tests keep one process-wide, bindable port block selected before `main`, propagated with `OSP_TEST_PORT_BASE`, and accessed through `test_port(offset)` [CI-TEST-PORTS]. Do not reintroduce fixed cross-suite ports or per-translation-unit port bases.

## Runtime and job timing

- [ ] Replace the wall-clock upper bounds in `http_read_tests.c` and `http_tls_tests.c` with assertions on the deadline mechanism. Preserve the lower-bound timeout checks without failing merely because a loaded runner delayed observation.
- [ ] Remove the dribbled-response margin races in `http_read_tests.c`: derive drip counts from the timeout or drive the origin from a monotonic deadline so the success case stays comfortably inside its whole-read bound and the cut-off case stays outside it.
- [ ] `package_matrix.rs` still requires genuine worker overlap within 2 s. Keep ordering assertions deterministic and make the overlap observation robust to an oversubscribed runner without silently weakening the scheduler contract.
- [ ] `bounded.rs` polls process-group RSS by spawning `ps` every 50 ms. Reduce watcher overhead and ensure each caller's deadline is configurable for cold or loaded CI runners.
- [ ] Replace the remaining loopback wall-clock ceilings: `expect_prompt` and the 16 MB scan in `http_read_tests.c`, and the two-handshake exchange in `http_tls_tests.c`. Assert functional results or use a measured, sanitizer-aware mechanism rather than scheduler-sensitive ceilings.
- [ ] **Open code-sweep timing checks:** VS Code `profiler-cpu-sessions.test.ts` and `profiler-cpu-e2e.test.ts` sleep before asserting samples arrived, `debug-panel.test.ts` sleeps 30 ms before asserting an async update, and `agent_check_budget.rs` gates lowering growth on a wall-clock ratio. Wait for samples/UI state and count IR growth instead.
- [ ] **Job deadlines.** `Tests: C runtime (coverage)` still has a 14-minute limit over concurrent coverage and ASan suites, and seven jobs have overruns on reruns that later went green. Measure p95 durations and size each timeout with headroom, or split sanitizer work without running suites twice.

## UI and app harnesses

- [ ] **Talon Bank iOS journey: the ledger server dies mid-run.** `hold ()` in `talon/backend/main.ospml` stops serving on any `readFile` error, not only a missing marker. Distinguish an absent marker from transient read failures; `ledger.js` now prints server output when it exits while serving.
- [ ] **VS Code workbench-browser profiler tests:** the 3000 ms locator timeout in `profiler-kit.ts` and the Library button's 5 s idle wait have failed on attempts that later went green. Wait on the actual workbench state with a CI-appropriate bound.
- [ ] **WinUI capture readiness.** Replace fixed `Settle` delays and fail-open `Undrawn` capture in `SelfTest.cs` with stable rendered-frame or animation-completion signals. In `self-test.ps1`, fail when no window handle appears or `SetForegroundWindow` fails instead of continuing to misleading golden diffs.
- [ ] **WinUI golden stability and total deadline.** Raw pixels remain sensitive to font, GPU, DPI and compositor changes; pin rendering inputs or use structural assertions for text-bearing regions. Budget `self-test.ps1`'s 300 s kill from the script's steps and 90 s per-step patience, and name the unfinished step on timeout.
- [ ] **`synth_vst/build.py` deadlines.** Replace the 3 s PID-reuse teardown probe with child-handle or exit-sentinel synchronization, and size 5 s VST graph response deadlines for payload and CI load. Keep the queue-empty poll event-driven rather than returning to a fixed sleep.
- [ ] **Bank browser goldens.** Pin Chromium and fonts to the recording image, or keep screenshots as artifacts while accessible journey state supplies the assertions; the current 0.5% full-page pixel budget is renderer-sensitive.
- [ ] **Website compiler and startup deadlines.** Scale the 20–30 s compiler, wasm/SQLite and playground startup bounds for loaded CI, batch compiler invocations where possible, and report timeout separately from assertion failure.
- [ ] **Website video assertions.** Replace absolute decoded-frame thresholds and short playback polls with readiness/time-progress assertions and explicit generous bounds; software decode throughput is machine-dependent.
- [ ] **Website test startup.** Separate the site build from Playwright's 120 s `webServer` budget so a cold Eleventy/wasm build cannot consume server startup time.
- [ ] **Windows and storage sleeps.** `window-picture.ps1` must poll for a real window rather than sleeping 20 s once. Storage's C-drive self-test must expose walk progress/completion instead of `sleep 60` and `sleep 45` guesses.
- [ ] **VS Code timing and DAP bounds.** Replace the 2 s dead-worker wall-clock assertion with its rejection mechanism, and make the DAP harness's 30 s session/stop deadlines CI-configurable while retaining hang detection.
- [ ] **WinUI cancellation and agent isolation.** `CapabilitiesTests.cs` must observe request start before cancelling instead of `Task.Delay(300)`. `agent/self-test.ps1` must use a per-run workspace instead of deleting the shared `osprey-agent-work` directory.
- [ ] **Windows Prism docked panel:** a white toolbar strip where Undo and Redo belong, and a light-theme panel beside the dark canvas. Logged as [daw.md](synth/daw.md) item 40. The 100% goldens record the current look; the 150% goldens still show the deleted keyboard dock.
- [ ] **Canvas scale with the panel docked.** The user said "scale a bit off". With [DAW-LAYOUT-FIXED-SCALE] at one unit per point, a 1152-point window with the 320-point panel hides the right of the patch. Decide whether a docked panel narrows the page or the canvas fits its pane.
- [ ] **CPython in CI.** `_prism_ui_test` and `_prism_vst_ui_test` run `python3 src/toolchain/hosts/synth_vst/build.py`, which AGENTS.md bans. Port it to Python-flavour Osprey.

## Compiler and runtime scale

- [ ] **Multi-shot review leftovers** ([MULTI-HANDLE-MANY]):
  - compile time grows about cubically with many resumes in one arm, and overflows the Rust stack near 10,000
  - resume is capped near 512 by the syntax nesting limit
  - the native stack overflows at a depth of 8,000
  - false rejections #5 and #7 (a closure built from resume results; a handler value passed as a parameter) came back when 113404804 was reverted, because it broke cleanarch_todo

## CI capacity

- [ ] `Tests: Rust workspace (rest)` (17 min) and WinUI (16 min) are the next critical paths. macOS jobs also wait for runners: iOS Inbox started 13 min late.
- [x] Compared the 16-worker pool with the [batched baseline](https://github.com/Nimblesite/osprey/actions/runs/45027767330) (17m23s for the job, 16m34s compiling). The four completed checks after the change all passed. Whole-job times in order were [12m26s](https://github.com/Nimblesite/osprey/actions/runs/45067066085), [15m27s](https://github.com/Nimblesite/osprey/actions/runs/45138676397), [14m59s](https://github.com/Nimblesite/osprey/actions/runs/45140038913) and [17m58s](https://github.com/Nimblesite/osprey/actions/runs/45143088617). Compilation took 11m24s to 16m39s. Keep one runner given the 15m13s median against the 17-minute target.

## macOS screenshot goldens

macOS and Windows cannot share goldens because their native text renderers differ beyond the comparers' tolerance: 24 per channel in at most 0.5% of pixels on macOS and 0.2% on Windows. Hosted and local Retina runs need separate sets (`agent/goldens/macos-26-ci` and `macos-26`).

- [ ] GPUI host compares against committed goldens (synth, casting, flock, painting, probe, signal-flow): a compare and an update mode in `src/toolchain/hosts/gpui/src/self_test.rs`, reusing the tolerance rule of the Swift host's `SelfTest.swift`. Its README states that PNG capture is not compared yet
- [ ] `<app>`, `<app>-headless` and `<app>-update` targets in `src/toolchain/targets/macos/tests/Makefile`, writing `<app>/goldens/macos-26-ci/`
- [ ] `tally` and `storage` take pictures on macOS (today a view check and a smoke run) and commit `goldens/macos-26-ci/`
- [ ] `gpui.yml` runs the compare on `macos-latest` as part of the required `CI` check
