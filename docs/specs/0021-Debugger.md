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

## Native build controls `[DEBUGGER-BUILD-OPTIONS]`

Each debug modifier enables native debug build policy, with the same target and profiling exclusions as `--debug`:

- `--debug-info=dwarf|none` selects source metadata. `dwarf` is the default; `none` omits both compiler metadata and the driver's `-g` flag while retaining debug optimization and frame pointers.
- `--debug-opt=none` explicitly selects `-O0`, even when `OSPREY_DEBUG_OPT` is set. Without this option, the existing environment override applies. `limited` and `optimized` are rejected until their stepping and variable-history contracts are implemented.
- `--debug-memory=off` explicitly requests the current runtime metadata policy. `object-graph` and `timeline` are rejected as unimplemented; accepting the flag must not imply that inspection exists.
- `--debug-out <path>` (also `--debug-out=<path>`) selects the executable path. It agrees with `-o`; conflicting paths are an error regardless of argument order.
- `--debug-preserve-ir` retains the exact generated IR at `<executable>.ll`, appending the suffix rather than replacing an extension. A native-driver failure still leaves this IR available for diagnosis.
- `--debug-preserve-symbols` retains the executable and its native symbols after `--run`. It requires DWARF. ELF and PE binaries retain embedded metadata; macOS debug and profiling builds collect a `.dSYM` before removing their intermediate object. Failed symbol collection fails the build.

Artifact controls require `--compile` or `--run`. An explicitly selected debug output or either preservation flag makes `--run` retain the executable; without an explicit path it uses the normal compile output location. Otherwise `--run` removes its temporary executable and symbol bundle after exit. Preserving artifacts does not change the program's stdout or exit status. Debug builds bypass the executable-only test cache so metadata and sidecars always belong to the current build.

`debug_build_controls_preserve_inspectable_ir_and_runnable_output` and `debug_run_retains_requested_artifacts_in_both_flavors` compile and execute both surfaces, inspect retained metadata and check exact output. Invocation tests cover unsupported policies, incompatible flags, output conflicts and cache exclusion. Editor tests stop in the retained binary and inspect its local value and source-specific IR.

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

A materialized source lambda has its own native debug scope, declaration location and body locations. This applies when a lambda is returned, bound to a local, passed to another Osprey function or passed as a capture-free C callback. A materialized generic specialization that executes a user function body retains that function's declaration name, position and source parameters. This includes function values placed in typed slots, C function-pointer callbacks and runtime callbacks. Synthetic forwarding adapters without a user body do not acquire an invented source location.

A function or lambda's block body retains its trailing value's own source position, including a block consisting only of a literal, identifier or call. Debug builds associate the return with that position so a breakpoint there stops inside the correct function and can inspect its parameters, captures and completed local bindings. Project assembly and string interpolation preserve these positions in the original source coordinates.

Primitive lambda parameters, immutable captures, captured mutable cells and local bindings are visible in the lambda's scope. The hidden closure environment occupies native argument one; source arguments follow it. A C callback has no environment, so its first source argument is native argument one. Captures are locals, not additional source arguments. Extracted host GPU kernels also retain source-lambda scopes; their flat ABI places captured uniforms before the source parameters, without an environment pointer ([GPU-KERNEL-EXTRACT](0034-GPUComputation.md#kernel-extraction--gpu-kernel-extract)).

`captured_lambda_bodies_keep_debug_scopes_in_both_flavors` and `bound_argument_and_ffi_lambdas_keep_their_debug_scopes` pin the metadata and scope ownership. The editor's `captured lambda breakpoints expose their own variables` cases stop on the return line in both flavors and read the parameter, capture and calculated local through LLDB-DAP. `single_expression_blocks_keep_their_return_locations` checks function/lambda scope ownership of return-only body locations; `single-expression lambda breakpoints retain return locations` verifies the exact stopping line and live values through LLDB-DAP. `generic_function_values_keep_their_source_scope`, `generic_c_callbacks_keep_their_source_scope` and `generic_runtime_callbacks_keep_their_source_scope` check specialized scopes and ABI argument numbering. The editor's `generic function values retain source names and variables` cases inspect integer and float specializations in both flavors.

## Match-arm scopes `[DEBUGGER-MATCH-SCOPES]`

Each selected match arm has its own lexical debug scope. Primitive pattern bindings and arm-local values are inspectable within that scope. A block arm's trailing expression retains its source line, including an identifier-only return. After execution leaves the arm, its locals are no longer visible and an enclosing binding with the same name becomes the selected binding again. LLDB may display both visible shadowed names with source-line labels; evaluating the unqualified name selects the innermost binding.

The editor's `pattern bindings stay in their debugger arm` cases verify the exact return-line stop, the inner pattern value, the arm's calculated local, restoration of the outer value and disappearance of the arm-local variable in both flavors. Union/Result/collection rendering and inlined generic call-frame reconstruction remain unfinished.

## Nested block scopes `[DEBUGGER-BLOCK-SCOPES]`

Ordinary nested block expressions follow the same visibility rule as match arms. Their primitive local bindings belong to a child lexical scope; an unqualified shadowed name selects the innermost visible binding. Leaving the block hides its locals and restores enclosing bindings. This applies inside named functions, materialized lambdas, and both direct and resumable effect handlers, in both flavors.

A nested block's trailing value retains an executable location within that scope, including an identifier-only return. The outer body of a function or handler shares its frame scope so completed local bindings remain inspectable at the function return. Debug-only scope boundaries do not change ordinary generated IR or program results.

`nested_source_blocks_keep_distinct_variable_scopes` asserts child scope ownership and the return location. The editor's `nested blocks restore debugger bindings` cases stop before and after a nested block, inspect the inner value and calculated local, then verify the enclosing value and disappearance of the inner local. They cover named functions, closures and both handler modes in each flavor.

## Binding lifetime `[DEBUGGER-BINDING-LIFETIME]`

A local binding becomes visible after its initializer completes and its value or cell address has been stored. Before that point, the debugger must not expose an uninitialized stack value under the binding's name. An initializer can inspect an enclosing binding with the same name; the new binding shadows it only after initialization. Later declarations remain out of scope. This applies to immutable bindings and shared mutable cells in both flavors.

Variable storage has a stable frame address even when initialization follows an effect-dispatch branch. Declaration instructions belong to the new lexical scope; the preceding initialization stores belong to the enclosing scope. The function's `DISubprogram` identity remains distinct from the current local scope. File-scope publication and cleanup retain their statement's source location, so stepping forward does not hit an earlier declaration again.

The editor's block and pattern scope cases assert that incomplete and later bindings are absent. The `immutable initializer keeps the enclosing debugger binding` and `cell initializer keeps the enclosing debugger binding` cases inspect the outer value during initialization and the completed result afterward. Existing stepping tests and `top_level_bindings_keep_monotonic_source_locations` pin file-scope stepping. Optimized-away values and union/Result/collection rendering remain separate unfinished work.

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
resolution uses the extension's configured Osprey compiler, unless `compilerPath` selects an executable for this launch. A missing override fails the launch rather than falling back. `preserveArtifacts: true` adds the IR and symbol preservation flags. The editor always passes `--debug-opt=none` so an inherited optimizer override cannot silently alter stepping. Console selection remains unfinished.

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

The both-flavor editor cases `handler debugger values follow live cell mutations`, `resuming handler debugger values follow live cell mutations` and `closure debugger values follow live cell mutations` stop twice and require the shared value to change from `42` to `43`, with the correct source parameter on each call. Closure cases also check the calculated local on both stops. Record fields are covered by [DEBUGGER-RECORD-VALUES]; other composite values still have no Osprey-specific renderer.

## Record field inspection `[DEBUGGER-RECORD-VALUES]`

Native debug builds describe record pointers and their named fields with DWARF composite/member metadata. Expanding a named, anonymous or concretely instantiated generic record exposes its actual nested fields, including integer, float, boolean and string values. Distinct generic instantiations must retain distinct debug type identities in one executable; a float field must never be displayed as its integer bits. Osprey's internal record tag is not a user field.

Field offsets follow the physical ABI, including tag words, boolean storage bytes and padding. The built-in C ABI `HttpResponse` has no tag; its boolean remains a full storage byte. Unsupported opaque handles are described as opaque pointers, never guessed to be strings or records. Metadata is confined to native debug builds.

Record parameters, immutable captures and shared mutable cells retain their concrete field types. A cell's debug location follows the live slot; when a handler replaces its record, inspection must show the replacement in the owning scope, closure and direct/resumable handler. Binding lifetime and lexical scope follow [DEBUGGER-BINDING-LIFETIME].

Both-flavor LLDB-DAP fixtures inspect exact field sets and values for nested records, simultaneous generic instantiations, specialized function parameters, anonymous captures, C ABI records and record cells changing from `42` to `43`. `record_debug_fields_match_the_native_layout_in_both_flavors` and `generic_record_parameters_keep_their_field_types_in_debug_metadata` pin metadata references, field types and offsets. This uses the native debugger's field expansion; dedicated collection, union, Result and closure renderers remain unfinished.
