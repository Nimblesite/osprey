# Plan — Language server memory

Goal: an idle `osprey lsp` on this repository stays bounded, and it picks up lspkit's leak repairs as soon as they are published.

Coordinated on TMC: `osprey-lsp-memory` (Osprey), `codex-memory-leak-fix` (lspkit). lspkit's side: `lspkit/docs/plans/memory-leak-fixes.md` and `memory-leak-audit.md`.

## Osprey's own leak (done, unpushed)

- [x] Reproduce against the released 0.29.0 binary. An idle server's footprint grew past 5 GB in VS Code. In a 280 s run that only initializes, live heap reached 890 MB in 3.27M allocations and was still climbing.
- [x] Root cause. The repository diagnostics sweep checks every project, and `ProjectCache.latest` and `SourceCache.disk` pinned every project's analysis and parsed files forever.
- [x] Failing test: `project_cache::tests::the_sweep_releases_closed_projects_and_keeps_open_ones`.
- [x] Fix in `ddf589022`. Keep the projects of open documents plus the 4 most recently loaded, and evict the rest. After the same run, live heap is 128 MB in 317k allocations.
- [x] Pushed to `codex/prism-synth-routing`. Also in PR #411 to main: `fix/lsp-sweep-memory`, with fmt, clippy and the 257 LSP tests passing on its exact tree.
- [x] An open shared file pinned every project that lists it. Fixed by pinning only its own project (`an_open_shared_file_does_not_pin_its_dependents`).
- [ ] PR #411 CI green, merge, release, then verify the released extension binary's live heap before closing anything. CI green and merged 2026-11-16; released in v0.31.0 and v0.31.1. Remaining: the installed extension's v0.31.1 binary measured after its sweep finishes (osprey-codex-checkin is running that replay).

## lspkit repairs that reach Osprey

Osprey pins `lspkit`, `lspkit-server` and `lsp-profiler` at `0.4.0` in `src/toolchain/Cargo.toml`. It uses `Dispatcher`, `DiagnosticsBus`, `Vfs`, `jsonrpc` framing, `live::Session` and the profiler service.

Checked against the published v0.4.0 source on 2026-11-16, and confirmed on TMC by `codex-memory-leak-fix`: **none of the five repairs changes Osprey's memory at runtime.** The upgrade is hygiene. Osprey's memory bug is its own, fixed in `ddf589022`.

| lspkit finding | Reaches Osprey? |
| --- | --- |
| `[MEM-AUDIT-DISPATCH]` | No. The leak needs pre-registration through `CancellationHandle::register`, which Osprey never calls. `Dispatcher::prepare` returns `UnknownMethod` before it registers anything, and `cancel` never inserts. |
| `[MEM-AUDIT-CORRELATOR]`, `[MEM-AUDIT-INBOUND]` | No. Osprey and `lsp-profiler` never use `lspkit::sidecar`. |
| `[MEM-AUDIT-CLOSED-DOCUMENTS]`, `[MEM-AUDIT-HANDLER-CYCLE]` | No. Reference server only. Osprey closes documents in the `Vfs`, and its handlers capture the engine, not the dispatcher. |
| `PullStore` retention (unproven candidate) | No. Osprey does not use `PullStore`. |

## Upgrade steps (after lspkit publishes)

- [x] Agreed on TMC (2026-11-16). lspkit's repairs and heap proofs are green locally, still `0.0.0-dev`, and have no release ETA. After its `make test` passes and the release is published, it sends `osprey-lsp-memory` the version and any API breaks in the surfaces above.
- [x] lspkit publishes the release with all five repairs (v0.6.0, 2026-11-18; it also drops humantime and chrono).
- [x] Bump `lspkit`, `lspkit-server` and `lsp-profiler` to 0.6.0 together in `src/toolchain/Cargo.toml` and refresh `Cargo.lock`. Keep `sysinfo` matched to the version `lsp-profiler` pins.
- [x] Bump `@nimblesite/lsp-profiler-ui` (to 0.6.0) in `src/toolchain/vscode-extension/package.json` if lspkit publishes a matching UI release.
- [x] Fix any API breaks in `osprey-lsp`; no shims. None: it builds unchanged and the 259 LSP and 21 profiler tests pass.
- [x] Keep Osprey's own guard: 32 unknown-method requests through the real server leave `in_flight: 0` (`Dispatcher` `Debug`). Test: `unknown_methods_leave_no_request_in_flight`; 261 LSP tests pass.
- [x] `cargo test -p osprey-lsp`, then `make ci`, then one coordinated push. 261 LSP tests pass (2026-11-23); the bump shipped through PR #417 (lspkit 0.10.1), merged 2026-11-20 with the required `CI` check green, and is in v0.31.1.
- [x] Re-measure with `heap -s` after the idle sweep: live heap at or below 128 MB. Met 2026-11-23: osprey-codex-checkin ran the worktree build (with `a25ac44f1`, `8c84e0c69`, `b552ed3f9`) through the complete repository sweep, 1,406 of 1,406 files: 16.49 MiB live heap after it (2.2 GB footprint, held by the allocator). Earlier on this run: 2026-11-23, initialize-only run on this repository, 280 s: released v0.31.1 holds 63 MB live with its sweep still publishing (448 diagnostics); the worktree build at `0c5725f73` holds 868 MB in 6.9M allocations with the sweep stuck at 81. `sample` puts the stuck sweep in the DAW project (`src/osprey/examples/daw`, added after v0.31.1 by #427): `osprey src/osprey/examples/daw --check --no-warnings` alone takes 32.7 s and 1.46 GB, and the redundant-annotation pass adds its trials on top. The cache rule holds; the type checker's cost on DAW is the blocker. Fix that, then re-measure until the sweep finishes. Progress: `a25ac44f1` keeps the redundancy proof out of the sweep for closed files (idle live heap 49–66 MB once the sweep passes DAW); `8c84e0c69` and `b552ed3f9` cut the checker's per-generalization obligation scans (4,411 generalizations rescanned 59.7M obligations on DAW), about 15% of DAW's check time.
  - Measured 2026-11-24 with `sample` on `osprey src/osprey/examples/daw --check`
    (release, 8,305 main-thread samples): 68% is the effect-row solver
    (`effect_rows::solver::converge` → `advance_function` → `function_body_model`),
    while all of HM inference is the remaining ~32%, of which the environment's
    free-variable walk in `generalize` is about 6%. A level-based `generalize`
    would therefore save at most about 6%. `c4f1b397a` (field resolution
    visits only method, index and field relations) saved about 4%. The solver
    models function bodies 32,259 times for 2,976 functions (about 11 each),
    across two passes of 37 Jacobi sweeps each. Six sweeps model nearly every
    function. Four of them are whole-program re-dirties from
    `worklist::invalidate_scope`, which dirties every function whenever the
    file-scope environment changes at all (3,876 models). The rest follow
    from sweeps reading pre-sweep state in id order, so each callee change
    re-dirties its callers on the next sweep. The candidate fixes are to
    invalidate only the readers of the scope entries that changed, and to
    defer a dirty function while a callee outside its own component is still
    dirty. `recursion.rs` already proves an acyclic function settles once its
    callees do. Either fix must keep diagnostics and IR byte-identical over
    the corpus and DAW. Separately, `Analyzer::scoped_env` clones the whole
    file-scope `CallableEnv` (`HashMap<String, Value>`) for every body model
    (32,259 clones on DAW). Sharing the file-scope base behind an `Arc` with a
    per-body overlay would remove those copies.
  - Done 2026-11-24: a dirty function now waits while a callee outside its
    own component is still dirty (`recursion::Callees::waits`), so callees
    settle first. Back to back, `osprey src/osprey/examples/daw --check` user
    time fell from 36.9/37.9 s to 29.3/30.3 s, and `src/osprey/examples/daw/test
    --check` from 256 s to 77 s. `--check` output is byte-identical over all
    617 corpus and failscompilation programs, all 161 projects and the DAW.
    osprey-types passes 980/980. Then the scope values became layered
    (`effect_rows::scope_values`): a scope copy shares the frozen file scope
    and owns only the names bound since. DAW `--check` user time went from
    37.8/37.6 s to 27.8/28.7 s back to back, with the same identical-output check.
    Still open: per-entry scope invalidation.

## Open, not yet fixed

- [x] The server quit with `analysis backlog limit reached` when 256 messages were queued, and VS Code restarted it until it gave up. Now requests that don't fit get `ServerCancelled` (-32802) so the client retries, notifications wait for room, and the session never ends. Test: `a_request_flood_is_answered_and_never_ends_the_session`. In PR #411.
- [x] Idle CPU at 100% (#384). Every watched save and every tab close re-queued the whole repository sweep, and the cache keeps only 4 closed projects, so each save re-checked all 191 projects. A watched change or a close now re-checks only the projects it affects, plus newly discovered files, and an in-progress sweep is kept, not restarted. Test: `watched_changes_recheck_only_the_projects_they_affect` (8 projects; one save went from 8 analyses to 1).
