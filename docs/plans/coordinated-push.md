# Coordinated pushes: no cancelled or red CI from parallel agents

## Incident (2026-11-19, PR #417, `chore/lspkit-0.10`)

Six CI runs started on the branch within 13 hours. Four were cancelled, one failed and none passed. Each push from a different agent cancelled the run in progress (`cancel-in-progress: true` in `ci.yml`), so no run ever finished on a green commit.

Run [44498694913](https://github.com/Nimblesite/osprey/actions/runs/44498694913) at `40d11b111` failed four jobs from one root cause:

- **Tests: Rust workspace (packages):** `python_packages.rs:196`. Synth tests 16 and 19 failed under both GC and ARC.
  - Test 16 is "replace a playing graph…".
  - Test 19 is "offline clips render…", which failed with "kernel block has invalid capacity, component or state identities".
- **Tests: package source coverage:** synth coverage collection failed because those same tests failed.
- **Coverage thresholds:** `merge_lcov.py` found no packages shard `lcov.info`, and the synth report was missing. Both follow from the failure above.
- **WinUI host:** the synth goldens failed. The `undo-delay` screenshot has no golden.

That commit was pushed without a local `make ci` pass on the commit itself.

## PR #427 verification

The [hosted run for `d4341c9ac`](https://github.com/Nimblesite/osprey/actions/runs/44901015355) finished red. The PR stays open while these checks are verified on a replacement commit.

- [x] **Dependency audit.** FerroUI revision `0cdbd6f73` includes both upstream merges (#1029 and #1030). The release-tree audit at `4f3215fa8` scanned all 196 packages in the tracked FerroUI lock and passed. [Hosted audit](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602113552)
- [x] **GPUI time limits.** Both hosted replays passed at `4f3215fa8` within the 20-minute limit. Linux completed in 11m13s and macOS in 12m29s. [Linux](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209614) · [macOS](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209495)
- [x] **DAW suite on Linux.** The hosted DAW suite at `4f3215fa8` completed all 39 tests with zero failures or skips. [Hosted suite](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602209567)
- [x] **WinUI Prism replay.** Prism completed its 62-step Windows journey at `4f3215fa8`, including the module library and inspector. All 11 screenshot goldens passed. [Windows replay](https://github.com/Nimblesite/osprey/actions/runs/45143088617/job/137602217783)
- [ ] **Build and coverage.** Local all-target workspace Clippy passes with warnings treated as errors after `a1183e8ed`, and the wide-record regression still verifies both selected callbacks. A clean Talon web build passes with ABI generation from `5ec9e5172`, and the exact synth coverage gate passes. Verify the hosted build and artifact producers on the replacement commit. [Clippy](https://github.com/Nimblesite/osprey/actions/runs/44901015355/job/136837186112) [Build](https://github.com/Nimblesite/osprey/actions/runs/44901015355/job/136837186146) [Coverage](https://github.com/Nimblesite/osprey/actions/runs/44901015355/job/136838272142)
- [ ] **Duplication.** The shared Deslop 0.41.0 gate reported 22,629 duplicated lines out of 551,993, 550 above the 4% limit. The arrangement cases pass in GC and ARC after sharing fixture setup, and the focused transport journey passes with the same marker and zoom assertions. Rerun `make _deslop` on the intended push commit after removing the remaining duplication.

### Local replay before the full build

- [x] Preflight preserves workflow, job and step environment variables, and runs each distinct execution context once. Windows steps use their declared shell. A selection that skips every check exits with a failure status. The regression tests cover an identical command with different GC and ARC settings, a Windows host, and an exact duplicate.
- [x] Preflight runs the compiler setup action's C runtime and release compiler builds before dependent tests. Jobs configured with `build: false` reuse those artifacts. The contract test verifies this order and the version setting.
- [x] `make ci` and the hosted GPUI job run the same `make -C src/toolchain/targets/tests synth APP_SUITE_PREBUILT=1` entry point. The local test phase runs it first with the cached release build.
- [x] Preflight writes a numbered log for each selected check and continues after a failure. Hosted CI and `make ci` run the same 93 contract tests. The actual preflight invocation passed the contract suite and logged the failing Deslop threshold using the shared, pinned `make _deslop` command.
- Run `make _deslop` and the release audit first. They catch dependency and duplication failures before the app builds. The Make target installs the same Deslop version as CI.
- To locate duplication involving the PR, run `git diff --no-ext-diff origin/main -- | target/ci-tools/deslop . --diff - --only-changed --nohtml --nojson --output target/deslop-pr`. The individual report entries show which parts of a clone occur on added lines. `make _deslop` remains the whole-repository gate.
- Build the release CLI once. Run the shared DAW gate and record elapsed time and peak resident memory. A successful replay must reach TAP and finish all assertions.
- [x] Verify the shared synth and UI test changes locally. All 91 synth package tests passed under GC and ARC. All 119 native UI tests passed under ARC. The `uitest` package source retained full line coverage.
- A timed solver trace attributes 33.2 s to `canvasInspectorJourney`, 35.9 s to `sampleFileJourney`, and 14.3 s to `panelDialJourney` in the seventh sweep. Moving the mutable bench state into its handler reduces whole-program sweeps from seven to two. All six focused voice and editor tests pass with this scoped state and the corrected voice assertion calls. Splitting the canvas Undo/Escape checks further reduced its seventh-sweep time to 0.15 s. A later replay entered the next proof phase with around 3 GiB resident. The full local TAP run completed after the inspector fixture repair.
- Sampling the stock release compiler located two source proofs. The first used the signal-forwarding specialization with 3,526 functions. Its error path fell back to the 3,520-function original. A diagnostic build must surface the first error to identify which specialization prevents reuse of the source proof [TYPE-EFFECT-PROOF-COUNT].
- Use `node scripts/ci/preflight.mjs --list --only='gpui\.yml:|winui\.yml:'` to locate the desktop replay commands. Run macOS checks on macOS and verify Linux resource use and WinUI goldens on their native runners. Preflight lists checks that require another host or GitHub runner data as skipped.
- The push lock holder runs `make ci` once the targeted checks are green and the branch is clean, then uses `git push` for the exact tested commit.

## Rules

- [x] **[CI-PUSH-LOCK]** Only the agent holding the TMC lock `PUSH-<branch>` pushes. Every other agent commits locally and tells the holder on TMC which commits to include.
- [x] **[CI-PUSH-GREEN]** A push follows a `make ci` pass on the exact commit being pushed. CI checks for a clean checkout at both ends of the run and verifies that HEAD still names the starting commit before writing approval. Source validation and the dependency audit finish before lint and test even with parallel Make.
- [x] **[CI-PUSH-ONE-RUN]** There is no push while a CI run on the branch is in progress. The only exception is the fix for that run's red, pushed after it finishes.
- [x] **[CI-PUSH-RED-OWNER]** The lock holder owns the run until it finishes. If it is red, they fix it locally, run `make ci` again and push once.

## Enforcement

- [x] **Direct push.** The lock holder verifies that `target/ci-green` matches `git rev-parse HEAD` and checks GitHub for queued or running CI on the branch. Push the verified SHA with `git push origin "$tested:refs/heads/$branch"`.
- [x] **`ci.yml` concurrency:** `cancel-in-progress: false` in `ci.yml` and the workflows it calls (Windows, GPUI, WinUI). A new push then waits behind the running run instead of destroying it. GitHub keeps only the newest pending run, so this costs at most one queued run.
- [x] **Synth tests 16 and 19:** fixed by `d591e96a5`. At HEAD they pass locally under GC and ARC (byte-exact golden, ARC leak-free).
- [x] **WinUI synth goldens:** refreshed by `3d8588a23`. `970615da3` added `undo-delay` and undo/redo steps but no goldens, and `d591e96a5` removes the oscillator's MIDI inlet, which changes every synth picture. Goldens can only be drawn on Windows, so take them from the `winui-self-test` artifact of the first run at HEAD, check each picture by eye and push them as that run's red fix.

## macOS screenshot goldens

macOS and Windows cannot share goldens: the apps run on different native hosts (SwiftUI/AppKit and GPUI on Metal with CoreText; WinUI 3 on Direct3D with DirectWrite), so text pixels differ far beyond the comparers' tolerance (24 per channel in at most 0.5% of pixels on macOS, 0.2% on Windows). Even on macOS the hosted runner and a local Retina display need separate sets (`agent/goldens/macos-26-ci` and `macos-26`). Today only `agent` compares macOS pictures; every app has Windows goldens.

- [ ] GPUI host compares against committed goldens (synth, casting, flock, painting, probe, signal-flow): a compare and an update mode in `src/toolchain/hosts/gpui/src/self_test.rs`, reusing the tolerance rule of the Swift host's `SelfTest.swift`. Its README states that PNG capture is not compared yet
- [ ] `<app>`, `<app>-headless` and `<app>-update` targets in `src/toolchain/targets/macos/tests/Makefile`, writing `<app>/goldens/macos-26-ci/`
- [ ] `tally` and `storage` take pictures on macOS (today a view check and a smoke run) and commit `goldens/macos-26-ci/`
- [ ] `gpui.yml` runs the compare on `macos-latest` as part of the required `CI` check

