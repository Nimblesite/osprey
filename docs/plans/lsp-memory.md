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

| lspkit finding | Reaches Osprey? |
| --- | --- |
| `[MEM-AUDIT-DISPATCH]` terminal preparation errors keep ingress registrations | Possibly. Every unknown method goes through `Dispatcher::dispatch` (`server.rs` `request`). Osprey never pre-registers through `CancellationHandle::register`. lspkit to confirm whether plain `dispatch` leaks. |
| `[MEM-AUDIT-CORRELATOR]`, `[MEM-AUDIT-INBOUND]` (sidecar) | Only if `lsp-profiler`'s live session or attach uses the sidecar. lspkit to confirm. |
| `[MEM-AUDIT-CLOSED-DOCUMENTS]`, `[MEM-AUDIT-HANDLER-CYCLE]` | No. Reference server only. Osprey closes documents in the `Vfs` and its handlers capture the engine, not the dispatcher. |

## Upgrade steps (after lspkit publishes)

- [ ] lspkit publishes the release with all five repairs to crates.io and posts the version on TMC to `osprey-lsp-memory`.
- [ ] Bump `lspkit`, `lspkit-server` and `lsp-profiler` together in `src/toolchain/Cargo.toml` and refresh `Cargo.lock`. Keep `sysinfo` matched to the version `lsp-profiler` pins.
- [ ] Bump `@nimblesite/lsp-profiler-ui` in `src/toolchain/vscode-extension/package.json` if lspkit publishes a matching UI release.
- [ ] Fix any API breaks in `osprey-lsp`; no shims.
- [ ] Add an Osprey regression for each finding that reaches Osprey. Example: 32 unknown-method requests through the real server leave no dispatcher registrations.
- [ ] `cargo test -p osprey-lsp`, then `make ci`, then one coordinated push.
- [ ] Re-measure with `heap -s` after the idle sweep: live heap at or below 128 MB.
- [ ] Release Osprey, then verify the released extension binary's live heap before closing anything.

## Open, not yet fixed

- [ ] The server quits with `analysis backlog limit reached` when 256 messages are queued (`server/work.rs`). VS Code then restarts it, and stops after repeated failures. Reproduced by opening and hovering 85 files quickly. Needs a failing test, and a fix that sheds or coalesces stale work instead of exiting.
