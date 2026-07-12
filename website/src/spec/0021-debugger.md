---
layout: page
title: "Debugger"
description: "Osprey Language Specification: Debugger"
date: 2026-07-12
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0021-debugger/"
---

# Debugger

> **Engineering spec** (tooling), not part of the `0001`-`0019` language
> reference. It defines how Osprey programs are built, launched, and inspected
> by debuggers.

The Osprey debugger is a source-level debugging system for native Osprey
programs. It is integrated with editors through the same extension surface as
the language server, but it uses a different protocol: LSP is the static
analysis plane; DAP is the runtime control plane. The implementation plan is
[Plan 0012](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0012-osprey-debugger.md).

> **Flavor layer — shared core (AST and above).**  Debug info derives from the
> canonical `osprey_ast::Program` and is flavor-blind: the same AST yields the
> same `!DISubprogram`/`!DILocation` metadata and the same debug semantics
> regardless of whether the program was authored in the Default (`.osp`) or ML
> (`.ospml`) flavor. Source positions and line tables point at the *authoring*
> flavor's source text, preserved by the lowering contract span rule
> ([FLAVOR-LOWER-CONTRACT] in [Language Flavors](/spec/0023-languageflavors/)):
> desugared nodes carry the `Position` of the source construct, so only the
> mapped source file (`.osp` vs `.ospml`) differs — never the debug
> semantics. The debugger never inspects which flavor produced a program.

## Status

| Capability                          | State                                                                                                     |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Debug build mode (`osprey --debug`) | Implemented for native Phase 1 builds: LLVM/DWARF metadata, debug driver flags, and wasm rejection.       |
| VS Code DAP launch                  | Implemented for Phase 2: compiles `.osp` to a native debug binary and launches real `lldb-dap`.           |
| Variables / value rendering         | Partial. Primitive params/lets emit metadata and are DAP-tested; full Osprey value renderers are planned. |
| Object graph / memory profiler      | Planned. Extends the watch/variables surface with retention paths, object neighborhoods, and snapshots.   |
| Fibers / effects inspection         | Planned. Requires runtime debug APIs.                                                                     |
| Replay / time travel                | Planned. Requires deterministic runtime event recording.                                                  |

## Protocol Split `[DEBUGGER-PROTOCOLS]`

Osprey uses two editor protocols:

- **LSP** (`osprey lsp`) owns editor-time analysis: diagnostics, hover,
  symbols, definition, completion, and source position normalization.
- **DAP** owns runtime control: launch, breakpoints, stepping, stack traces,
  scopes, variables, evaluate, pause, and terminate.

The debugger MUST NOT fake a debug session by canceling DAP and running
`osprey --run`. The `osprey.run` command is a run command; F5 is a debugger
launch.

Both planes MUST agree on source identity and positions. AST/source positions
used by LSP are also the provenance for emitted debug metadata.

## Debug Build Contract `[DEBUGGER-BUILD]`

`osprey --debug --compile` builds a native executable suitable for source-level
debugging.

Required behavior:

- `--debug` is accepted by `--llvm`, `--compile`, and `--run`.
- Native debug builds emit LLVM debug metadata that lowers to DWARF.
- Native debug builds pass debugger-friendly driver flags (`-g`, no omitted
  frame pointer where supported).
- Native debug builds default to no optimization (`-O0`) unless an explicit
  debug optimization override is supplied.
- Non-debug builds keep their release-oriented defaults.
- `--debug --target=wasm32` is rejected until WebAssembly debug information is
  specified and tested.
- The emitted DWARF version is platform-aware: default to **DWARF 4 on macOS**
  (Apple `dsymutil`/LLDB lag on v5 features such as `.debug_names` and
  `DW_FORM_strx`) and DWARF 5 elsewhere when the target toolchain supports it.
  Hard-coding DWARF 5 for the macOS-first target is a defect.
- Language identity: until a registered DWARF language code for Osprey exists,
  `DW_LANG_C` is the interim choice, but it is NOT neutral — debuggers apply
  C expression-eval, formatting, demangling, and array-lower-bound semantics
  from it, which the Osprey-aware evaluator (see Plan Layer 6) must override.
  The real fix is to register `DW_LNAME_Osprey` with dwarfstd.org and emit the
  DWARF 6 `DW_AT_language_name` + `DW_AT_language_version` pair while still
  dual-emitting legacy `DW_AT_language` for older consumers. See
  [Plan 0012, Layer 2](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0012-osprey-debugger.md).

Minimum emitted metadata:

- `source_filename`.
- `!llvm.dbg.cu`.
- `!llvm.module.flags` including debug-info version and DWARF version.
- `!DIFile`.
- `!DICompileUnit`.
- `!DISubprogram` for user functions and generated `main`.
- `!DILocation` on instructions derived from executable source statements.

## Source Mapping `[DEBUGGER-SOURCE-MAP]`

The parser and lowerers must preserve source positions for executable
statements and declarations.

Rules:

- Osprey AST positions use 1-based lines and 0-based columns.
- DAP/source debugger positions exposed to users use 1-based lines and columns.
- Emitted DWARF/`!DILocation` lines and columns are 1-based. The 0-based AST
  column MUST be converted with `column + 1` before emission, because LLVM
  reserves `!DILocation` column `0` as the "no column" sentinel — emitting a
  raw 0-based column collides with it and yields off-by-one or dropped column
  data. A 1-based AST line maps straight through.
- Compiler-generated code may be associated with the nearest source statement
  only when doing so improves stepping/breakpoint behavior.
- Generated helper frames should be hidden from normal stepping once smart
  stepping exists.

## Editor Launch `[DEBUGGER-EDITOR-LAUNCH]`

For VS Code:

1. The debug provider resolves the Osprey source file (`.osp` or `.ospml`) from
   the active editor or launch configuration.
2. Dirty documents are saved or the debug launch is rejected.
3. The provider runs the version-matched compiler:

   ```text
   osprey <source.osp> --debug --compile -o <debug-binary>
   ```

4. The provider launches a real DAP adapter, initially `lldb-dap`, against the
   compiled native binary.
5. DAP handles breakpoints, stepping, stack, scopes, and variables.

The extension may let users configure:

- Osprey compiler path.
- LLDB-DAP path.
- Debug output path.
- Program args.
- Working directory.
- Environment variables.
- Stop-on-entry.

## Reusable Debugger Helpers `[DEBUGGER-REUSE]`

Generic debugger utilities MUST NOT be re-implemented per language. Osprey,
Basilisk (Python) and SharpLsp (C#) each ship a VS Code debugger and are
converging on the shared [LspKit](https://github.com/Nimblesite/lspkit) toolkit.
The debugger glue has two reuse layers, and duplication across the three
projects in either layer is a defect:

**Compiler/native layer (Rust).** Debug-build policy (`-g`, `-O0`,
`-fno-omit-frame-pointer`, platform DWARF version), debug source identity, and
DWARF helpers are editor- and language-neutral. They live in `osprey-debug`
today as the seed and are candidates to upstream into an `lspkit-debug` crate
(LspKit currently has no debugger code). `osprey-debug` intentionally avoids
Osprey parser, type-checker, codegen, and editor dependencies. This layer only
benefits other native-compiled languages; Python/C# do not consume it.

**Editor layer (TypeScript).** This is where Osprey, Basilisk and SharpLsp
actually triplicate code, so it is the priority reuse target. The following are
language-neutral and MUST be hoisted into a shared package under the LspKit
umbrella rather than forked into each extension:

- DAP adapter resolution (setting override → common toolchain paths → PATH,
  with a precise missing-tool error). SharpLsp's `findNetcoredbg` /
  `getNetcoredbgCandidates` is the reference shape.
- Debug-launch config synthesis/normalization (empty F5 config → defaults,
  missing program → active file / entry artifact, profile merge). Basilisk's
  pure `applyDebugConfigDefaults` and SharpLsp's `resolveDebugConfiguration`
  are the reference shapes.
- Save-dirty-documents-or-reject, and the pre-launch native build hook.
- The DAP test harness: a DAP client (initialize/launch/setBreakpoints/
  continue/stepIn/Out/Over/stackTrace/scopes/variables/evaluate), poll helpers,
  and UI stubs. Basilisk's debug-integration test client and SharpLsp's
  `pollUntilResult` / UI stubs are the reference shapes.

Only the genuinely language-specific bits stay in each extension: the debug
`type`, adapter name (`lldb-dap` for Osprey), compiler/build command, and
toolchain-specific paths. Osprey-specific lowering remains in `osprey-codegen`.
The Osprey extension's debugger code is the seed to upstream, not a private
fork to grow in isolation.

## Future Runtime Inspection `[DEBUGGER-RUNTIME]`

The finished debugger must inspect Osprey values, not just native pointers.

Required future support:

- Local variables and parameters via `DILocalVariable` plus LLVM value-location
  records. Prefer LLVM 19+ `#dbg_value` / `#dbg_declare` debug records; the
  current textual backend may use the older `@llvm.dbg.value` /
  `@llvm.dbg.declare` compatibility form while it is verified to lower to
  correct DWARF in supported toolchains.
- Records, unions, `Result`, strings, lists, maps, closures, fibers, channels,
  and effect handlers rendered as Osprey values.
- Safe runtime inspection helpers for opaque handles.
- Object graph inspection for any heap-backed variable or watch expression.
- Fiber and effect runtime debug ids.
- Replayable scheduler/effect/IO event streams.

These features are not allowed to guess raw memory layouts ad hoc from the
editor. Stable runtime inspection APIs are required.

## Object Graph Watch Window `[DEBUGGER-OBJECT-GRAPH]`

The debugger must integrate an object graph and memory-profiler view directly
into the watch/variables workflow. Selecting a variable or evaluating a watch
expression must let the user answer two questions without leaving the debugger:

1. What heap values is this value connected to?
2. What roots, variables, fibers, runtime handles, or shared structures are
   keeping this value reachable?

This is a debugger/profiler feature, not an Osprey language API. It must obey
[Memory Management](/spec/0018-memorymanagement/) [MEM-DEBUG-OBSERVABILITY]: object
identity, addresses, retained size, roots, allocation sites, reference counts,
and collector/backend state are visible only in explicit debug/profiling
surfaces and never become program-observable semantics.

Required model:

- Every heap-backed value that can appear in a variables/watch response has a
  stable debug object id for the current process or replay trace. The id is not
  a language-level identity and need not be a raw address.
- Nodes expose debugger metadata: Osprey type, runtime kind, value summary,
  shallow size, retained size when computed, allocation site/source span when
  known, owning fiber, allocation generation/timestamp when recorded, backend
  provenance (`arc`, `gc`, custom), and validity (`live`, `collected`,
  `moved`, `unavailable`, `corrupt`).
- Edges are typed and directional: record field, tuple/list index, map key/value,
  union payload, closure capture, persistent-collection sharing, fiber capture,
  channel queue, effect handler/resumption, runtime handle, stack/global root,
  and backend bookkeeping. The UI must distinguish incoming retainers from
  outgoing references.
- Roots are first-class nodes/categories: selected local/watch value, stack
  slots, module globals, active fibers, suspended fibers, channels, effect
  handlers, runtime singletons, FFI handles, and conservative-GC roots when the
  backend can only report an approximate native root.

Required debugger operations:

- Expand from a selected variable into outgoing children, incoming retainers, or
  both, lazily and with paging.
- Show shortest paths to roots and "key" distinct retention paths, not only a
  single path that hides alternate owners.
- Compute a dominator tree and retained size for snapshots where the root set
  and graph are complete enough. Retained-size results must be labelled
  unavailable or approximate when custom managers or conservative roots make the
  graph incomplete.
- Group/aggregate graph regions by Osprey type, source allocation site, owning
  fiber, runtime kind, module, or user-defined watch selection.
- Capture snapshots at a breakpoint, on demand from the watch window, or during
  replay; compare snapshots by object count, shallow bytes, retained bytes,
  allocation site, and retention-path changes.
- Export the current graph/snapshot as JSON and DOT for bug reports and
  reproducible tests.

Required visual behavior:

- The variables tree remains the canonical drill-down for exact values; the
  graph view is a companion visualizer reachable from the same selected
  variable/watch expression.
- The initial view is a focused object neighborhood, not the whole heap. Whole
  heap visualizations must start from aggregated dominators, allocation sites,
  or type/fiber groups.
- Layout must be stable across expansions and refreshes so a paused program does
  not visually scramble while the user drills down.
- Large graphs must avoid hairballs through focus+context navigation, filters,
  edge bundling where useful, hidden-edge counts, top-K retained-size defaults,
  search, pinned nodes, and collapsible aggregate nodes.
- Text must not overlap nodes/edges; color must not be the only carrier of
  ownership, lifetime, or retention warnings.

Design authorities:

- GCspy, Printezis and Jones, OOPSLA 2002, introduced a reusable architecture
  for collecting, transmitting, storing, and replaying memory-management
  behavior:
  <https://dl.acm.org/doi/10.1145/583854.582451>.
- Cork, Jump and McKinley, POPL 2007, uses summarized points-from graphs to
  identify heap growth in garbage-collected languages:
  <https://dl.acm.org/doi/10.1145/1190215.1190224>.
- Object ownership profiling, Rayside et al., ASE 2007, uses ownership views to
  find and fix leaks:
  <https://dl.acm.org/doi/10.1145/1321631.1321661>.
- Reiss, VISSOFT 2009, motivates interactive heap visualization for extracting
  actionable memory-problem information:
  <https://ieeexplore.ieee.org/document/5336418/>.
- AntTracks TrendViz, Weninger et al., ICPE 2019, combines trace-based heap
  reconstruction with configurable grouping and time evolution:
  <https://dl.acm.org/doi/10.1145/3302541.3313100>.
- Chrome DevTools, Eclipse MAT, JetBrains dotMemory, and Visual Studio memory
  tools establish the production vocabulary: shallow size, retained size,
  dominators, paths to roots, retention paths, snapshot diffing, and hot paths:
  <https://developer.chrome.com/docs/devtools/memory-problems/get-started>,
  <https://eclipse.dev/mat/>,
  <https://www.jetbrains.com/help/dotmemory/Retained_by.html>,
  <https://learn.microsoft.com/en-us/visualstudio/profiling/hot-path-to-root>.
- Graph-visualization guidance comes from Herman/Melancon/Marshall's survey,
  Holten's hierarchical edge bundles, and Munzner's H3 focus+context work:
  <https://dl.acm.org/doi/abs/10.1109/2945.841119>,
  <https://dl.acm.org/doi/10.1109/TVCG.2006.147>,
  <https://dl.acm.org/doi/10.5555/857188.857627>.

## Conformance

A change is conformant only if:

1. `osprey --debug --llvm` emits the minimum debug metadata in
   `[DEBUGGER-BUILD]`.
2. `osprey --debug --compile` produces a native executable that a supported DAP
   adapter can launch.
3. The VS Code debugger contribution starts a DAP session; it does not proxy to
   `osprey --run`.
4. LSP and debugger source positions follow `[DEBUGGER-SOURCE-MAP]`, including
   the `column + 1` DWARF emission rule.
5. Generic debugger utilities remain isolated AND upstreamable per
   `[DEBUGGER-REUSE]`; the editor DAP glue and DAP test harness are not
   duplicated across Osprey, Basilisk, and SharpLsp.
6. Variable/parameter metadata, once emitted, is verified through LLDB/DAP; the
   IR spelling may be `#dbg_*` records or the current `@llvm.dbg.*`
   compatibility intrinsics, and the DWARF version honors the per-platform
   default (DWARF 4 on macOS).
7. Object graph inspection, once enabled, follows `[DEBUGGER-OBJECT-GRAPH]`:
   stable debug ids, typed incoming/outgoing edges, root paths, bounded lazy
   expansion, labelled approximation, and no editor-side raw-layout guessing.