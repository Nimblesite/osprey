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
- The compile unit uses `DW_LANG_C` as its debugger language code.

Minimum emitted metadata:

- `source_filename`.
- `!llvm.dbg.cu`.
- `!llvm.module.flags` including debug-info version and DWARF version.
- `!DIFile`.
- `!DICompileUnit`.
- `!DISubprogram` for user functions, materialized source lambdas and generated `main`.
- `!DILocation` on instructions derived from executable source statements.

Module function names in debug metadata and stack frames use their qualified source identity, such as `billing::Tax::add`; native symbols retain their encoded ABI. The optional DWARF linkage name is omitted because LLDB otherwise displays that encoded name. Both-flavor `module stack frames retain their source names` editor tests stop in a real adapter and check the source name, line and parameter value. See [MODULES-ABI](0025-ModulesAndNamespaces.md#name-mangling-and-abi-modules-abi).

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

## Lambda Scopes `[DEBUGGER-LAMBDA-SCOPES]`

A materialized source lambda has its own native debug scope, declaration location and body locations. This applies when a lambda is returned, bound to a local, passed to another Osprey function or passed as a capture-free C callback. Synthetic compiler adapters do not acquire an invented source location.

A block with local bindings retains its trailing value's own source position, including when that value is a bare identifier. Debug builds associate the return with that position so a breakpoint there can inspect the completed local bindings. Project assembly and string interpolation preserve these positions in the original source coordinates.

Primitive lambda parameters, immutable captures, captured mutable cells and local bindings are visible in the lambda's scope. The hidden closure environment occupies native argument one; source arguments follow it. A C callback has no environment, so its first source argument is native argument one. Captures are locals, not additional source arguments. Extracted host GPU kernels also retain source-lambda scopes; their flat ABI places captured uniforms before the source parameters, without an environment pointer ([GPU-KERNEL-EXTRACT](0034-GPUComputation.md#kernel-extraction--gpu-kernel-extract)).

`captured_lambda_bodies_keep_debug_scopes_in_both_flavors` and `bound_argument_and_ffi_lambdas_keep_their_debug_scopes` pin the metadata and scope ownership. The editor's `captured lambda breakpoints expose their own variables` cases stop on the return line in both flavors and read the parameter, capture and calculated local through LLDB-DAP.

## Editor Launch `[DEBUGGER-EDITOR-LAUNCH]`

For VS Code:

1. The debug provider resolves the Osprey source file (`.osp` or `.ospml`) from
   the active editor or launch configuration.
2. Dirty documents are saved or the debug launch is rejected.
3. The provider runs the version-matched compiler:

   ```text
   osprey <source.osp> --debug --compile -o <debug-binary>
   ```

4. The provider launches a DAP adapter, initially `lldb-dap`, against the
   compiled native binary.
5. DAP handles breakpoints, stepping, stack, scopes, and variables.

Launch configuration accepts the program, arguments, working directory,
environment, stop-on-entry, debug output path, and LLDB-DAP path. Compiler
resolution uses the extension's configured Osprey compiler.

## Reusable Debugger Helpers `[DEBUGGER-REUSE]`

The `osprey-debug` crate owns source identity and native build policy without
depending on compiler or editor crates. The VS Code extension owns launch
normalization, `lldb-dap` discovery, and native pre-launch compilation.

## Effect Trace `[DEBUGGER-EFFECT-TRACE]`

A native stack answers "how did control get here" only while control got here
by calling. A resumed continuation did not: the frames below a `resume` belong
to the handled body, the frames above belong to the arm, and the line joining
them is the `perform` the arm is answering. The physical stack cannot express
that edge, so a debug build MUST carry the effect trace of
[MULTI-TRACE](0017-AlgebraicEffects.md#effect-trace--multi-trace) beside it.

A paused session presents the trace as its own view: performed at *site*,
handled at *region*, resumed *n* times, innermost first. Each entry resolves to
a source position through `[DEBUGGER-SOURCE-MAP]`, so selecting one navigates
to the `perform` or the arm that answered it. Sites belonging to a
`static effect` MUST NOT appear — the rewrite removed them from the program
before code generation, and a trace naming them would describe code the binary
does not contain
([STAGE-RESIDUE](0017-AlgebraicEffects.md#zero-residue--stage-residue)).

For a continuation resumed at most once the trace is a straight line and adds
context to the physical stack. For a multi-shot continuation it is the only
stack that corresponds to the source, because the physical stack after a second
resume describes a control path no source line expresses.

## Variables `[DEBUGGER-DBG-DECLARE]`

Primitive function parameters use `llvm.dbg.value` and a one-based `arg` in `DILocalVariable`. The argument number identifies a formal parameter; omitting it can discard the parameter's location during LLVM instruction selection. Parameters also retain an addressable debug-only slot after their incoming register is reused. Primitive `let` bindings
use the same slot representation and `llvm.dbg.declare`, so LLDB/DAP can read
them while paused. Parameter storage initialization belongs to the native prologue, before the first executable source breakpoint.

A primitive mutable variable promoted to a shared heap cell must expose the live cell value in its owning function, captured lambda and handler arm. Debug storage retains the cell address with a dereferencing location expression; it must not copy the initial value. Direct and resumable handler arms expose their source parameters and primitive captures. Native argument numbering accounts for the hidden environment and, for resumable arms, the continuation parameter.

The both-flavor editor cases `handler debugger values follow live cell mutations`, `resuming handler debugger values follow live cell mutations` and `closure debugger values follow live cell mutations` stop twice and require the shared value to change from `42` to `43`, with the correct source parameter on each call. Closure cases also check the calculated local on both stops. Composite values have no Osprey-specific renderer.
