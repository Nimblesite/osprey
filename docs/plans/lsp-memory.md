# Plan — Language server memory

Goal: an idle `osprey lsp` on this repository stays bounded, and it picks up lspkit's leak repairs as soon as they are published.

Coordinated on TMC: `osprey-lsp-memory` (Osprey), `codex-memory-leak-fix` (lspkit). lspkit's side: `lspkit/docs/plans/memory-leak-fixes.md` and `memory-leak-audit.md`.

## Osprey's own leak (done, unpushed)

- [x] Reproduce against the released 0.29.0 binary. An idle server's footprint grew past 5 GB in VS Code. In a 280 s run that only initializes, live heap reached 890 MB in 3.27M allocations and was still climbing.
- [x] Root cause. The repository diagnostics sweep checks every project, and `ProjectCache.latest` and `SourceCache.disk` pinned every project's analysis and parsed files forever.
- [x] Failing test: `project_cache::tests::the_sweep_releases_closed_projects_and_keeps_open_ones`.
- [x] Fix in `ddf589022`. Keep the projects of open documents plus the 4 most recently loaded, and evict the rest. After the same run, live heap is 128 MB in 317k allocations.
- [ ] Push with the next coordinated `PUSH-<branch>` batch after `make ci`.

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
- [ ] lspkit publishes the release with all five repairs.
- [ ] Bump `lspkit`, `lspkit-server` and `lsp-profiler` together in `src/toolchain/Cargo.toml` and refresh `Cargo.lock`. Keep `sysinfo` matched to the version `lsp-profiler` pins.
- [ ] Bump `@nimblesite/lsp-profiler-ui` in `src/toolchain/vscode-extension/package.json` if lspkit publishes a matching UI release.
- [ ] Fix any API breaks in `osprey-lsp`; no shims.
- [ ] Keep Osprey's own guard: 32 unknown-method requests through the real server leave `in_flight: 0` (`Dispatcher` `Debug`).
- [ ] `cargo test -p osprey-lsp`, then `make ci`, then one coordinated push.
- [ ] Re-measure with `heap -s` after the idle sweep: live heap at or below 128 MB.
- [ ] Release Osprey, then verify the released extension binary's live heap before closing anything.

## Open, not yet fixed

- [ ] The server quits with `analysis backlog limit reached` when 256 messages are queued (`server/work.rs`). VS Code then restarts it, and stops after repeated failures. Reproduced by opening and hovering 85 files quickly. Needs a failing test, and a fix that sheds or coalesces stale work instead of exiting.
