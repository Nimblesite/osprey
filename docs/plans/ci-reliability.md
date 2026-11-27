# CI reliability and coordinated pushes

Goal: one exact, fully verified commit reaches each branch push, no parallel agent cancels an in-flight run, and the platform test suites give deterministic results without retries, wider timeouts or timing races.

## 🛑 Zero warnings: CI fails on ANY warning [CI-ZERO-WARNINGS]

- [ ] **CI MUST fail if the repository has even one warning.** Every lint the CLI or the language server reports at `warn` or above fails the required `CI` check. That covers `redundant-annotation`, `repeated-literal`, `unused-*`, `unused-import`, `closing-run` and the rest, plus compiler warnings. It applies to every tracked program: `src/osprey/tests/`, `src/osprey/examples/`, `src/osprey/packages/`, benchmarks and website snippets. The only exceptions are `failscompilation/` and the deliberate `test-skipped` cases. Today `scripts/ci/check-programs.py` only appends warnings to its log, so this one, shown in the editor, stays green in CI: `src/osprey/tests/core/collections/nested_generic_collections.test.osp:19:52 redundant-annotation: redundant type annotation on parameter `present` of `chooseByCollection`: inference derives `U` without it`. Done means:
  - `make _check_programs` passes `--warnings-as-errors`, or fails on any `warning:` line;
  - its test (`scripts/ci/test-check-programs.mjs`) proves that a planted warning turns the job red;
  - the CLI and the editor report the same warnings, so an editor-only warning is a bug;
  - every existing warning is fixed in the same change, never suppressed by `[lints]` or `allow`.

## Push protocol

- **[CI-PUSH-LOCK]** Only the agent holding the TMC lock `PUSH-<branch>` pushes; every other agent commits locally and messages the holder which commits to include.
- **[CI-PUSH-GREEN]** A push follows a `make ci` pass on the exact commit being pushed: `target/ci-green` equals `git rev-parse HEAD`. CI checks for a clean checkout at both ends of the run, verifies that HEAD still names the starting commit before writing approval, and finishes source validation and the dependency audit before lint and test even with parallel Make.
- **[CI-PUSH-ONE-RUN]** No push while a CI run on the branch is queued or in progress; the only exception is the fix for that run's red, pushed after it finishes.
- **[CI-PUSH-RED-OWNER]** The lock holder owns the run to green: on red, fix locally, run `make ci` again and push once with `git push origin "$tested:refs/heads/$branch"`.
- `cancel-in-progress: false` stays in `ci.yml` and in every workflow it calls (`ci-windows.yml`, `gpui.yml`, `winui.yml`).

To locate duplication in a change, run `git diff --no-ext-diff origin/main -- | target/ci-tools/deslop . --diff - --only-changed --nohtml --nojson --output target/deslop-pr`; `make _deslop` (inside `make lint`, hence `make ci`) remains the whole-repository gate within the `.deslop.toml` ceiling. `node scripts/ci/preflight.mjs --list --only='gpui\.yml:|winui\.yml:'` lists the desktop replay commands, each run on its native host. A diagnostic build must surface the first source-proof specialization error [TYPE-EFFECT-PROOF-COUNT].

## Flake-fix contract

Root-cause fixes only: no retries, no wider timeouts hiding a race, no skipped tests; replace sleep-then-assert with an observable signal. Tick an item only after its suite passes twice in a row under `make -j9`, beside its AddressSanitizer twin. Loopback tests take one process-wide, bindable port block selected before `main`, propagated with `OSP_TEST_PORT_BASE` and read through `test_port(offset)` [CI-TEST-PORTS]; no fixed cross-suite ports or per-translation-unit port bases.

## Runtime and Rust test timing

- [ ] Replace the remaining wall-clock ceilings in `src/toolchain/compiler/runtime/http_read_tests.c` (`expect_prompt`, the malformed-head cases, the status-only builtins, the `expect_linear_scan` bound) and `http_tls_tests.c` (the 3 s two-handshake bound) with assertions on the deadline mechanism, keeping the lower-bound timeout checks without failing because a loaded runner delayed observation.
- [ ] `package_matrix.rs` (`src/toolchain/crates/osprey-cli/tests/common/`) still requires genuine worker overlap within 2 s: keep the ordering assertions deterministic and make the overlap observation robust to an oversubscribed runner without silently weakening the scheduler contract.
- [ ] `bounded.rs` in the same folder polls process-group RSS by spawning `ps` every 50 ms: reduce watcher overhead and keep every caller's deadline configurable for cold or loaded CI runners.
- [ ] `agent_check_budget.rs` gates lowering growth on a wall-clock ratio: count IR growth instead.
- [x] VS Code `profiler-cpu-sessions.test.ts` and `profiler-cpu-e2e.test.ts` sleep before asserting samples arrived and `debug-panel.test.ts` sleeps 30 ms before asserting an async update: wait for the samples and UI state. (done: each sleep polls the listed session or the panel state it asserts)
- [ ] Multi-shot review leftovers [MULTI-HANDLE-MANY]: compile time grows about cubically with many resumes in one arm and overflows the Rust stack near 10,000; resume is capped near 512 by the syntax nesting limit; the native stack overflows at a depth of 8,000; the false rejections of a closure built from resume results and of a handler value passed as a parameter returned when `113404804` was reverted (`7a4d8b04e`). Check: a named regression in `src/osprey/tests/regressions/effects/` for each.

## Job deadlines and capacity

- [ ] Size job timeouts from measured p95 durations: `Tests: C runtime (coverage)` (`test-c-runtime`) still has a 14-minute limit over concurrent coverage and ASan suites, and seven jobs had overruns on reruns that later went green; split sanitizer work without running suites twice.
- [ ] Shorten the next critical paths, the `Tests: Rust workspace (rest)` shard (17 min) and the WinUI job (16 min), and the macOS runner wait (iOS Inbox started 13 min late); keep the 16-worker pool, whose 15m13s median beat the batched baseline against the 17-minute target.
- [ ] (verify) The hosted gates last observed green at `4f3215fa8` stay green on the current push: the FerroUI dependency audit, the desktop replays within the 20-minute limit, the Linux DAW suite and the WinUI Prism replay.

## App and UI harness timing

- [x] Talon Bank iOS journey: `hold ()` in `src/osprey/examples/talon/backend/main.ospml` stops serving on any `readFile` error; distinguish an absent marker from transient read failures (`ledger.js` prints server output when it exits while serving).
- [x] VS Code workbench-browser profiler tests: the 3000 ms default locator timeout and the Library button's 5 s idle wait in `test/suite/profiler-kit.ts` have failed on attempts that later went green; wait on the actual workbench state with a CI-appropriate bound. (done: the waits already watch `aria-busy`, enablement and frames; every one now shares `UI_MS`, 15 s times `OSPREY_TEST_DEADLINE_SCALE`, under a suite timeout of four of them)
- [x] The dead-worker call must settle before the event loop turns once; the DAP harness's 30 s session/stop deadlines in `dap-harness.ts` CI-configurable while retaining hang detection (done: `OSPREY_TEST_DEADLINE_SCALE` >= 1 multiplies every DAP and mocha deadline, refused when malformed).
- [ ] WinUI: `Undrawn` in `src/toolchain/hosts/winui/OspreyHost/SelfTest.cs` still takes an undrawn layout as drawn after 3 s (fail-open); `goldens.ps1` compares raw pixels sensitive to font, GPU, DPI and compositor changes (pin rendering inputs or assert structurally on text-bearing regions); `self-test.ps1` names the unfinished step when the journey budget expires.
- [ ] WinUI `CapabilitiesTests.cs` must observe request start before cancelling instead of `Task.Delay(300)`; `src/osprey/examples/agent/self-test.ps1` must use a per-run workspace instead of deleting the shared `osprey-agent-work` directory.
- [ ] `scripts/mobile/window-picture.ps1` must poll for a real window rather than sleep once for 20 s; the storage app's C-drive self-test (`src/osprey/examples/storage/self-test-drive.json`) must expose walk progress or completion instead of its 60 s and 45 s sleeps.
- [ ] `src/toolchain/hosts/synth_vst/build.py`: replace the PID-reuse teardown probe and the 5 s VST graph response deadlines with child-handle or exit-sentinel synchronization sized for payload and CI load; keep the queue-empty poll event-driven.
- [ ] Bank browser goldens: pin Chromium and fonts to the recording image, or keep screenshots as artifacts while accessible journey state supplies the assertions (the 0.5% full-page pixel budget is renderer-sensitive).
- [ ] Website tests: scale the 20–30 s compiler, wasm/SQLite and playground startup bounds for loaded CI and report timeout separately from assertion failure; replace absolute decoded-frame thresholds and short playback polls with readiness and time-progress assertions; separate the site build from Playwright's 120 s `webServer` budget in `src/toolchain/website/playwright.config.js`.
- [ ] Port `src/toolchain/hosts/synth_vst/build.py` to Python-flavour Osprey: `_prism_ui_test` and `_prism_vst_ui_test` (`scripts/make/apps.mk`) run it with `python3`, which AGENTS.md bans.

## macOS screenshot goldens

macOS and Windows cannot share goldens, because their native text renderers differ beyond the comparers' tolerance (24 per channel in at most 0.5% of pixels on macOS and 0.2% on Windows); hosted and local Retina runs need separate sets (`agent/goldens/macos-26-ci` and `macos-26`). Every GPUI-based app (the probe, Flock, Painting, signal-flow, Casting, Prism's `_prism_ui_test` and the daw's `daw-gpui` fallback) is being retargeted to the FerroUI host (`src/toolchain/hosts/ferroui/`): no new GPUI work is planned, the compare lands in the FerroUI self-test, and `src/toolchain/hosts/gpui` is deleted once nothing uses it.

- [ ] Retarget probe, flock, painting, signal-flow and casting to the FerroUI host: `make -C src/toolchain/targets/macos <app>` opens each on FerroUI and its self-test runs there [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Prism and the daw's `daw-gpui` fallback to the FerroUI host: `make -C src/toolchain/targets/macos synth` and `daw` open on FerroUI and `_prism_ui_test` runs without `GPUI_HOST_BIN` [NATIVE-FERROUI-RETARGET].
- [ ] The FerroUI self-test compares against committed goldens in a compare and an update mode, reusing the tolerance rule of the Swift host's `src/toolchain/hosts/swift/Sources/OspreyShared/Nodes/SelfTest.swift`; the GPUI self-test writes PNGs under `target/gpui-self-test/` without comparing them.
- [ ] `<app>`, `<app>-headless` and `<app>-update` targets in `src/toolchain/targets/macos/tests/Makefile` write `<app>/goldens/macos-26-ci/`.
- [ ] `tally` and `storage` take pictures on macOS (today a view check and a smoke run) and commit `goldens/macos-26-ci/`.
- [ ] The desktop app workflow (`gpui.yml`, renamed when the GPUI host goes) runs the compare on `macos-latest` as part of the required `CI` check.
