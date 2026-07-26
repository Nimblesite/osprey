# Plan 0012 - Modern Osprey Debugger

## Summary

The debugger scope includes source breakpoints, stepping, stack
inspection, variables, Osprey value rendering, expression evaluation, fibers,
effects, replay, and machine-checkable debug-info quality. It should integrate
with existing debugger infrastructure first, then add Osprey-specific semantics
where generic native debuggers cannot know the language.

The architecture is layered:

1. Emit standards-based source debug information from the compiler.
2. Compile debug builds as native executables that LLDB/GDB can understand.
3. Use DAP for editor integration and headless automation.
4. Add an Osprey debug shim for language-specific values, fibers, effects, and
   expression evaluation.
5. Add deterministic replay/time-travel after the runtime can record the needed
   events.

The initial shipped slices cover line tables, `--debug`, and VS Code launch
through LLDB-DAP.

The watch window is also the memory-profiler entry point. A user must be able
to select any heap-backed variable or watch expression and open an object graph
that shows outgoing children, incoming retainers, paths to roots, retained
size, allocation site, owning fiber, and snapshot diffs. It must expose both
outgoing references and the roots or retainers keeping an object alive.

## Design constraints

- Use DWARF as the native interchange format on Unix-like platforms. It is the
  standard consumer contract for LLDB/GDB and already models source languages,
  scopes, functions, variables, line tables, and locations. Emit the version
  the target toolchain best supports: DWARF 4 on macOS (Apple `dsymutil`/LLDB
  lag on v5), DWARF 5 elsewhere.
- Emit debug metadata through LLVM IR metadata, not a sidecar map that only our
  extension understands. Osprey's backend already emits textual LLVM IR, so the
  metadata boundary is `!DICompileUnit`, `!DISubprogram`, `!DILocation`,
  `!DILocalVariable`, and value-location records. The long-term target is
  LLVM's `#dbg_value` / `#dbg_declare` **debug records**; the current textual IR
  backend may emit the older `@llvm.dbg.*` intrinsic compatibility form only
  while the pinned local LLVM toolchain accepts and lowers it correctly.
- Use DAP for IDEs, but do not define a new DAP dialect unless required.
  Reuse `lldb-dap` first; add Osprey-only requests only behind the standard
  adapter boundary.
- Debug builds start at `-O0`. Optimized modes require a per-variable history
  oracle: every local's observed value-change history must match the source
  program at the correct file and line under DCE, CSE, and code motion. Report
  unavailable values rather than fabricating them.
- When Osprey gains its own IR transforms, debug-location updates must obey the
  **control-flow conformance** invariant: an optimized path's location set must
  be a _subset_ of the corresponding source path's. Per
  moved/cloned/merged instruction choose Preserve / Merge / Drop accordingly;
  never attach a source location to a path that did not already carry it, and
  prefer Drop over a guessed location. Osprey currently optimizes at the clang
  level; this constraint applies when Osprey adds IR passes.
- Validate stack/scope correctness against the **source-level dynamic call
  tree**, comparing an optimized run to its unoptimized reference. Classify
  each divergence as a compiler bug or a debug-info format limitation, use DWARF
  `inlined_subroutine` records and location views, and mark synthesized frames
  artificial so debuggers can hide them.
- Use **cross-debugger differential testing** over the same binary: drive two
  independent adapters, instruction-step, and diff full program state including
  registers, treating optimized-out, error, and not-found as distinct states.
- Treat the object graph as a rooted directed graph of Osprey heap values and
  runtime roots. Outgoing edges represent references; incoming reverse edges
  and root paths identify retainers. The UI must show both because a variables
  tree alone hides aliasing, sharing,
  captured closures, retained channels, and effect resumptions.
- Compute retained size from a dominator tree when the snapshot has a complete
  enough root set and edge set. Use the Lengauer-Tarjan dominator algorithm;
  when conservative roots or custom managers make the graph incomplete, label
  retained size as approximate or unavailable.
- The first object graph view is the selected variable's focused neighborhood,
  with lazy inbound/outbound expansion. Whole-heap views
  start as dominator trees, allocation-site/type/fiber aggregates, treemaps, or
  timelines. Apply focus+context navigation, stable incremental layout, hidden
  edge counts, filtering, search, pinning, and edge bundling before drawing
  dense cross-links.
- Keep DAP standard where it fits and prefix Osprey extensions where it does
  not. `variables`/`variableReference` remain the value tree. Object graph
  payloads need custom Osprey requests because they are graph/snapshot data,
  not a linear child list.

## User-facing requirements

The finished debugger must support these workflows:

- Press F5 in VS Code on an `.osp` file and hit source breakpoints.
- Run `osprey file.osp --debug --compile` to produce a native debug binary.
- Run `osprey file.osp --debug --run` and preserve debug info for crash/stack
  analysis.
- Launch an Osprey program with args, cwd, env, stdin/stdout/stderr terminal
  configuration, and optional stop-on-entry.
- Attach to an existing Osprey process when symbols are present.
- Load a core dump when the platform/debugger supports it.
- Use breakpoints: source line, conditional, hit count, logpoint, function,
  exception/panic-like runtime trap, and data watchpoint where supported.
- Step over, step into, step out, continue, pause, restart, and stop.
- Show call stacks in Osprey names, not only lowered helper symbols.
- Show locals, parameters, captured variables, module globals, and runtime
  handles.
- Render Osprey values: ints, floats, bools, strings, Unit, records, unions,
  Result, lists, maps, fibers, channels, effects, closures, and Ptr/FFI values.
- From any heap-backed variable or watch expression, open an object graph that
  shows outgoing references, incoming retainers, typed edges, source allocation
  sites, owning fibers, and runtime roots.
- Ask "why is this still alive?" and get shortest/key retention paths from
  stack/global/fiber/channel/effect/runtime roots to the selected object.
- Inspect dominator/retained-size views for snapshots, labelled `approximate`
  when the memory backend cannot provide complete edge/root data.
- Capture and compare memory snapshots during breakpoints or replay to see
  object-count, byte-size, retention-path, and allocation-site changes.
- Evaluate Osprey expressions in the current frame, with side-effect controls.
- Inspect fibers as logical tasks even when the OS sees one thread.
- Inspect active effect handlers and resumptions.
- Record/replay deterministic executions for concurrency/effects bugs.
- Work headlessly in CI so debug behavior is tested without a GUI.

## Non-goals

- Do not build a native-code debugger from scratch.
- Do not invent a proprietary source map format as the primary contract.
- Do not make VS Code the only frontend.
- Do not promise reliable optimized variable inspection before explicit
  optimized-debug validation exists.
- Do not let debug mode change Osprey language semantics.
- Do not expose debugger object ids, raw addresses, retain counts, root sets, or
  retained sizes to Osprey source code.

## Architecture

### Layer 1: compiler source model

The AST must carry complete source spans, not just optional declaration
positions.

Required model:

- `SourceId`: canonical source file identity.
- `SourceSpan`: file, start line/column, end line/column, byte offsets.
- Every `Stmt`, every `Expr`, every pattern, every type expression, every
  parameter, every field, every handler arm, and every generated wrapper must
  either have a real source span or be marked synthetic.
- Lowering rewrites such as pipe desugaring, method-call normalization,
  lambda lifting, generic specialization, effect handler lowering, and closure
  construction must preserve provenance.
- Synthetic compiler/runtime code must be marked as artificial and skipped by
  source stepping unless the user disables smart stepping.

Acceptance:

- Parser tests assert spans for representative declarations, statements,
  nested block expressions, lambdas, match arms, handler arms, and interpolation
  holes.
- LSP and debugger share the same line/column convention: Osprey AST uses
  1-based lines and 0-based columns; DAP uses 1-based lines and columns.
  Emitted `!DILocation` columns are 1-based, so convert the 0-based AST column
  with `column + 1` — LLVM reserves column `0` as the "no column" sentinel, and
  emitting a raw 0-based column silently collides with it.

### Layer 2: LLVM/DWARF debug info

The backend must emit LLVM debug metadata for native debuggers.

Required module metadata:

- `source_filename` with the canonical source path.
- `!llvm.dbg.cu` with one `DICompileUnit`.
- `!llvm.module.flags` with `"Debug Info Version"` and DWARF version.
- `DIFile` for every source file involved.
- Osprey producer string and language tag. `DW_LANG_C` is the pragmatic interim
  code, but it is **not neutral**: LLDB/GDB drive expression-evaluation language,
  value/type formatting, name demangling, and default array lower bounds off
  `DW_AT_language`, so claiming C imports C semantics that the Osprey-aware
  evaluator (Layer 6) must then override. `DW_LANG_lo_user`..`hi_user` is honest
  but unrecognized by every debugger (and ≥ `0x8000` forces `DW_FORM_data2`).
  The target is to register `DW_LNAME_Osprey` with dwarfstd.org and emit the
  DWARF 6 `DW_AT_language_name` + `DW_AT_language_version` pair
  (permitted from a DWARF 5 producer) while dual-emitting legacy `DW_AT_language`
  for older consumers.

Required function metadata:

- `DISubprogram` for user functions and `main`.
- Distinct linkage names for lowered symbols, but display names should be
  Osprey names.
- `DISubroutineType` for function signatures.
- Lexical scopes for blocks, lambdas, handler arms, match arms, and generated
  closures.
- Artificial/generated functions marked artificial where LLVM/DWARF supports
  it, so stepping favors source code.

Required instruction metadata:

- Every executable instruction derived from user code must carry `!dbg`.
- Statement-frontier lines receive stable locations for breakpoints.
- Pure bookkeeping instructions may inherit the nearest source location only
  when needed to keep breakpoints/stepping coherent; otherwise they are
  artificial/no-location.
- Tail code generated for `return`, block completion, and implicit `Unit`
  should map to the last user expression, not to random generated lines.

Required variable metadata:

- Emit variable locations as LLVM value-location records. Prefer
  `#dbg_declare` / `#dbg_value` **debug records** on LLVM 19+ / RemoveDIs
  toolchains. The current Osprey textual backend uses the `@llvm.dbg.declare`
  compatibility intrinsic over a dedicated stack slot per local: it keeps the
  line table free of the stray line-0 rows an inline `@llvm.dbg.value` lowers
  to (which derail x86_64 lldb-dap breakpoint line resolution). This is a
  bridge, not the desired final IR spelling. [DEBUGGER-DBG-DECLARE]
- Function parameters: `DILocalVariable` plus a `dbg.declare` over a slot
  holding the incoming argument.
- Immutable locals: value-location records for SSA values; `dbg.declare`/
  `#dbg_declare` for stack/heap slots when addressable.
- Mutable locals: a user variable and its storage cell must be represented so
  the user sees the value, not only the cell pointer.
- Captures: closure environment fields mapped back to capture names.
- Match bindings and handler parameters: lexical-block scoped locals.
- Optimized-away values: reported honestly as unavailable; never fabricated.

Required type metadata:

- Primitive Osprey types mapped to base types.
- Strings as Osprey strings, not raw `i8*` by default.
- Records/objects as composite types with named fields.
- Unions as tagged variants.
- Result as tagged success/error shape with value/message rendering.
- Lists/maps/channels/fibers as runtime handle types with pretty-printer
  expansion, because their concrete C layout is runtime-owned.
- Function values/closures as callable handles with captures.
- Ptr/FFI types as opaque pointers unless an extern binding supplies richer
  metadata later.

Acceptance:

- `osprey --llvm --debug file.osp` contains `DICompileUnit`,
  `DISubprogram`, and `DILocation`.
- Compiled binaries contain a source line table visible to `llvm-dwarfdump` or
  platform equivalent.
- LLDB can set a breakpoint by `.osp` file and line, then stop there.
- Stepping through a simple program visits Osprey source lines in expected
  order.

### Layer 3: compiler CLI and build modes

New CLI surface:

- `--debug`: emit debug info and build for debugging.
- `--debug-info=dwarf|none`: explicit debug-info choice, default `dwarf` when
  `--debug` is present.
- `--debug-opt=none|limited|optimized`: optimization/debuggability policy.
  Initial implementation supports `none` only.
- `--debug-memory=off|object-graph|timeline`: memory-profiler metadata policy.
  Initial implementation may support `off` and on-demand `object-graph` only.
- `--debug-out <path>`: optional path for the debug executable.
- `--debug-preserve-ir`: keep the `.ll` file for inspection.
- `--debug-preserve-symbols`: keep platform symbol bundles such as `.dSYM`.

Debug build behavior:

- `--debug` implies `-O0` unless `--debug-opt` requests otherwise.
- Pass `-g` to the C compiler driver.
- Prefer `-fno-omit-frame-pointer` where supported.
- On macOS, run or document `dsymutil` behavior so line tables and symbols are
  discoverable by LLDB.
- Do not strip debug symbols.
- Runtime archives should have debug variants once runtime stepping is needed.
  First slice may debug user code only.

Acceptance:

- `osprey file.osp --debug --compile` writes a binary and leaves enough debug
  info for LLDB to resolve Osprey lines.
- `osprey file.osp --debug --run` executes normally.
- Existing non-debug `--compile`/`--run` behavior and performance defaults are
  unchanged.

### Layer 4: DAP and editor integration

Initial adapter strategy:

- Reuse LLDB-DAP as the real adapter.
- The Osprey VS Code extension resolves the Osprey compiler, compiles the
  active `.osp` file with `--debug --compile`, then launches `lldb-dap` against
  the produced binary.
- The extension contributes a real `osprey` debugger configuration, not a
  provider that cancels the session and runs the file.

Debug configuration fields:

- `program`: `.osp` source path.
- `args`: program args.
- `cwd`: working directory.
- `env`: environment map.
- `stopOnEntry`: bool.
- `compilerPath`: override compiler path.
- `lldbDapPath`: override LLDB-DAP path.
- `debugOutput`: optional binary path.
- `preserveArtifacts`: bool.
- `console`: integrated terminal, external terminal, or internal console.
- `preLaunchTask` support through VS Code's native mechanisms.

Resolution:

- Find `lldb-dap` from explicit config, extension setting, common LLVM toolchain
  paths, or PATH.
- If missing, show a precise error explaining the required tool and the setting
  to configure.
- On systems where only `lldb-vscode` exists, detect it as a compatibility
  fallback if it supports DAP.

Future Osprey-aware adapter:

- A small `osprey-dap` shim may sit in front of LLDB-DAP when native DAP cannot
  express Osprey semantics.
- It should proxy standard requests and only intercept Osprey-specific
  variables, evaluate, fibers, effects, and replay requests.
- Any non-standard DAP extension must be prefixed and documented.

Shared editor components (no per-language duplication):

- Osprey, Basilisk, and SharpLsp each ship a VS Code debugger. The editor-side
  glue is language-neutral and MUST be shared via a package under the
  [LspKit](https://github.com/Nimblesite/lspkit) umbrella, not forked into each
  extension. See spec `[DEBUGGER-REUSE]`.
- LspKit is currently Rust-only and carries no debugger code, so this needs a
  new shared TypeScript package (VS Code DAP glue + DAP test harness) alongside
  a future `lspkit-debug` Rust crate for native debug-build policy.
- Shared TypeScript surface: adapter resolution (setting → toolchain paths →
  PATH + precise missing-tool error; cf. SharpLsp `findNetcoredbg`), debug-config
  synthesis (cf. Basilisk `applyDebugConfigDefaults`, SharpLsp
  `resolveDebugConfiguration`), save-dirty-or-reject, the pre-launch build hook,
  and the DAP test client/poll/UI-stub harness.
- Language-specific only: debug `type`, adapter binary (`lldb-dap`), build
  command, toolchain paths. The Osprey extension contributes its generic glue
  upstream rather than maintaining a private copy.

Acceptance:

- Pressing F5 on a focused `.osp` file starts a debug session.
- The editor DAP glue and DAP test harness are imported from the shared LspKit
  package, not duplicated in the Osprey/Basilisk/SharpLsp extensions.
- The VS Code debug UI can set/hit a source breakpoint.
- The debug session does not use `osprey.run`; it launches a debugger.
- A headless DAP integration test can initialize, launch, set breakpoints,
  continue, and observe a stopped event.

### Layer 5: Osprey value model and pretty-printers

Generic LLDB display will expose raw pointers and structs. A modern Osprey
debugger must render language values.

Required value renderers:

- `int`, `float`, `bool`, `string`, `Unit`.
- Records and anonymous objects with field names and nested values.
- Union variants with tag and payload fields.
- `Result<T, E>` as `Success(value)` or `Error(message/value)`.
- Lists with length and paged children.
- Maps with length and paged key/value entries.
- Strings with UTF-8 validation and byte/codepoint display policy matching the
  language spec.
- Closures with function name and captures.
- Fibers with state, result type, and join/await status.
- Channels with open/closed state and buffer/queue info if available.
- Effect handlers with active operation, captured env, and continuation state.
- FFI pointers with address and optional extern binding name.

Implementation options:

- LLDB Python synthetic providers and summaries for native LLDB.
- DAP variableReference expansion through an Osprey shim.
- Runtime helper functions for safe inspection of opaque handles. Helpers must
  be side-effect-free and safe when the program is paused.

Acceptance:

- Variables pane shows Osprey names and values for primitive locals.
- Records/unions expand by field/variant name.
- Lists/maps are paged and do not dump unbounded memory.
- Runtime handles never crash the debug session when null/corrupt; they render
  as invalid/unavailable.

### Layer 5A: object graph watch window and memory profiler

The object graph is part of the debugger, not a separate heap-dump tool. It
starts from the same value references used by the variables/watch UI and then
adds graph, root, retention, ownership, and snapshot analysis.

Debugger data model:

- `DebugObjectId`: stable for the current paused process or replay trace. It is
  not a source-level identity and does not have to be a raw pointer.
- `ObjectNode`: id, Osprey type, runtime kind, short value summary, shallow
  bytes, retained bytes or unavailable/approximate, source allocation site,
  owning fiber id, allocation generation/time when recorded, backend provenance,
  and validity state.
- `ObjectEdge`: source id, target id, edge kind, display label, source slot
  (`field name`, list index, map bucket/key/value, capture name, queue entry),
  direction, and strength/precision (`precise`, `conservative`, `synthetic`).
- `RootNode`: stack local/parameter, module global, selected watch expression,
  active fiber, suspended fiber, channel, effect handler/resumption, runtime
  singleton, FFI handle, or conservative native root.
- `GraphSnapshot`: snapshot id, process/debug-session id, stop reason, selected
  frame/fiber, root set, nodes, edges, aggregation nodes, profiler warnings,
  and backend capability flags.

Required runtime/debug APIs:

- Resolve a DAP `variableReference` or Osprey watch expression to a
  `DebugObjectId` when the value is heap-backed.
- Enumerate outgoing Osprey heap edges for a node without guessing C layouts in
  TypeScript.
- Enumerate incoming edges through a runtime-maintained reverse index or a
  bounded graph scan; large scans must be cancellable.
- Enumerate roots from the selected frame, all stacks/fibers, module globals,
  channels, active effects, runtime tables, and backend-specific roots.
- Capture snapshots in stop-the-world debug mode so graph topology is
  internally consistent for the selected stop event.
- Export JSON and DOT snapshots for tests and issue attachments.

DAP/editor boundary:

- Standard DAP `variables`, `scopes`, and `evaluate` continue to power the
  exact tree drill-down.
- Add prefixed custom requests only for graph-shaped data:
  `osprey/objectGraph`, `osprey/objectGraph/expand`,
  `osprey/objectGraph/retainers`, `osprey/objectGraph/roots`,
  `osprey/heapSnapshot`, `osprey/heapSnapshot/diff`, and
  `osprey/allocationSites`.
- Every request accepts paging, max-node, max-edge, max-depth, root-kind, edge
  kind, type, fiber, allocation-site, and timeout filters.
- The adapter must return partial results with explicit truncation/cancellation
  metadata instead of blocking the IDE on huge heaps.

Retention analysis:

- Reachability starts at runtime roots, then follows precise Osprey heap edges.
- Incoming retainers are computed from the reverse graph.
- Shortest paths to roots answer the first retention question quickly.
- Key retention paths must de-duplicate equivalent paths so one noisy stack root
  does not hide independent owners.
- Dominator tree and retained size are computed for complete snapshots. For
  snapshot roots, a synthetic super-root dominates all real roots.
- Conservative GC roots, FFI handles, and custom managers can make retention
  incomplete. The UI must mark affected paths/sizes approximate or unavailable.
- Snapshot diff compares object count, shallow bytes, retained bytes, type,
  allocation site, owning fiber, and root-path changes.

Visual UX:

- The variables/watch row gets a "show object graph" action for heap-backed
  values. Non-heap values explain that there is no graph root.
- The first graph is a local neighborhood: selected node, outgoing children,
  incoming retainers, direct roots, and hidden-edge counts.
- Expansion is directional: expand children, expand retainers, expand to root,
  expand allocation-site group, expand fiber group.
- Whole-heap mode starts from aggregated dominators, allocation sites, type
  groups, or fiber groups; it does not render every object by default.
- The visualizer supports pinned nodes, search, filters, type/source/fiber
  grouping, snapshot timeline, snapshot diff overlay, and JSON/DOT export.
- Layout is stable across refreshes and expansions. Text must not overlap.
  Dense cross-links use bundling, elision, or aggregate nodes rather than a
  useless hairball.

Performance and safety:

- Object graph profiling is off in release builds and opt-in in debug builds.
- Runtime metadata is compiled only under `--debug-memory=object-graph` or
  stronger modes unless a minimal id/descriptor table is cheap enough to keep in
  all debug builds.
- Snapshot capture must have a bounded memory budget and cancellation path.
- Large collections and maps are paged. Huge object groups collapse by default.
- Runtime helper functions are side-effect-free and safe while the program is
  paused. Corrupt/null handles render as invalid, not as adapter crashes.
- Native addresses and raw bytes are hidden unless the user enables expert
  native view.

Acceptance:

- Selecting a record/list/map/closure/fiber/channel/effect value can open a
  graph rooted at that value.
- The graph shows both outgoing children and incoming retainers with typed
  edges.
- A structurally shared list/map test shows the shared node and both owners.
- A closure-capture test shows the captured value and the closure retaining it.
- A fiber/channel/effect test shows runtime roots and owning logical fiber ids.
- A leak-shaped example can show a path from a root to the retained object.
- Retained size/dominator view works for a complete snapshot and marks
  conservative/custom-manager limitations honestly.
- Snapshot diff detects growth by allocation site and changed root paths.
- JSON/DOT export is deterministic enough for golden tests.

### Layer 6: expression evaluation

Expression evaluation must be Osprey-aware. LLDB C expression evaluation is not
enough for Osprey syntax or type rules.

Required modes:

- `hover`: display variable value for symbols in scope.
- `watch`: evaluate pure Osprey expressions.
- `repl`: evaluate expressions and optional debugger commands.
- `call`: explicitly allow side-effecting calls only when the user requests it.

Implementation:

- Parse the expression with `osprey-syntax`.
- Resolve names from the selected frame's debug scope.
- Type-check against the frame environment.
- Lower either to an LLDB expression using known native symbols or to a small
  JIT/evaluation thunk when safe.
- Block or prompt for side-effecting operations, FFI, IO, spawning, HTTP, and
  effect operations unless explicitly enabled.

Acceptance:

- Evaluate local arithmetic/string expressions.
- Evaluate field access and match-safe value inspection.
- Reject unavailable optimized values with a clear message.
- Reject side effects by default.

### Layer 7: stack traces and source navigation

Stack traces must hide implementation noise by default but preserve access for
advanced debugging.

Required behavior:

- Frames display Osprey function/module/lambda names.
- Closure/lambda frames show declaration line and capture summary.
- Handler/resume frames show effect and operation.
- Runtime frames are hidden under smart stepping by default.
- Users can disable smart stepping to step into generated/runtime code.
- Stack traces from crashes include file/line/function.

Acceptance:

- `print(backtrace)` equivalent runtime support or LLDB backtrace shows Osprey
  file/line for user code.
- Stepping into `print` or runtime helpers is skipped by default.
- A crash in user code reports the Osprey call stack.

### Layer 8: fibers, channels, and effects

Osprey's runtime has logical concurrency that native OS-thread debuggers do not
fully model.

Required fiber support:

- Each fiber has a stable debug id.
- Debugger can list fibers with state: runnable, running, waiting, completed,
  failed.
- Stack for suspended fibers is available when runtime representation allows it.
- Breakpoints can stop all fibers or only the triggering fiber.
- Scheduler stepping can advance one fiber or run until next breakpoint.

Required channel/select support:

- Inspect channel state.
- Show waiting senders/receivers if runtime tracks them.
- Break on send/receive/select operations.

Required effect support:

- Show active handler stack.
- Show performed effect operation and arguments.
- Step into handler arm.
- Step over `resume`.
- Break on unhandled effect or invalid resume.

Acceptance:

- Debugger lists fibers in a concurrent test program.
- Breakpoint inside spawned work stops with a meaningful logical fiber id.
- Effect handler frames are visible and named.

### Layer 9: deterministic replay and time travel

Replay is not the first debugger, but a modern debugger plan must include it
because concurrency/effects bugs are otherwise hard to reproduce.

Required record stream:

- Program args, cwd, env, Osprey version, compiler version, target triple.
- External inputs: stdin, files when sandbox/record mode permits, network when
  enabled, clock/random/process results.
- Scheduler decisions for fibers/channels/select.
- Effect operation/resume ordering.
- FFI boundary calls when recordable; opaque calls marked non-replayable.
- Runtime allocation ids for stable handle identity where needed.

Required replay behavior:

- `osprey file.osp --debug --record trace.ospreytrace --run`.
- `osprey debug replay trace.ospreytrace`.
- Reverse-continue and reverse-step once snapshots/checkpoints exist.
- Deterministic failure if the run depends on non-recorded external state.

Acceptance:

- A fiber scheduling test replays exactly.
- A trace records metadata sufficient to reject replay on incompatible binary or
  compiler version.

### Layer 10: optimized debugging

Optimized debugging must be explicit and tested. It cannot be hand-waved.

Policy:

- `--debug-opt=none` is the default and the first supported mode.
- `--debug-opt=limited` may enable optimizations proven not to break line/var
  expectations for covered cases.
- `--debug-opt=optimized` is allowed only after validation infrastructure is in
  place; it must be honest about unavailable variables.

Required validation:

- Per-variable history oracle (Kell & Stinnett): each local's observed
  value-change history must match the source program's, with each change at the
  correct file and line. This factors out temporal imprecision and flags only
  outright wrong or missing variable info; it is the automatable correctness
  test for emitted local/line metadata.
- Control-flow conformance for location updates (Huang et al.): for every
  instruction a pass moves/clones/merges, assert the optimized path's location
  set is a subset of the source path's, and that the Preserve/Merge/Drop choice
  follows from that containment. Applies once Osprey runs its own IR passes.
- Source-level dynamic call-tree differential (Stinnett & Kell): compare the
  call tree recovered from an optimized run against its unoptimized reference;
  attribute divergences to compiler vs debug-info format. Do not settle for the
  weak backtrace invariant (reversed/empty backtraces satisfy it).
- Cross-debugger differential testing (DTD, Lu et al.): diff full program state
  between two adapters over the same binary, treating optimized-out / error /
  not-found as distinct states.
- Track coverage of debug metadata: functions, lines, variables, scopes, and
  value locations.

Acceptance:

- Optimized debug mode has its own test suite and documented limitations.
- Any optimized-away variable is reported as unavailable, not stale.

### Layer 11: security and safety

Debugger integration runs programs and can inspect memory; it needs explicit
boundaries.

Requirements:

- Never launch a program without user action in the IDE.
- Preserve workspace trust expectations.
- Quote paths safely and avoid shell execution where `execFile`/spawn APIs are
  available.
- Do not expose remote attach by default.
- Redact sensitive environment values from logs.
- Keep debug adapter logs opt-in.
- Side-effecting expression evaluation is off by default.
- FFI and network replay require explicit user opt-in.
- Debug artifacts should not be silently published in release packages unless
  intended.

Acceptance:

- All compiler/debugger subprocess launches use argument arrays, not shell
  interpolation.
- Missing tool errors do not leak env secrets.

### Layer 12: packaging and cross-platform

Platforms:

- macOS: LLDB/LLDB-DAP first, DWARF/dSYM support.
- Linux: LLDB-DAP first, GDB/DAP fallback only if needed.
- Windows: MinGW DWARF via LLDB/GDB first; native MSVC CodeView/PDB is a later
  track if the toolchain moves that way.

Packaging:

- The VS Code extension should not bundle a large LLDB toolchain initially.
- It should detect and explain missing LLDB-DAP.
- Release notes must state debugger prerequisites.
- Future binary releases may bundle platform-specific `lldb-dap` if licensing,
  size, and update cadence are acceptable.

Acceptance:

- Debug prerequisites documented for macOS/Linux/Windows.
- Extension fails cleanly when prerequisites are missing.

## Phased implementation

### Phase 0 - Spec and safety rails

- [x] Write this plan.
- [x] Add `docs/specs/0021-Debugger.md` once phase 1 behavior is real enough to
      become language/tooling contract.
- [ ] Add issue checklist and labels for debugger phases.

### Phase 1 - Source line debugging

- [x] Add complete statement source positions, including bare expression
      statements.
- [x] Add `osprey_codegen::compile_program_debug`.
- [x] Emit `DICompileUnit`, `DIFile`, `DISubprogram`, `DILocation`, and module
      flags.
- [x] Add `--debug` and debug clang flags.
- [x] Preserve debug artifacts for tests.
- [x] Add golden tests for debug IR metadata, including `DIFile`, user-function
      `DISubprogram`, per-platform DWARF version, and 1-based `DILocation`
      column assertions.
- [ ] Add a repo-run headless LLDB smoke test for source breakpoint and
      stepping. A manual LLDB smoke has passed; it is not yet an automated repo
      test.

### Phase 2 - VS Code real debug launch

- [x] Replace the current fake debug provider that cancels the session and runs
      the file.
- [x] Compile the selected `.osp` file with `--debug --compile`.
- [x] Resolve `lldb-dap` through config, setting, PATH, `xcrun`, and common LLVM
      paths with a precise missing-tool error.
- [x] Launch LLDB-DAP with the compiled binary.
- [x] Add extension tests for configuration synthesis and missing-tool errors.
- [x] Add a DAP smoke test that launches, hits a source breakpoint, reads stack
      and primitive locals, steps over, continues, and terminates.
- [ ] Upstream/import generic VS Code debugger glue and the DAP test harness
      from a shared LspKit TypeScript package. Local `lsp_toolkit` currently has
      Rust crates only and no debugger package, so Osprey keeps the pure helpers
      as the seed for extraction instead of duplicating them elsewhere.

### Phase 3 - Variables and lexical scopes

- [x] Emit `DILocalVariable` for primitive function parameters and `let`
      bindings.
- [x] Emit compatibility value-location intrinsics for primitive params/lets and
      verify the values through LLDB-DAP.
- [ ] Migrate the textual spelling from `@llvm.dbg.*` compatibility intrinsics
      to `#dbg_*` debug records once the supported LLVM floor makes that path
      portable.
- [ ] Extend value-location records to mut cells, captures, match bindings, and
      handler parameters.
- [ ] Add lexical scopes for blocks, lambdas, match arms, and handlers.
- [x] Validate primitive local inspection in LLDB-DAP.
- [ ] Add unavailable-value reporting tests.

### Phase 4 - Osprey type and value rendering

- [ ] Emit type metadata for primitives, records, unions, Result, and closures.
- [ ] Add LLDB summaries/synthetic providers or an Osprey DAP shim.
- [ ] Add runtime inspection helpers for list/map/string/fiber/channel/effect
      handles.
- [ ] Page large collections.
- [ ] Add corruption/null safety tests for runtime handle display.

### Phase 4A - Object graph watch window and memory profiler

- [ ] Define debug object ids, node metadata, edge kinds, root categories, and
      snapshot JSON/DOT formats.
- [ ] Add runtime descriptors for Osprey heap object kinds and typed outgoing
      edges.
- [ ] Add root enumeration for stack locals, module globals, fibers, channels,
      active effects, runtime tables, and backend-specific roots.
- [ ] Add `--debug-memory=object-graph` and compile/link only the required
      profiler metadata in debug builds.
- [ ] Add Osprey DAP custom requests for object graph expansion, retainers,
      roots, snapshots, snapshot diffs, and allocation-site summaries.
- [ ] Add a VS Code watch/variables action that opens the graph visualizer for
      a selected heap-backed value.
- [ ] Implement local-neighborhood graph view with directional expansion,
      hidden-edge counts, search, filters, pinning, grouping, and stable layout.
- [ ] Implement shortest/key retention paths, dominator tree, retained size, and
      explicit approximate/unavailable labelling.
- [ ] Add snapshot diff and replay snapshot hooks.
- [ ] Test structural sharing, closure capture retention, fiber/channel/effect
      roots, conservative-root approximation, custom-manager unsupported mode,
      large-graph paging, cancellation, and deterministic JSON/DOT export.

### Phase 5 - Expression evaluation

- [ ] Parse/type-check watch expressions in Osprey syntax.
- [ ] Resolve names against current frame scopes.
- [ ] Implement pure expression evaluation.
- [ ] Gate side effects behind explicit user action.
- [ ] Add REPL/watch/hover tests.

### Phase 6 - Fibers, channels, effects

- [ ] Add stable runtime debug ids for fibers and channels.
- [ ] Expose runtime debug inspection APIs.
- [ ] Add fiber list and logical stack display.
- [ ] Add breakpoints on spawn/await/send/recv/select.
- [ ] Expose effect handler stack and resume events.
- [ ] Add concurrent/effects debugger tests.

### Phase 7 - Replay/time travel

- [ ] Define `.ospreytrace`.
- [ ] Record scheduler, IO, time/random, process, network, and effect events.
- [ ] Implement deterministic replay runner.
- [ ] Add checkpointing for reverse operations.
- [ ] Add replay tests for fibers/effects.

### Phase 8 - Optimized debug mode

- [ ] Implement debug metadata coverage metrics.
- [ ] Add unoptimized-vs-optimized stepping comparisons.
- [ ] Add control-flow conformance validation for debug locations.
- [ ] Add dynamic call-tree validation for representative programs.
- [ ] Enable `--debug-opt=limited`, then `--debug-opt=optimized` only after
      tests define the contract.

## Testing matrix

Compiler tests:

- Parse span tests.
- Debug IR golden tests.
- Type metadata tests.
- Variable metadata tests.
- Non-debug IR unchanged tests.

Native debugger tests:

- LLDB command-script tests.
- `llvm-dwarfdump` line/function/variable checks.
- macOS dSYM checks.
- Linux ELF DWARF checks.
- Windows MinGW DWARF checks when CI is available.

DAP tests:

- Initialize/launch/configurationDone.
- SetBreakpoints.
- Continue/stopped.
- StackTrace.
- Scopes/variables.
- Evaluate.
- Object graph custom requests: selected root, outgoing expansion, incoming
  retainers, roots, retention paths, snapshot, snapshot diff, cancellation.
- Disconnect/terminate.

Osprey semantic tests:

- Functions, top-level statements, blocks.
- Lambdas and closures.
- Records/unions/Result/list/map.
- Match and pattern bindings.
- Fibers/channels/select.
- Effects/handlers/resume.
- FFI safe/opaque boundary.

Regression tests:

- Breakpoint on every executable source line in selected examples.
- Step sequence is stable for `--debug-opt=none`.
- Variables are correct or explicitly unavailable.
- Object graph output is bounded, deterministic after sorting/canonicalization,
  and labels approximate/unavailable retention data.
- Runtime/generated frames hidden by default.

## Risks

- LLVM textual debug metadata is easy to get syntactically valid but semantically
  weak. Mitigation: validate with LLDB and `llvm-dwarfdump`, not just string
  checks.
- Osprey currently optimizes at the clang invocation level. Debug mode must
  override that without changing release behavior.
- Runtime handles are opaque. Pretty-printers need stable runtime inspection
  APIs, not ad hoc memory guessing.
- Object graph visualizers can become unreadable on real heaps. Mitigation:
  start from focused neighborhoods and aggregated dominators/allocation sites,
  require paging/filtering/cancellation, and treat layout stability as a testable
  feature.
- Retained-size computation is only as good as the root/edge model. Mitigation:
  label conservative/custom-manager gaps explicitly and prefer unavailable over
  fake precision.
- Fibers/effects may require runtime changes to expose logical stacks.
- macOS debug symbols may require dSYM handling even when ELF-like tests pass
  elsewhere.
- Windows native debug format support will lag unless the project adopts an
  MSVC/CodeView path.

## Definition of done

The Osprey debugger is complete when:

- A user can debug normal Osprey programs from VS Code and from a headless DAP
  test runner.
- Breakpoints, stepping, stacks, scopes, variables, and expression evaluation
  work for the language constructs in `tests/regressions`.
- Osprey runtime values render as language values, not raw implementation
  pointers.
- The watch/variables UI can open an object graph for heap-backed values, show
  connected objects and retainers, compute root paths/retained size when
  supported, compare snapshots, and export deterministic graph data.
- Fibers and effects are inspectable at the language level.
- Replay can reproduce deterministic concurrency/effects executions.
- Debug-info quality is continuously tested, including optimized-debug modes
  once those modes are enabled.
- `make ci` includes the non-GUI debugger checks; GUI/VS Code checks remain in
  the extension test lane.
