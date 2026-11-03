# Debugger

Debug metadata retains positions from the authoring `.osp` or `.ospml` source.

Compiler and language rules are separate from integration tooling. See [Compiler debug artifacts](../compiler/DebugArtifacts.md) for the extracted contract.

## Protocol Split `[DEBUGGER-PROTOCOLS]`

- LSP (`osprey lsp`) provides editor analysis.
- DAP provides launch, breakpoints, stepping, stack traces, scopes, variables,
  evaluate, pause, and terminate.

F5 starts a DAP session; `osprey.run` remains a separate run command. LSP and
DAP use the same source identity and AST positions.

## Editor Launch `[DEBUGGER-EDITOR-LAUNCH]`

For VS Code:

1. The debug provider resolves the Osprey source file (`.osp` or `.ospml`) from
   the active editor or launch configuration.
2. Dirty documents are saved or the debug launch is rejected.
3. The provider runs the version-matched compiler:

   ```text
   osprey <source.osp> --debug --debug-opt=none --compile -o <debug-binary>
   ```

4. The provider launches a DAP adapter, initially `lldb-dap`, against the
   compiled native binary.
5. DAP handles breakpoints, stepping, stack, scopes, and variables.

Launch configuration accepts the program, arguments, working directory,
environment, stop-on-entry, debug output path, and LLDB-DAP path. Compiler
resolution uses the extension's configured Osprey compiler, unless `compilerPath` selects an executable for this launch. A missing override fails the launch rather than falling back. `preserveArtifacts: true` adds the IR and symbol preservation flags. The editor always passes `--debug-opt=none` so an inherited optimizer override cannot silently alter stepping. Console selection follows [DEBUGGER-CONSOLE].

## Program input and output `[DEBUGGER-CONSOLE]`

Launch configurations accept `console: "internalConsole" | "integratedTerminal" | "externalTerminal"`. The default is `internalConsole`: program output reaches the Debug Console through DAP output events; this console is not stdin. LLDB's native pseudo-terminal can translate newlines to CRLF. `integratedTerminal` supplies interactive stdin in a VS Code terminal. `externalTerminal` asks VS Code to open its configured external terminal. Arguments, environment and working directory retain the ordinary launch contract.

Terminal modes require a verified LLDB-DAP version of at least 21, which introduced the three-way console contract. A failed, timed-out or unrecognised version probe rejects the launch with an actionable diagnostic; it must not silently launch in the Debug Console. The probe runs the selected adapter directly, without a shell, and is bounded to three seconds. Internal-console launches do not need a version probe. This requirement is specific to terminal selection; it does not change the compiler's LLVM floor. Xcode's lldb-dap reports LLVM 21 but never completes a `runInTerminal` launch, so a terminal mode under an adapter installed inside Xcode (`…/Contents/Developer/…`) or the Command Line Tools is refused before the probe, naming Homebrew's `lldb` formula. On macOS the adapter search therefore tries Homebrew's kegs (`opt/lldb`, `opt/llvm`, then each `opt/llvm@NN`, newest first, under `/opt/homebrew` and `/usr/local`) before `xcrun`.

Invalid console values, the obsolete `runInTerminal` launch field and terminal modes combined with custom `launchCommands` are rejected before compilation. `console: "integratedTerminal"` replaces `runInTerminal`. The native adapter receives the same resolved executable that passed the capability check. The Test Explorer keeps the internal default so its TAP verdicts remain available through DAP output events.

Both-flavor `console selection preserves input and breakpoints` tests launch through the actual extension, read a line from the integrated terminal, inspect the resulting source binding, continue and verify output routing. `external console requests the external host terminal` compiles both surfaces and checks the real adapter's external reverse request, executable and working directory without opening an unmanaged operating-system window. Unit cases cover supported, old, unknown and failing probes, conflicting options and the launch schema. The host editor owns the external terminal UI.

The adapter contract is documented by [LLVM 21's LLDB-DAP reference](https://github.com/llvm/llvm-project/blob/release/21.x/lldb/tools/lldb-dap/README.md#configuration-settings-reference).

## Reusable Debugger Helpers `[DEBUGGER-REUSE]`

The `osprey-debug` crate owns source identity and native build policy without
depending on compiler or editor crates. The VS Code extension owns launch
normalization, `lldb-dap` discovery, and native pre-launch compilation.

The editor-side debugger glue is language-neutral and MUST be shared via a
package under the [LspKit](https://github.com/Nimblesite/lspkit) umbrella, not
forked into each extension. This is a new shared TypeScript package (VS Code DAP
glue + DAP test harness) alongside a future `lspkit-debug` Rust crate for native
debug-build policy. Not implemented yet.

## Effect Trace `[DEBUGGER-EFFECT-TRACE]`

A native stack answers "how did control get here" only while control got here
by calling. A resumed continuation did not: the frames below a `resume` belong
to the handled body, the frames above belong to the arm, and the line joining
them is the `perform` the arm is answering. The physical stack cannot express
that edge, so a debug build MUST carry the effect trace of
[MULTI-TRACE](../language/0017-AlgebraicEffects.md#effect-trace--multi-trace) beside it.

A paused session presents the trace as its own view: performed at *site*,
handled at *region*, resumed *n* times, innermost first. Each entry resolves to
a source position through `[DEBUGGER-SOURCE-MAP]`, so selecting one navigates
to the `perform` or the arm that answered it. Sites belonging to a
`static effect` MUST NOT appear — the rewrite removed them from the program
before code generation, and a trace naming them would describe code the binary
does not contain
([STAGE-RESIDUE](../language/0017-AlgebraicEffects.md#zero-residue--stage-residue)).

For a continuation resumed at most once the trace is a straight line and adds
context to the physical stack. For a multi-shot continuation it is the only
stack that corresponds to the source, because the physical stack after a second
resume describes a control path no source line expresses.

## Value rendering `[DEBUGGER-VALUE-RENDER]`

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

Lists and maps are paged and do not dump unbounded memory. Runtime handles never
crash the debug session when null/corrupt; they render as invalid/unavailable.

Record field expansion is implemented; the remaining renderers are not
implemented yet.

## Object graph and memory profiler `[DEBUGGER-OBJECT-GRAPH]`

The object graph is part of the debugger, not a separate heap-dump tool. It
starts from the same value references used by the variables/watch UI and adds
graph, root, retention, ownership, and snapshot analysis.

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
  frame/fiber, root set, nodes, edges, aggregation nodes, profiler warnings, and
  backend capability flags.

Prefixed custom requests are added only for graph-shaped data:
`osprey/objectGraph`, `osprey/objectGraph/expand`, `osprey/objectGraph/retainers`,
`osprey/objectGraph/roots`, `osprey/heapSnapshot`, `osprey/heapSnapshot/diff`,
and `osprey/allocationSites`.

Retention analysis:

- Reachability starts at runtime roots, then follows precise Osprey heap edges.
- Incoming retainers are computed from the reverse graph.
- Shortest paths to roots answer the first retention question quickly.
- Key retention paths de-duplicate equivalent paths so one noisy stack root does
  not hide independent owners.
- Dominator tree and retained size are computed for complete snapshots. For
  snapshot roots, a synthetic super-root dominates all real roots.
- Conservative GC roots, FFI handles, and custom managers can make retention
  incomplete. The UI must mark affected paths/sizes approximate or unavailable.
- Snapshot diff compares object count, shallow bytes, retained bytes, type,
  allocation site, owning fiber, and root-path changes.

Retained size is computed from a dominator tree when the snapshot has a complete
enough root set and edge set, using the Lengauer-Tarjan dominator algorithm; when
conservative roots or custom managers make the graph incomplete, retained size is
labelled approximate or unavailable.

Object graph profiling is off in release builds and opt-in in debug builds.
Runtime metadata is compiled only under `--debug-memory=object-graph` or stronger
modes unless a minimal id/descriptor table is cheap enough to keep in all debug
builds. Snapshot capture must have a bounded memory budget and cancellation path.
Large collections and maps are paged. Huge object groups collapse by default.
Native addresses and raw bytes are hidden unless the user enables expert native
view.

Not implemented yet.

## Expression evaluation `[DEBUGGER-EVALUATION]`

Required modes:

- `hover`: display variable value for symbols in scope.
- `watch`: evaluate pure Osprey expressions.
- `repl`: evaluate expressions and optional debugger commands.
- `call`: explicitly allow side-effecting calls only when the user requests it.

Side-effecting operations, FFI, IO, spawning, HTTP, and effect operations are
blocked or prompted for unless explicitly enabled.

Not implemented yet.

## Fibers, channels, effects `[DEBUGGER-FIBERS-EFFECTS]`

- Each fiber has a stable debug id.
- The debugger lists fibers with state: runnable, running, waiting, completed,
  failed.
- The stack for suspended fibers is available when the runtime representation
  allows it.
- Breakpoints can stop all fibers or only the triggering fiber.
- Scheduler stepping can advance one fiber or run until the next breakpoint.
- Channel state is inspected; waiting senders/receivers are shown if the runtime
  tracks them; break on send/receive/select operations.
- The active handler stack is shown; the performed effect operation and
  arguments are shown; step into the handler arm; step over `resume`; break on
  unhandled effect or invalid resume.

Not implemented yet.

## Deterministic replay `[DEBUGGER-REPLAY]`

Required record stream:

- Program args, cwd, env, Osprey version, compiler version, target triple.
- External inputs: stdin, files when sandbox/record mode permits, network when
  enabled, clock/random/process results.
- Scheduler decisions for fibers/channels/select.
- Effect operation/resume ordering.
- FFI boundary calls when recordable; opaque calls marked non-replayable.
- Runtime allocation ids for stable handle identity where needed.

Replay commands: `osprey file.osp --debug --record trace.ospreytrace --run` and
`osprey debug replay trace.ospreytrace`. Reverse-continue and reverse-step exist
once snapshots/checkpoints exist. A run fails deterministically if it depends on
non-recorded external state.

Not implemented yet.

## Optimized debugging `[DEBUGGER-OPTIMIZED]`

- `--debug-opt=none` is the default and the first supported mode.
- `--debug-opt=limited` may enable optimizations proven not to break line/var
  expectations for covered cases.
- `--debug-opt=optimized` is allowed only after validation infrastructure is in
  place; it must be honest about unavailable variables.

Validation:

- Per-variable history oracle (Kell & Stinnett): each local's observed
  value-change history must match the source program's, with each change at the
  correct file and line.
- Control-flow conformance (Huang et al.): for every instruction a pass
  moves/clones/merges, assert the optimized path's location set is a subset of
  the source path's, and that the Preserve/Merge/Drop choice follows from that
  containment.
- Source-level dynamic call-tree differential (Stinnett & Kell): compare the
  call tree recovered from an optimized run against its unoptimized reference;
  attribute divergences to compiler vs debug-info format.
- Cross-debugger differential testing (DTD, Lu et al.): diff full program state
  between two adapters over the same binary, treating optimized-out / error /
  not-found as distinct states.

`--debug-opt=none` is implemented. `--debug-opt=limited` and
`--debug-opt=optimized` are not implemented yet, and the validation
infrastructure is not yet in place.

## Security and packaging `[DEBUGGER-SECURITY]`

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
- The VS Code extension should not bundle a large LLDB toolchain initially. It
  should detect and explain missing LLDB-DAP. Release notes must state debugger
  prerequisites.

Launch-on-user-action, path quoting, missing-LLDB-DAP detection, the
no-bundled-toolchain rule, and the absence of remote attach are implemented.
Expression evaluation, replay, their opt-ins, and opt-in debug adapter logs are
not implemented yet.
