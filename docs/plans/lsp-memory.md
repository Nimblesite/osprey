# Language server memory and lspkit upgrades

Goal: an idle `osprey lsp` on this repository stays bounded, queued requests remain cancellable, and Osprey uses lspkit's leak repairs without exposing its affected APIs.

## Memory and API contracts

### [LSPKIT-UPGRADE-SOURCE] lspkit exposure

Osprey pins `lspkit`, `lspkit-server` and `lsp-profiler` at `0.10.1` and `@nimblesite/lsp-profiler-ui` at `^0.10.1`. Preserve these boundaries when upgrading or adding calls:

| lspkit leak surface | Osprey boundary |
| --- | --- |
| Sidecar correlator and dropped inbound readers | Osprey does not use the sidecar correlator; it reads with `read_message` on its own loop |
| Pre-registered ingress controls | Use `Dispatcher::dispatch`, which registers after finding a handler; do not introduce `CancellationHandle::register` without proving cancellation entries are reclaimed |
| Reference-server closed documents and handler cycles | Reference example only; Osprey closes documents in `Vfs`, and handlers capture the engine rather than the dispatcher |

### [LSPKIT-UPGRADE-OSPREY-LEAK] Project retention

`SourceCache::kept` retains each open document's own project plus only the most recently loaded closed projects. A shared source open in many dependents must not pin all of them. Keep `an_open_shared_file_does_not_pin_its_dependents` and `the_sweep_releases_closed_projects_and_keeps_open_ones` as the retention regressions.

Queued profiler requests honor `$/cancelRequest` through `server/work/pending.rs`; excess requests receive `ServerCancelled` without ending the session. Keep the real transport and `live_profiler_lsp` coverage for those behaviors.

Rust stays pinned to 1.100.0, the version lspkit builds with [TOOLCHAIN-RUST-PIN].

## [LSPKIT-UPGRADE-STEPS] Remaining release steps

- [ ] Run `make ci` on the exact push commit.
- [ ] Push under the TMC lock `PUSH-<branch>` once `make ci` is green

## Manual and hosted verification records

These checked claims have no repository-local artifact that proves the measurement or hosted result, so they remain pending independent evidence rather than being presented as source-proven here.

- [x] Reproduce against the released 0.29.0 binary. An idle server's footprint grew past 5 GB in VS Code. In a 280 s run that only initializes, live heap reached 890 MB in 3.27M allocations and was still climbing.
- [x] Fix in `ddf589022`. Keep the projects of open documents plus the 4 most recently loaded, and evict the rest. After the same run, live heap is 128 MB in 317k allocations.
- [x] Pushed to `codex/prism-synth-routing`. Also in PR #411 to main: `fix/lsp-sweep-memory`, with fmt, clippy and the 257 LSP tests passing on its exact tree.
- [x] PR #411 passed CI and merged on 2026-11-16. On 2026-11-23 the installed v0.31.1 extension binary completed its diagnostics sweep of 1,407 files in 1,456 seconds. macOS `heap -s` measured 17,281,216 live bytes (16.48 MiB in 107,322 allocations), below the 128 MB limit. Physical footprint was 107.2 MB at idle and peaked at 956.6 MB.
- [x] Agreed on TMC (2026-11-16). lspkit's repairs and heap proofs are green locally, still `0.0.0-dev`, and have no release ETA. After its `make test` passes and the release is published, it sends `osprey-lsp-memory` the version and any API breaks in the surfaces above.
- [x] Fix any API breaks in `osprey-lsp`; no shims. None: it builds unchanged and the 259 LSP and 21 profiler tests pass.
- [x] `cargo test -p osprey-lsp`, then `make ci`, then one coordinated push. 261 LSP tests pass (2026-11-23); the bump shipped through PR #417 (lspkit 0.10.1), merged 2026-11-20 with the required `CI` check green, and is in v0.31.1.
- [x] Re-measure with `heap -s` after the idle sweep. On 2026-11-23 the worktree release build published diagnostics for all 1,406 files after initialize, then sent the diagnostics progress end notification at 1,280 s. macOS `heap -s` measured 17,293,072 live bytes (16.49 MiB, 107,338 allocations), below the 128 MB limit. Physical footprint remained 2.2 GB and peaked at 3.9 GB during the sweep.
- [x] `cargo test -p osprey-lsp` (261 passed), `-p lsp-profiler-osprey` (21 passed) and `--test live_profiler_lsp` (5 passed), 2026-11-23 at `4f3215fa8` + `0c5725f73`.
- [x] `make _test_vscode_extension` passed on 2026-11-23 with 986 development-host tests and 170 installed VSIX tests. `make _coverage_check_vscode_extension` passed all four thresholds.
