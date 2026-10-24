# Plan 0035: Native profiler backends

The shared profiler UI from lspkit is mounted, live CPU attach works on macOS and Linux, and every view draws in Osprey's palette ([0028-Profiler](../specs/tooling/0028-Profiler.md) `[PROF-VSCODE-SHARED]`). Two things the shared UI can show have no Osprey runtime behind them yet: a live endpoint on Windows, and heap data. Until they exist, a Windows program is absent from the Processes view, and `memory: false` hides every memory surface rather than faking one ([PROF-LSP-RUNTIME]).

## Windows live attach

Windows builds carry inactive profiler stubs and open no diagnostics endpoint, so `crates/lsp-profiler-osprey/src/attach_none.rs` recognises nothing there ([PROF-ATTACH-ENDPOINT]).

- [ ] A Windows sampler: suspend each registered thread, read its context and walk the stack, at the rate `[PROF-COLLECT-SAMPLER]` sets for Unix.
- [ ] A Windows endpoint: a named pipe per process, speaking the same `[PROF-ATTACH-PROTOCOL]`, discoverable by a listing as the Unix socket directory is.
- [ ] `attach_windows.rs` beside `attach.rs`, and the live profiler e2e tests (`crates/osprey-cli/tests/live_profiler*.rs`) on the Windows CI job.

## Heap data for the shared memory views

The shared UI already has the views: snapshots, allocation sites, diffs, leak evidence and retention graphs, run by lspkit's memory contracts in SharpLsp. Osprey needs a `MemoryBackend` that answers them for both reclaiming memory modes, ARC and GC ([spec 0018](../specs/compiler/0018-MemoryManagement.md)).

- [ ] The heap model the backend serves: object kinds, typed outgoing edges and roots. It is the debugger's object graph, specified once, in [plan 0012](0012-osprey-debugger.md)'s object-graph phase.
- [ ] Allocation-site attribution, recorded only while a memory session is open, so a program that is not being profiled pays nothing.
- [ ] A `MemoryBackend` in `crates/lsp-profiler-osprey` over the endpoint: snapshot, diff and references.
- [ ] Drop `memory: false` from `PROFILER_HOST`, regenerate the contributions, and run lspkit's memory contracts in Osprey's extension suite.
- [ ] Debugger type retention: the Watch/Variables "Inspect type retainers" action, which `[PROF-UI-DEBUG-LINK]` offers only with a backend callback.
