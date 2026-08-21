<p align="center">
  <img src="https://raw.githubusercontent.com/Nimblesite/osprey/main/website/src/assets/images/logo.png" alt="Osprey logo" width="128" />
</p>

# Osprey for VS Code

> **Preview.** Osprey is an alpha language and the extension follows the
> compiler's current feature set.

Language support for [Osprey](https://ospreylang.dev) — a functional programming
language with inferred types, algebraic effects, fiber-based concurrency and
pattern matching.

Osprey has two source flavors. Default (`.osp`) uses braces, `fn` and familiar
function calls. ML (`.ospml`) uses layout, currying and whitespace application.
Both lower to the same AST before type checking and compilation.

Select a flavor per file with its extension or a leading
`// osprey: flavor=ml` marker. The compiler also accepts `--flavor ml` for a
single-file build. Multi-file cross-flavor imports remain under development.

Powered by a Rust language server (`osprey lsp`, built on
[lspkit](https://github.com/Nimblesite/lspkit)) that runs the compiler front-end
in-process — the same engine targeted at Neovim and Zed next.

## Features

- **Syntax highlighting** — keywords, types, string interpolation
  (`"Hello ${name}!"`), operators, and comments. Default (`.osp`) is fully
  supported; ML (`.ospml`) support follows the compiler's current ML parser.
- **Live diagnostics** — errors and warnings from the Osprey compiler as you
  type, inline in the editor.
- **Hover, go-to-definition, find-references, document symbols, signature help,
  and completion** — driven by the compiler's own parser and type checker.
- **Compile & run** from the editor:
  - `Osprey: Compile Osprey File` (`Ctrl/Cmd+Shift+B`)
  - `Osprey: Compile and Run Osprey File` (`F5`)
- **Test Explorer** — `*.test.osp` / `*.test.ospml` files and their cases appear
  in the Testing view, with three run profiles: **Run**, **Coverage**, and
  **Profile** (runs the suite under the sampling CPU profiler and opens its
  flame graph plus inline heat annotations).
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
- **Bracket matching, auto-closing, and comment toggling.**

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

## Settings

| Setting                      | Default | Description                                                                                                                                       |
| ---------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `osprey.server.enabled`      | `true`  | Enable/disable the language server.                                                                                                               |
| `osprey.diagnostics.enabled` | `true`  | Enable/disable inline diagnostics.                                                                                                                |
| `osprey.server.compilerPath` | `""`    | Path to an Osprey compiler. **Leave empty** to use the version-matched compiler bundled with this extension (falling back to `osprey` on `PATH`). |

## Links

- Website & docs: <https://ospreylang.dev>
- Source & issues: <https://github.com/Nimblesite/osprey>

## License

See [LICENSE](LICENSE).
