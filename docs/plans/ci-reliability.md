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

## macOS screenshot goldens

macOS and Windows cannot share goldens because their native text renderers differ beyond the comparers' tolerance: 24 per channel in at most 0.5% of pixels on macOS and 0.2% on Windows. Hosted and local Retina runs need separate sets (`agent/goldens/macos-26-ci` and `macos-26`).

- [ ] GPUI host compares against committed goldens (synth, casting, flock, painting, probe, signal-flow): a compare and an update mode in `src/toolchain/hosts/gpui/src/self_test.rs`, reusing the tolerance rule of the Swift host's `SelfTest.swift`. Its README states that PNG capture is not compared yet
- [ ] `<app>`, `<app>-headless` and `<app>-update` targets in `src/toolchain/targets/macos/tests/Makefile`, writing `<app>/goldens/macos-26-ci/`
- [ ] `tally` and `storage` take pictures on macOS (today a view check and a smoke run) and commit `goldens/macos-26-ci/`
- [ ] `gpui.yml` runs the compare on `macos-latest` as part of the required `CI` check
