# Plan 0012 - Developer Tools (Debugger, Profiler, Module Explorer)

Contract: [0021 - Debugger](../specs/tooling/0021-Debugger.md) · [Compiler debug artifacts](../specs/compiler/DebugArtifacts.md) · [0028 - Profiler](../specs/tooling/0028-Profiler.md) · [0018 - Memory Management](../specs/compiler/0018-MemoryManagement.md) · [Editor Integration (module explorer)](../specs/tooling/EditorIntegration.md#module-explorer-editor-module-explorer)

Depends on: lspkit shared profiler + memory contracts (`../../lspkit/docs/specs/LSP-PROFILER-SPEC.md`); lspkit #27 is already merged into lspkit `main` (commit `5590a52`) and published in v0.4.0.

## Prerequisites in place

- Native debug line tables, `--debug*` build controls, and real LLDB-DAP F5 launch ship: `compile_program_debug` in `src/crates/osprey-codegen/src/lower/entrypoints.rs`, record DWARF in `src/crates/osprey-codegen/src/builder/debug_types.rs`, editor tests in `src/vscode-extension/test/suite/debug-fixtures.ts`.
- lspkit's shared profiler UI and its memory contracts are merged and published; Osprey pins `lspkit`/`lsp-profiler` 0.4.0 (Cargo.toml) and `@nimblesite/lsp-profiler-ui` ^0.4.0 (src/vscode-extension/package.json).

## Hard constraints

- Report optimized-away/unavailable values as unavailable; never fabricate a value.
- Debug mode must not change Osprey language semantics.
- Do not expose debugger object ids, raw addresses, retain counts, root sets, or retained sizes to Osprey source code.
- Retained size is a dominator-tree result; label it approximate/unavailable when the root/edge set is incomplete.
- Osprey-specific DAP requests are prefixed (`osprey/...`) and never a new DAP dialect.
- Runtime inspection helpers are side-effect-free and safe while the program is paused; corrupt/null handles render invalid, never crash the adapter.
- Object-graph/memory profiling is off in release builds and opt-in in debug builds.

## Phase 2 - Shared editor debug glue

- [ ] Upstream the generic VS Code DAP glue and DAP test harness into the shared LspKit TypeScript package, seeded from `src/vscode-extension/client/src/{debug-config,debug-launch,debug-console,test-debug}.ts` [DEBUGGER-REUSE]. Check: the extension imports the glue/harness from the shared package and no private copy remains.

## Phase 3 - Variable metadata tails

- [ ] Migrate the textual `@llvm.dbg.value`/`@llvm.dbg.declare` compatibility intrinsics to `#dbg_value`/`#dbg_declare` debug records in `src/crates/osprey-codegen/src/builder/debug_variables.rs` once the pinned LLVM floor accepts them [DEBUGGER-DBG-DECLARE]. Check: emitted IR uses the `#dbg_*` form and the golden IR tests in `src/crates/osprey-codegen/src/ir_tests/lowering_1.rs` plus LLDB-DAP cases still inspect values.
- [ ] Extend value-location records to the remaining capture representations in `src/crates/osprey-codegen/src/builder/debug_variables.rs` / `debug_scopes.rs`. Check: each remaining capture representation carries a `DILocalVariable` + location and LLDB-DAP reads it.
- [ ] Add unavailable-value reporting tests. Check: a test pauses where a value is unavailable and asserts the adapter reports unavailable/error, not a fabricated value.

## Phase 4 - Type and value rendering tails

- [ ] Emit union, `Result` and closure type metadata in `src/crates/osprey-codegen/src/builder/debug_types.rs` (today only `DW_TAG_structure_type`/`pointer_type`/`member`). Check: IR carries tagged-variant/closure-capture composite metadata; `llvm-dwarfdump` shows it.
- [ ] Add LLDB summaries/synthetic providers or an Osprey DAP shim for language value rendering. Check: Variables pane renders union tag+payload, `Result` success/error, closure name+captures.
- [ ] Add side-effect-free runtime inspection helpers for list/map/string/fiber/channel/effect handles (`src/crates/osprey-debug` + src/compiler/runtime). Check: a paused program reads handle state without touching raw C layouts or crashing.
- [ ] Page large collections. Check: a large list/map expands in pages, not an unbounded dump.
- [ ] Add corruption/null safety tests for runtime handle display. Check: null/corrupt handles render invalid/unavailable and the session survives.

## Phase 4A - Object graph, memory backend, Windows profiler

- [ ] Define the heap/object-graph model once for both reclaiming modes `gc` and `arc` (spec 0018): `DebugObjectId`, `ObjectNode`, `ObjectEdge`, `RootNode`, `GraphSnapshot`, and JSON/DOT snapshot formats. Check: a documented serde model reusable by both the debugger and the profiler `MemoryBackend`.
- [ ] Add runtime descriptors for Osprey heap object kinds and typed outgoing edges (`src/compiler/runtime/memory_gc.c`, `memory_arc.c`). Check: a paused program enumerates typed outgoing edges without guessing C layouts.
- [ ] Add root enumeration: stack locals, module globals, fibers, channels, active effects, runtime tables, backend-specific roots. Check: roots reported per category.
- [ ] Implement `--debug-memory=object-graph` in `src/crates/osprey-cli/src/debug_options.rs` and compile/link only the required profiler metadata in debug builds [DEBUGGER-BUILD-OPTIONS]. Check: flag accepted; metadata absent unless selected; `timeline` still rejected.
- [ ] Add allocation-site attribution recorded only while a memory session is open. Check: a non-profiled run pays nothing; an open session records sites.
- [ ] Implement `MemoryBackend` in `src/crates/lsp-profiler-osprey/src` over the live-attach endpoint [PROF-ATTACH-PROTOCOL]: snapshot, diff, references; `Runtime::memory()` in `lib.rs` returns `Some(...)` [PROF-LSP-RUNTIME]. Check: `src/crates/osprey-cli/tests/live_profiler*.rs` exercise snapshot/diff/references.
- [ ] Add Osprey DAP custom requests `osprey/objectGraph`, `osprey/objectGraph/expand`, `osprey/objectGraph/retainers`, `osprey/objectGraph/roots`, `osprey/heapSnapshot`, `osprey/heapSnapshot/diff`, `osprey/allocationSites`. Check: each answers with paging/truncation metadata, never blocking on huge heaps.
- [ ] Add a VS Code watch/variables action that opens the graph visualizer for a selected heap-backed value. Check: non-heap values explain that no graph root exists.
- [ ] Implement the local-neighborhood graph view with directional expansion, hidden-edge counts, search, filters, pinning, grouping, stable layout. Check: view shows outgoing children, incoming retainers and direct roots.
- [ ] Implement shortest/key retention paths, dominator tree (Lengauer-Tarjan), retained size, and explicit approximate/unavailable labelling. Check: "why is this still alive?" answers paths; incomplete graphs are labelled.
- [ ] Add snapshot diff and replay snapshot hooks. Check: diff reports object count, shallow/retained bytes, allocation site, owning fiber and root-path changes.
- [ ] Drop `memory: false` from `PROFILER_HOST` in `src/vscode-extension/scripts/sync-profiler-contributions.mjs`, regenerate the contributions, and run lspkit's memory contracts in Osprey's extension suite [PROF-LSP-RUNTIME]. Check: memory command/menu/setting/launch link appear and the memory contracts pass.
- [ ] Wire the Watch/Variables "Inspect type retainers" action through the `MemoryBackend` callback [PROF-UI-DEBUG-LINK]. Check: action offered only with a backend callback and typed as a type query.
- [ ] Windows sampler: suspend each registered thread, read its context and walk the stack at the rate [PROF-COLLECT-SAMPLER] sets for Unix. Check: samples `(t_ns, thread, stack, state)` on Windows.
- [ ] Windows endpoint: a named pipe per process speaking [PROF-ATTACH-PROTOCOL], discoverable by listing like the Unix socket directory [PROF-ATTACH-ENDPOINT]. Check: `hello`/`cpu_start`/`cpu_stop` over the named pipe.
- [ ] Add `attach_windows.rs` beside `attach.rs` in `src/crates/lsp-profiler-osprey/src` and run `src/crates/osprey-cli/tests/live_profiler*.rs` on the Windows CI job. Check: Windows CI passes the live profiler e2e tests.
- [ ] Object-graph tests: structural sharing (shared node + both owners), closure capture retention, fiber/channel/effect roots, conservative-root approximation, custom-manager unsupported mode, large-graph paging, cancellation, deterministic JSON/DOT export. Check: each test passes; exports are deterministic after canonicalization.

## Phase 5 - Expression evaluation

- [ ] Parse and type-check watch expressions in Osprey syntax (`src/crates/osprey-syntax`). Check: an expression parses and type-checks against the frame environment.
- [ ] Resolve names against the selected frame's debug scope. Check: locals/params/captures resolve to frame values.
- [ ] Implement pure expression evaluation for `hover`/`watch`. Check: local arithmetic/string and field-access expressions evaluate.
- [ ] Gate side effects behind explicit user action (`call` mode). Check: side-effecting, FFI, IO, spawning, HTTP and effect operations are rejected by default.
- [ ] Add REPL/watch/hover tests. Check: tests pass; unavailable optimized values are rejected with a clear message.

## Phase 6 - Fibers, channels, effects

- [ ] Add stable runtime debug ids for fibers and channels. Check: ids stable for a paused process/replay trace.
- [ ] Expose runtime debug inspection APIs for fibers, channels and effects. Check: a paused runtime reports state.
- [ ] Add fiber list and logical stack display. Check: debugger lists fibers with state; suspended-fiber stacks available.
- [ ] Add breakpoints on spawn/await/send/recv/select. Check: breakpoints fire on those operations.
- [ ] Expose the effect handler stack and resume events. Check: handler frames are visible and named; step into arm and step over `resume` work; unhandled effect/invalid resume break.
- [ ] Add concurrent/effects debugger tests. Check: a breakpoint inside spawned work stops with a meaningful logical fiber id.

## Phase 7 - Replay / time travel

- [ ] Define the `.ospreytrace` format. Check: schema covers args/cwd/env/versions/inputs/scheduler/effect/FFI/allocation ids.
- [ ] Record scheduler, IO, time/random, process, network and effect events. Check: a recording emits them.
- [ ] Implement a deterministic replay runner (`osprey debug replay <trace.ospreytrace>`). Check: a fiber scheduling test replays exactly; incompatible binary/compiler versions are rejected.
- [ ] Add checkpointing for reverse-continue/reverse-step. Check: reverse operations work.
- [ ] Add replay tests for fibers/effects. Check: replay tests pass.

## Phase 8 - Optimized debugging

- [ ] Implement debug-metadata coverage metrics (functions, lines, variables, scopes, value locations). Check: the metric detects omissions across all executable expression forms.
- [ ] Add unoptimized-vs-optimized stepping comparisons. Check: comparisons run in the suite.
- [ ] Add control-flow conformance validation for debug locations (Preserve/Merge/Drop; optimized path location set ⊆ source path's). Check: validator flags any non-subset path.
- [ ] Add source-level dynamic call-tree validation for representative programs. Check: optimized-vs-unoptimized call-tree diff classifies each divergence.
- [ ] Enable `--debug-opt=limited`, then `--debug-opt=optimized`, only after the tests define the contract [DEBUGGER-BUILD-OPTIONS]. Check: flags accepted only with the validation suite green; unavailable variables reported honestly.

## Module explorer

- [ ] Add an automated keyboard test for the module explorer; the view is VS Code's native tree, whose keyboard operation is not separately exercised today [EDITOR-MODULE-EXPLORER]. Check: `src/vscode-extension/test/suite/module-explorer.test.ts` exercises keyboard navigation and passes.
