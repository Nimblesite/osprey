# Plan 0012 - Developer Tools (Debugger, Profiler, Module Explorer)

Goal: a paused Osprey program can be inspected honestly (variables, types, heap, fibers, effects), recorded and replayed, and debugged under optimization; the same delivery finishes the module explorer and the Osprey GitHub Action, and keeps `osprey lsp` memory bounded across lspkit upgrades.

Contracts: [0021 - Debugger](../specs/tooling/0021-Debugger.md) · [Compiler debug artifacts](../specs/compiler/DebugArtifacts.md) · [0028 - Profiler](../specs/tooling/0028-Profiler.md) · [0018 - Memory Management](../specs/compiler/0018-MemoryManagement.md) · [Module explorer](../specs/tooling/EditorIntegration.md#module-explorer-editor-module-explorer) · [0033 - Osprey GitHub Action](../specs/ecosystem/0033-OspreyGitHubAction.md) · lspkit's [LSP-PROFILER-SPEC](https://github.com/Nimblesite/lspkit/blob/main/docs/specs/LSP-PROFILER-SPEC.md)

Constraints beyond the contracts: debug mode never changes Osprey language semantics; debugger object ids, raw addresses, retain counts, root sets and retained sizes never reach Osprey source; Osprey DAP requests stay `osprey/`-prefixed, never a new DAP dialect; unavailable values are reported as unavailable, never fabricated.

## Shared editor debug glue

- [ ] Upstream the VS Code DAP glue and DAP test harness into the shared lspkit TypeScript package, seeded from `src/toolchain/vscode-extension/client/src/{debug-config,debug-launch,debug-console,test-debug}.ts`: the extension imports them from the shared package and no private copy remains [DEBUGGER-REUSE].

## Debug metadata

- [ ] Migrate `@llvm.dbg.value`/`@llvm.dbg.declare` to `#dbg_value`/`#dbg_declare` records in `src/toolchain/crates/osprey-codegen/src/builder/debug_variables.rs` once the pinned LLVM floor accepts them: emitted IR uses the `#dbg_*` form, the golden IR tests in `src/toolchain/crates/osprey-codegen/src/ir_tests/lowering_1.rs` pass and LLDB-DAP still inspects values [DEBUGGER-DBG-DECLARE].
- [ ] Extend value-location records to the remaining capture representations in `debug_variables.rs`/`debug_scopes.rs`: each carries a `DILocalVariable` plus location and LLDB-DAP reads it.
- [ ] Emit union, `Result` and closure type metadata in `src/toolchain/crates/osprey-codegen/src/builder/debug_types.rs` (today only `DW_TAG_structure_type`/`pointer_type`/`member`): `llvm-dwarfdump` shows the tagged-variant and closure-capture composites.

## Value rendering and runtime inspection

- [ ] Add LLDB summaries/synthetic providers or an Osprey DAP shim for value rendering: the Variables pane renders union tag+payload, `Result` success/error and closure name+captures.
- [ ] Add side-effect-free runtime inspection helpers for list/map/string/fiber/channel/effect handles (`src/toolchain/crates/osprey-debug`, `src/toolchain/compiler/runtime`): a paused program reads handle state without touching raw C layouts.
- [ ] Page large collections: a large list or map expands in pages, never an unbounded dump.
- [ ] Add corruption/null safety tests for handle display: null and corrupt handles render invalid/unavailable and the session survives.
- [ ] Parse and type-check watch expressions in Osprey syntax (`src/toolchain/crates/osprey-syntax`) and resolve names against the selected frame's scope: an expression type-checks against the frame environment and locals, params and captures resolve to frame values.
- [ ] Evaluate pure expressions for `hover`/`watch` and gate side effects behind an explicit `call` action: arithmetic, string and field-access expressions evaluate; FFI, IO, spawning, HTTP and effects are refused by default; tests cover unavailable optimized values with a clear message.

## Object graph, memory backend and Windows profiler

- [ ] Define the heap/object-graph model once for `gc` and `arc` (spec 0018): `DebugObjectId`, `ObjectNode`, `ObjectEdge`, `RootNode`, `GraphSnapshot` and JSON/DOT snapshot formats, a documented serde model shared by the debugger and the profiler `MemoryBackend`.
- [ ] Add runtime descriptors for Osprey heap object kinds and typed outgoing edges (`src/toolchain/compiler/runtime/memory_gc.c`, `memory_arc.c`): a paused program enumerates typed edges without guessing C layouts.
- [ ] Add root enumeration for stack locals, module globals, fibers, channels, active effects, runtime tables and backend roots: roots are reported per category.
- [ ] Implement `--debug-memory=object-graph` in `src/toolchain/crates/osprey-cli/src/debug_options.rs`, linking only the required profiler metadata in debug builds: the flag is accepted, the metadata is absent unless selected and `timeline` is still rejected [DEBUGGER-BUILD-OPTIONS].
- [ ] Add allocation-site attribution recorded only while a memory session is open: a non-profiled run pays nothing and an open session records sites.
- [ ] Implement `MemoryBackend` in `src/toolchain/crates/lsp-profiler-osprey/src` over the live-attach endpoint (snapshot, diff, references), with `Runtime::memory()` in `lib.rs` returning `Some`: `src/toolchain/crates/osprey-cli/tests/live_profiler*.rs` exercise snapshot, diff and references [PROF-ATTACH-PROTOCOL] [PROF-LSP-RUNTIME].
- [ ] Add the DAP requests `osprey/objectGraph`, `osprey/objectGraph/expand`, `osprey/objectGraph/retainers`, `osprey/objectGraph/roots`, `osprey/heapSnapshot`, `osprey/heapSnapshot/diff` and `osprey/allocationSites`: each answers with paging/truncation metadata and never blocks on a huge heap.
- [ ] Add the Watch/Variables actions that open the graph visualizer and "Inspect type retainers" for a selected heap-backed value through the `MemoryBackend` callback: offered only with a backend callback and typed as a type query; non-heap values explain that no graph root exists [PROF-UI-DEBUG-LINK].
- [ ] Implement the local-neighborhood graph view with directional expansion, hidden-edge counts, search, filters, pinning, grouping and stable layout: it shows outgoing children, incoming retainers and direct roots.
- [ ] Implement shortest/key retention paths, the Lengauer-Tarjan dominator tree, retained size and explicit approximate/unavailable labelling: "why is this still alive?" answers paths and incomplete graphs are labelled.
- [ ] Add snapshot diff and replay snapshot hooks: a diff reports object count, shallow/retained bytes, allocation site, owning fiber and root-path changes.
- [ ] Drop `memory: false` from `PROFILER_HOST` in `src/toolchain/vscode-extension/scripts/sync-profiler-contributions.mjs`, regenerate the contributions and run lspkit's memory contracts in the extension suite: the memory command, menu, setting and launch link appear and the contracts pass [PROF-LSP-RUNTIME].
- [ ] Add the Windows sampler that suspends each registered thread, reads its context and walks its stack at the rate [PROF-COLLECT-SAMPLER] sets for Unix: samples `(t_ns, thread, stack, state)` on Windows.
- [ ] Add the Windows endpoint, a named pipe per process speaking [PROF-ATTACH-PROTOCOL] and discoverable like the Unix socket directory [PROF-ATTACH-ENDPOINT]: `hello`/`cpu_start`/`cpu_stop` answer over the pipe.
- [ ] Add `attach_windows.rs` beside `attach.rs` and run `src/toolchain/crates/osprey-cli/tests/live_profiler*.rs` on the Windows CI job: the live profiler e2e tests pass there.
- [ ] Add object-graph tests for structural sharing, closure capture retention, fiber/channel/effect roots, conservative-root approximation, the custom-manager unsupported mode, large-graph paging, cancellation and deterministic JSON/DOT export after canonicalization.

## Fibers, channels and effects

- [ ] Add stable runtime debug ids and inspection APIs for fibers, channels and effects: ids stay stable for a paused process or replay trace and a paused runtime reports their state.
- [ ] Add the fiber list with logical stacks and breakpoints on spawn/await/send/recv/select: the debugger lists fibers with state, suspended-fiber stacks are readable and those breakpoints fire.
- [ ] Expose the effect handler stack and resume events: handler frames are visible and named, step into an arm and step over `resume` work, and unhandled effects or invalid resumes break; a breakpoint inside spawned work stops with a meaningful fiber id.

## Replay

- [ ] Define the `.ospreytrace` format and record scheduler, IO, time/random, process, network and effect events: a recording emits them and the schema covers args/cwd/env/versions/inputs/scheduler/effect/FFI/allocation ids.
- [ ] Implement `osprey debug replay <trace.ospreytrace>`: a fiber scheduling test replays exactly and incompatible binary or compiler versions are rejected.
- [ ] Add checkpointing for reverse-continue and reverse-step, with replay tests for fibers and effects: the reverse operations work.

## Optimized debugging

- [ ] Implement debug-metadata coverage metrics (functions, lines, variables, scopes, value locations) that detect omissions across every executable expression form, and run unoptimized-vs-optimized stepping comparisons in the suite.
- [ ] Add control-flow conformance validation for debug locations (Preserve/Merge/Drop; each optimized path's location set ⊆ its source path's): the validator flags any non-subset path.
- [ ] Add source-level dynamic call-tree validation for representative programs: the optimized-vs-unoptimized call-tree diff classifies each divergence.
- [ ] Enable `--debug-opt=limited`, then `--debug-opt=optimized`, in `src/toolchain/crates/osprey-cli/src/debug_options.rs` only once that suite is green: unavailable variables are reported honestly [DEBUGGER-BUILD-OPTIONS].

## GitHub Action

- [ ] Owner decision before packaging: what the published action computes (the spec fixes only its shape) and the GHCR org plus Marketplace listing [GHA-WRAPPER].
- [ ] Create `main.osp` at the repository root beside `action.yml`: one `input()` call, parse the JSON line with the JSON builtins, print `key=value` lines as one interpolated string and return a non-zero int on failure; `osprey main.osp --run < payload.json` reproduces the stdout and exit code of `a_json_line_on_stdin_becomes_outputs_and_an_exit_status` (`src/toolchain/crates/osprey-cli/tests/cases/program_contracts.rs`) [GHA-FERRY-STDIN] [BUILTIN-JSON] [ENTRY-EXIT-STATUS].
- [ ] Add a root `Dockerfile` (clang, the `osprey` binary, the `libfiber_runtime*.a` archives, `main.osp` at a fixed path, optionally pre-compiled) whose entrypoint passes the stdin line through unchanged and routes stdout to `$GITHUB_OUTPUT`; root `action.yml` uses `runs.using: docker`, an image pinned by GHCR `sha256` digest and typed `inputs`/`outputs` [GHA-PACKAGING-DOCKER].
- [ ] Add the GHCR build/publish job to `.github/workflows/release.yml`, listed in `release-complete.needs` with its credential checked in `preflight`: `node scripts/ci/verify-release-gates.mjs` passes and a consuming workflow reproduces the ferry output for the same inputs [GHA-PACKAGING-DOCKER].
- [ ] Add the composite `action.yml` for `ubuntu`, `macos` and `windows` runners (install `osprey`, `clang` and `jq`, cache the toolchain, run the same `main.osp`): all three reproduce the Docker image's output [GHA-PACKAGING-COMPOSITE].
- [ ] Add the root `action.yml` marketplace metadata and README usage section, version stamped from the tag, and a publication step on a `vX.Y.Z` tag in `release.yml` [GHA-MARKETPLACE] [SWR-VERSION-BUILD-STAMPING].
- [ ] Package a precompiled `main.wasm` under wasmtime or a Node WASI host: the round trip is green with only a WASI host on the runner [GHA-WASM-DEFERRED].

## LSP memory and lspkit upgrades

Osprey pins `lspkit`, `lspkit-server` and `lsp-profiler` at 0.10.1 and `@nimblesite/lsp-profiler-ui` at ^0.10.1, on Rust 1.100.0 [TOOLCHAIN-RUST-PIN]. Keep these boundaries when upgrading: no sidecar correlator (Osprey reads with `read_message` on its own loop); no `CancellationHandle::register` without proving cancellation entries are reclaimed (`Dispatcher::dispatch` registers after finding a handler); handlers capture the engine, never the dispatcher [LSPKIT-UPGRADE-SOURCE]. `SourceCache::kept` retains each open document's project plus only the most recently loaded closed projects, and `an_open_shared_file_does_not_pin_its_dependents` and `the_sweep_releases_closed_projects_and_keeps_open_ones` in `src/toolchain/crates/osprey-lsp/src/project_cache.rs` stay the retention regressions [LSPKIT-UPGRADE-OSPREY-LEAK].

- [ ] (verify) An idle `osprey lsp` stays below the 128 MB live-heap limit after a diagnostics sweep of this repository (last measured 16.5 MiB live with v0.31.1): re-measure with `heap -s` and record the run [LSPKIT-UPGRADE-OSPREY-LEAK].
