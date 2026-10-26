# Editor features and boundaries

The [extension README](README.md) covers installation, commands and settings. This page records implementation boundaries and verification paths. The extension registers `.osp` and `.ospml`; Python-flavor compiler support does not yet provide equivalent `.py` editor integration.

## Language service

`osprey lsp` runs the compiler frontend in-process and loads project modules. Implemented requests include diagnostics, context-aware completion, hover, signatures, go-to-definition, references, document symbols, formatting and checked quick fixes. Rename, semantic tokens, call hierarchy and workspace-symbol requests are not advertised by the current server. Definitions and references work across loaded project files; they are not pending editor-routing fixes.

The service uses inferred bindings and written types for hover. Arbitrary subexpression hover remains limited. Completion and diagnostics follow the implemented language contract, including unhandled-effect errors and import repairs. [Spec 0020](../docs/specs/tooling/0020-LanguageServerAndEditors.md) defines the request-specific behavior and remaining limits.

Syntax highlighting uses the bundled [Default](syntaxes/osprey.tmGrammar.json) and [ML](syntaxes/osprey-ml.tmLanguage.json) TextMate grammars. Highlighting a reserved word does not establish compiler support for that construct.

## Running, debugging and tests

- **Compile:** `Osprey: Compile Osprey File`, bound to Ctrl/Cmd+Shift+B.
- **Run:** `Osprey: Compile and Run Osprey File` in the Command Palette.
- **Debug:** `Osprey: Debug Osprey File`, bound to F5, compiles native DWARF and launches LLDB-DAP. Breakpoints, stepping, primitive values and supported record fields are implemented. Collection/union renderers, optimized values and concurrency inspection remain open in [plan 0012](../docs/plans/0012-osprey-debugger.md).
- **Test Explorer:** discovers Default and ML test suites and provides Run, Coverage, Profile and Debug profiles. Case documentation and skipped/unexecuted-case warnings appear in the editor.
- **Hunt:** `Osprey: Hunt for Bugs` invokes `osprey hunt --json`; findings become diagnostics and the report appears in the Osprey Hunt output channel. KLEE and runtime bitcode are separate prerequisites; see the [Hunt guide](../website/src/docs/hunt.md).
- **Profile:** launch or attach CPU sampling on supported macOS/Linux runtimes. The shared profiler UI presents captured samples; a Windows sampler and ARC/GC heap profiling remain delivery work in [plan 0035](../docs/plans/0035-native-profiler-backends.md).

Native compilation requires the host C/LLVM toolchain. Debugging requires `lldb-dap`; terminal input requires LLDB-DAP 21 or newer. Platform-specific compiler capabilities still apply to every editor command.

## Verification

[TESTING.md](TESTING.md) describes the extension test workflow. From this directory, `npm run compile` builds the TypeScript client and `npm test` runs the VS Code suite. Node.js 20 or newer is required for extension development; the manifest's VS Code minimum is 1.96.

- [Language-service tests](../crates/osprey-lsp/src/server/tests/) exercise the JSON-RPC requests and project analysis.
- [Editor tests](test/suite/) cover activation, navigation, highlighting, test discovery, native debugging, Hunt and profiling.
- [Client implementation](client/src/) and [manifest](package.json) define command names, document selectors, settings and debugger contributions.

Use these sources when changing a support claim. A command contribution alone does not prove its runtime backend is available.
