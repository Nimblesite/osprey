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
