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

## Rules

- [x] **[CI-PUSH-LOCK]** Only the agent holding the TMC lock `PUSH-<branch>` pushes. Every other agent commits locally and tells the holder on TMC which commits to include.
- [x] **[CI-PUSH-GREEN]** A push follows a `make ci` pass on the exact commit being pushed. CI checks for a clean checkout at both ends of the run and verifies that HEAD still names the starting commit before writing approval.
- [x] **[CI-PUSH-ONE-RUN]** There is no push while a CI run on the branch is in progress. The only exception is the fix for that run's red, pushed after it finishes.
- [x] **[CI-PUSH-RED-OWNER]** The lock holder owns the run until it finishes. If it is red, they fix it locally, run `make ci` again and push once.

## Enforcement

- [x] **`make push`:** refuses unless `make ci` left a stamp for `git rev-parse HEAD` and `gh run list --branch <b> --status in_progress` is empty. It runs `git push` only after both checks pass. AGENTS.md replaces raw `git push` with `make push`.
- [x] **`ci.yml` concurrency:** `cancel-in-progress: false` in `ci.yml` and the workflows it calls (Windows, GPUI, WinUI). A new push then waits behind the running run instead of destroying it. GitHub keeps only the newest pending run, so this costs at most one queued run.
- [x] **Synth tests 16 and 19:** fixed by `d591e96a5`. At HEAD they pass locally under GC and ARC (byte-exact golden, ARC leak-free).
- [x] **WinUI synth goldens:** refreshed by `3d8588a23`. `970615da3` added `undo-delay` and undo/redo steps but no goldens, and `d591e96a5` removes the oscillator's MIDI inlet, which changes every synth picture. Goldens can only be drawn on Windows, so take them from the `winui-self-test` artifact of the first run at HEAD, check each picture by eye and push them as that run's red fix.

## macOS screenshot goldens

macOS and Windows cannot share goldens: the apps run on different native hosts (SwiftUI/AppKit and GPUI on Metal with CoreText; WinUI 3 on Direct3D with DirectWrite), so text pixels differ far beyond the comparers' tolerance (24 per channel in at most 0.5% of pixels on macOS, 0.2% on Windows). Even on macOS the hosted runner and a local Retina display need separate sets (`agent/goldens/macos-26-ci` and `macos-26`). Today only `agent` compares macOS pictures; every app has Windows goldens.

- [ ] GPUI host compares against committed goldens (synth, casting, flock, painting, probe, signal-flow): a compare and an update mode in `src/toolchain/hosts/gpui/src/self_test.rs`, reusing the tolerance rule of the Swift host's `SelfTest.swift`. Its README states that PNG capture is not compared yet
- [ ] `<app>`, `<app>-headless` and `<app>-update` targets in `src/toolchain/targets/macos/tests/Makefile`, writing `<app>/goldens/macos-26-ci/`
- [ ] `tally` and `storage` take pictures on macOS (today a view check and a smoke run) and commit `goldens/macos-26-ci/`
- [ ] `gpui.yml` runs the compare on `macos-latest` as part of the required `CI` check

