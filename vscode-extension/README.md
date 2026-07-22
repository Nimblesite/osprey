<p align="center">
  <img src="https://raw.githubusercontent.com/Nimblesite/osprey/main/website/src/assets/images/logo.png" alt="Osprey logo" width="128" />
</p>

# Osprey for VS Code

> **Preview.** Osprey is pre-production and evolving fast. Expect rough edges.

Language support for [Osprey](https://ospreylang.dev) — a functional programming
language with algebraic effects, fiber-based concurrency, pattern matching, and
strong compile-time safety.

**One core. Two surfaces. Zero compromise.** Osprey is one language — one
Hindley-Milner type checker, one effect system, one runtime, one standard
library, one LLVM/wasm backend — fronted by two first-class **flavors**:

- **Default flavor (`.osp`)** — the accessible surface: C-style braces, `fn`,
  `f(x: a, y: b)` calls with named arguments, `if`/`else if`/`else`. Borrows
  the shapes of Kotlin, Swift, Go, Dart, C#, and Java so it reads like home.
  **Fully implemented today.**
- **ML flavor (`.ospml`)** — the uncompromising surface: offside-rule layout
  (indentation, no braces), curry-by-default, whitespace application `f a b`,
  `\x => e` lambdas, `:=` mutation. The best of the ML family, all the way —
  no C-isms, no concessions. **In active development.**

Neither flavor is the watered-down one: the surface goes all the way in your
direction. Mainstream developers get a surface that reads like the languages
they already know; FP devotees get real layout and real currying. Pick your
tribe and go all in — nobody is forced into the other camp's spelling.

Select the flavor _per file_ (the `.ospml` extension, a leading
`// osprey: flavor=ml` marker, or the `--flavor ml` CLI flag — all shipping
today). Because both flavors lower to the same canonical AST before any type
checking, the design lets a `.osp` file and a `.ospml` file live in one folder
and compile into a single program, sharing one type checker, one effect system,
and one binary.

Powered by a Rust language server (`osprey lsp`, built on
[lspkit](https://github.com/Nimblesite/lspkit)) that runs the compiler front-end
in-process — the same engine targeted at Neovim and Zed next.

## Features

- **Syntax highlighting** — keywords, types, string interpolation
  (`"Hello ${name}!"`), operators, and comments. Default (`.osp`) is fully
  supported today; ML (`.ospml`) support is rolling out alongside the flavor.
- **Live diagnostics** — errors and warnings from the Osprey compiler as you
  type, inline in the editor (full for Default `.osp`; ML `.ospml` diagnostics
  track the in-development ML front-end).
- **Hover, go-to-definition, find-references, document symbols, signature help,
  and completion** — driven by the compiler's own parser and type checker.
- **Compile & run** from the editor:
  - `Osprey: Compile Osprey File` (`Ctrl/Cmd+Shift+B`)
  - `Osprey: Compile and Run Osprey File` (`F5`)
- **Bracket matching, auto-closing, and comment toggling.**

## Requirements

The extension bundles a version-matched Osprey compiler for your platform and
verifies it at startup, so syntax checking works out of the box.

To **compile and run** programs, Osprey invokes LLVM and a C toolchain, so install:

- **LLVM** (provides `llc`) — `brew install llvm` / `scoop install llvm`
- A C compiler — `clang` (macOS/Linux) or MinGW `gcc` (`scoop install gcc`)

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
