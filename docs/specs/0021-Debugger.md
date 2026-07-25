# Debugger

Debug metadata retains positions from the authoring `.osp` or `.ospml` source.

## Protocol Split `[DEBUGGER-PROTOCOLS]`

- LSP (`osprey lsp`) provides editor analysis.
- DAP provides launch, breakpoints, stepping, stack traces, scopes, variables,
  evaluate, pause, and terminate.

F5 starts a DAP session; `osprey.run` remains a separate run command. LSP and
DAP use the same source identity and AST positions.

## Debug Build Contract `[DEBUGGER-BUILD]`

`osprey --debug --compile` builds a native executable suitable for source-level
debugging.

- `--debug` is accepted by `--llvm`, `--compile`, and `--run`.
- Native debug builds emit LLVM debug metadata that lowers to DWARF.
- Native debug builds pass `-g -fno-omit-frame-pointer` and default to `-O0`;
  `OSPREY_DEBUG_OPT` overrides the optimization flag.
- Non-debug builds keep their release-oriented defaults.
- `--debug --target=wasm32` is rejected.
- Debug metadata uses DWARF 4 on macOS and DWARF 5 elsewhere.
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

Launch configuration accepts the program, arguments, working directory,
environment, stop-on-entry, debug output path, and LLDB-DAP path. Compiler
resolution uses the extension's configured Osprey compiler.

## Reusable Debugger Helpers `[DEBUGGER-REUSE]`

The `osprey-debug` crate owns source identity and native build policy without
depending on compiler or editor crates. The VS Code extension owns launch
normalization, `lldb-dap` discovery, and native pre-launch compilation.

## Variables `[DEBUGGER-DBG-DECLARE]`

Primitive function parameters use `llvm.dbg.value`. Primitive `let` bindings
use an addressable debug-only slot and `llvm.dbg.declare`, so LLDB/DAP can read
them while paused. Composite values have no Osprey-specific renderer.
