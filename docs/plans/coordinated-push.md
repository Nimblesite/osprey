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

- [ ] **[CI-PUSH-LOCK]** Only the agent holding the TMC lock `PUSH-<branch>` pushes. Every other agent commits locally and tells the holder on TMC which commits to include.
- [ ] **[CI-PUSH-GREEN]** A push follows a `make ci` pass on the exact HEAD being pushed. A commit added after that pass needs a new pass.
- [ ] **[CI-PUSH-ONE-RUN]** There is no push while a CI run on the branch is in progress. The only exception is the fix for that run's red, pushed after it finishes.
- [ ] **[CI-PUSH-RED-OWNER]** The lock holder owns the run until it finishes. If it is red, they fix it locally, run `make ci` again and push once.

## Enforcement (to build)

- [ ] **`make push`:** refuses unless `make ci` left a stamp for `git rev-parse HEAD` and `gh run list --branch <b> --status in_progress` is empty. It runs `git push` only after both checks pass. AGENTS.md replaces raw `git push` with `make push`.
- [ ] **`ci.yml` concurrency:** set `cancel-in-progress: false` for `pull_request`. A new push then waits behind the running run instead of destroying it. GitHub keeps only the newest pending run, so this costs at most one queued run.
- [ ] **Fix synth tests 16 and 19 and the WinUI `undo-delay` golden** before the next push of `chore/lspkit-0.10`.
