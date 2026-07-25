# Debugger

Debug metadata derives from the lowered program and retains positions from the
authoring `.osp` or `.ospml` source.

## Protocol Split `[DEBUGGER-PROTOCOLS]`

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
- The compile unit currently uses `DW_LANG_C` as its debugger language code.

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

The `osprey-debug` crate owns source identity and native build policy without
depending on the parser, type checker, code generator, or editor. The VS Code
extension owns launch normalization, `lldb-dap` discovery, native pre-launch
compilation, and its DAP test client.

## Variables `[DEBUGGER-DBG-DECLARE]`

Primitive function parameters use `llvm.dbg.value`. Primitive `let` bindings
use an addressable debug-only slot and `llvm.dbg.declare`, allowing LLDB/DAP to
read their values while paused. Composite Osprey-specific renderers are not
part of the current debugger surface.

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
5. Debugger helpers retain the dependency boundaries in `[DEBUGGER-REUSE]`.
6. Primitive parameter and local metadata is verified through LLDB/DAP, and
   the DWARF version follows the platform default.
