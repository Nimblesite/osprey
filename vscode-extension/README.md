<p align="center">
  <img src="https://raw.githubusercontent.com/Nimblesite/osprey/main/website/src/assets/images/logo.png" alt="Osprey logo" width="128" />
</p>

# Osprey for VS Code

> **Preview.** Osprey is an actively evolving language and the extension follows the
> compiler's current feature set.

Language support for [Osprey](https://ospreylang.dev) — a functional programming
language built around algebraic effects, inferred types, fibers, and pattern matching. The [effects guide](../website/src/docs/algebraic-effects.md) introduces callable handlers, block-scoped policies, and static selection.

The compiler accepts Default (`.osp`), ML (`.ospml`) and Python flavor (`.py` with `# osprey: flavor=python`). All three lower to the same AST. The extension currently registers Default and ML documents for its language server, commands, debugger and Test Explorer. Use the CLI for Python-flavor files while dedicated editor coverage remains unfinished; see [plan 0031](../docs/plans/0031-python-flavor.md).

Select Default or ML per file using its extension or a leading `// osprey: flavor=ml` marker. Mixed-flavor project imports are supported. The Rust language server (`osprey lsp`, built on [lspkit](https://github.com/Nimblesite/lspkit)) runs the compiler frontend in-process.

## Features

- **Syntax highlighting** — keywords, types, string interpolation
  (`"Hello ${name}!"`), operators, and comments. Default (`.osp`) is fully
  supported alongside ML (`.ospml`); highlighting uses the bundled TextMate grammars.
- **Live diagnostics** — errors and warnings from the Osprey compiler as you
  type, inline in the editor.
- **Hover, go-to-definition, find-references, document symbols, signature help,
  completion and formatting** — driven by the compiler's parser, type checker and project loader.
- **Compile & run** from the editor:
  - `Osprey: Compile Osprey File` (`Ctrl/Cmd+Shift+B`)
  - `Osprey: Compile and Run Osprey File` (Command Palette)
  - `Osprey: Debug Osprey File` (`F5`) uses a native debug build and LLDB-DAP
- **Test Explorer** — `*.test.osp` / `*.test.ospml` files and their cases appear
  in the Testing view, with four run profiles: **Run**, **Coverage**,
  **Profile** (runs the suite under the sampling CPU profiler and opens its
  flame graph plus inline heat annotations), and **Debug** (runs the suite
  under the Osprey debugger, so a breakpoint inside a `test(...)` body stops
  with the case's own bindings live).
- **No test skips silently** — a skipped or ignored case raises a warning you
  cannot miss: a squiggle on the `test(...)` line and a row in the Problems
  panel. A case whose body simply returns `Skip` is flagged as you type, before
  anything runs; a case that skips at run time, or that a run never executed at
  all, is flagged when the run reports. Reviving the case clears its warning.
- **Documented tests** — a `///` block (or ML `(** … *)`) above a `test(...)`
  case shows as the case's description in the Testing tree, renders in full when
  you hover the `test(...)` call, opens as Markdown via
  `Osprey: Show Test Documentation`, and leads the failure message when the case
  fails.
- **Hunt for bugs** — `Osprey: Hunt for Bugs` runs KLEE, prints the report in the Osprey Hunt output channel and places findings at source locations. [Hunt prerequisites and limits](../website/src/docs/hunt.md) apply.
- **CPU profiling** — run the current file or attach to a supported native process, then inspect samples in the shared profiler UI. Memory profiling and a Windows sampler remain unfinished.
- **Bracket matching, auto-closing, and comment toggling.**

## Packages

Run **Osprey: Add Package** from the Command Palette, search the registry by the complete `@org/package` name, and confirm the selection. **Osprey: Remove Package** lists the project's direct dependencies and removes the selected dependency through the same bundled client. Both operations update the manifest and relock through Osprey; unsaved manifest or lock editors block changes. Multi-folder workspaces prompt for the project first.

Set `osprey.packages.registry` to your registry API origin. HTTPS is required; loopback HTTP supports the local folder registry (`make registry-serve`, default `http://127.0.0.1:8787`). Folder-registry listings provide no audit evidence, so the picker labels releases unverified and requires confirmation. Hosted authentication and public trust enforcement are separate delivery work; this editor flow does not establish either.

The VSIX bundles `osprey-pkg` for its platform. `osprey.packages.frontendPath` is a machine setting for an explicit client executable; a configured missing executable fails visibly. Package commands work with the language server disabled and require a trusted workspace.

## Requirements

The extension bundles a version-matched Osprey compiler for your platform and
verifies it at startup. Syntax checking does not require a separate compiler
installation.

To **compile and run** programs, Osprey invokes LLVM and a C toolchain, so install:

- **LLVM/clang** — `brew install llvm` / `scoop install llvm`
- MinGW `gcc` on Windows for runtime linking (`scoop install gcc`)

Or install the full toolchain via a package manager (this also puts `osprey` on
your `PATH`):

```bash
brew install nimblesite/tap/osprey            # macOS / Linux
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket && scoop install osprey   # Windows
```

Debugging also requires `lldb-dap`. Set `osprey.debug.lldbDapPath` when it is outside `PATH`. Integrated and external terminal consoles require LLDB-DAP 21 or newer; the default Debug Console has no stdin. See the [debugger contract](../docs/specs/tooling/0021-Debugger.md).

## Settings

| Setting                      | Default | Description                                                                                                                                       |
| ---------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `osprey.server.enabled`      | `true`  | Enable/disable the language server.                                                                                                               |
| `osprey.diagnostics.enabled` | `true`  | Enable/disable inline diagnostics.                                                                                                                |
| `osprey.server.compilerPath` | `""`    | Path to an Osprey compiler. **Leave empty** to use the version-matched compiler bundled with this extension (falling back to `osprey` on `PATH`). |
| `osprey.debug.lldbDapPath` | `""` | LLDB-DAP executable; empty uses `lldb-dap` on `PATH`. |
| `osprey.hunt.maxTime` | `60` | KLEE exploration budget in seconds; requires at least 1. |

## Links

- Website & docs: <https://ospreylang.dev>
- Source & issues: <https://github.com/Nimblesite/osprey>

## License

See [LICENSE](LICENSE).
