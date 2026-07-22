<p align="center">
  <img src="website/src/assets/images/logo.png" alt="Osprey logo" width="160" />
</p>

<h1 align="center">Osprey Programming Language</h1>

<p align="center">
  <strong>One core. Two surfaces. Zero compromise.</strong><br/>
  One Hindley-Milner type checker, one effect system, one runtime, one LLVM/wasm
  backend — fronted by two first-class syntaxes. An <strong>accessible</strong>
  brace-style surface that reads like the mainstream languages you already know,
  and an <strong>uncompromising</strong> layout-based ML surface for the FP
  devotee.<br/>Written in Rust, outputs to LLVM.
</p>

⭐ **[Star us on GitHub](https://github.com/Nimblesite/osprey)** to support the project and allow us to submit to Homebrew! ⭐

## Two Flavors, One Core

Osprey is **one language** with **two first-class, permanent syntaxes** called
flavors. Neither is the watered-down one — each goes all the way in its own
direction.

- **Default flavor (`.osp`)** — the **accessible** surface. C-style braces,
  `fn`, `f(x: a, y: b)` calls with named arguments, `if`/`else if`/`else`, the
  `? :` ternary. It deliberately borrows the shapes of Kotlin, Swift, Go, Dart,
  C#, and Java, so a mainstream developer can read a `.osp` file cold.
  **Fully implemented today** (specs 0001–0022).
- **ML flavor (`.ospml`)** — the **uncompromising** surface. Offside-rule layout
  (indentation, no braces), curry-by-default, whitespace application `f a b`,
  `\x => e` lambdas, `:=` mutation, `->` for types and `=>` for clauses. The
  most elegant constructs of the ML family, all the way — no C-isms, no
  concessions. **In active development**, with runnable proof in
  [`examples/tested/ml/`](examples/tested/ml/).

**No compromise — pick your tribe.** ML is not "braces optional"; Default is not
deprecated or transitional. Mainstream developers get a surface that reads like
home; FP folks get real layout and real currying. Nobody is asked to accept the
other camp's spelling — pick your flavor and go all in.

### The same program, both flavors

Both surfaces lower to the **same canonical AST** before any type checking. The
currying twin below is **machine-checked equal**:

```osprey
// Default flavor (.osp):
fn add(x) = fn(y) => x + y
```

```osprey
// ML flavor (.ospml) — identical canonical AST:
add x y = x + y
```

After lowering, nothing — type checker, effect checker, optimiser, codegen — can
tell which flavor you wrote. Same safety, same effects, same performance.

### Same folder, compiled together

Because every flavor lowers to the same canonical AST, the architecture is
**designed** so that files of different flavors live in **one project folder** and
compile into **one program**. Pick the flavor **per file**; the team is never
forced to pick one tribe. Exports are canonical signatures with stable names and
order, so by design a Default module and an ML module can import each other.

```text
project/
  math.ospml     # ML flavor — curry-by-default module
  app.osp        # Default flavor — braces; imports math
```

**Flavor selection (shipping today):** select the ML surface with the `.ospml`
extension, the `--flavor ml` CLI flag, or a leading `// osprey: flavor=ml`
marker. Precedence: flag > marker > extension > Default. One flavor per file;
mixed flavors per project via imports. Multi-file cross-flavor imports are the
design direction; per-file flavor selection is implemented and green today.

## Installation

```bash
# macOS / Linux (Homebrew)
brew install nimblesite/tap/osprey

# Windows (Scoop)
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

Osprey shells out to LLVM (`llc`) and a C compiler at compile time; the package
managers pull those in as dependencies (`llvm` for brew; `llvm` + `gcc` for scoop).

The [VS Code extension](https://marketplace.visualstudio.com/items?itemName=nimblesite.osprey)
(`nimblesite.osprey`) bundles a version-matched compiler and a Rust language
server (`osprey lsp`, built on [lspkit](https://github.com/Nimblesite/lspkit)) for
live diagnostics, hover, go-to-definition, and completion. The same server is
editor-agnostic — Neovim and Zed are on the roadmap. See
[Language Server & Editors](docs/specs/0020-LanguageServerAndEditors.md).

## Language Features

- **Functional-first**: Immutable data, pattern matching, pipe operators
- **Algebraic Effects**: Typed operations, lexical handlers, and native single-shot continuations
- **Type-safe**: Algebraic data types with variant types
- **HTTP-native**: Built-in server/client with streaming support
- **Fiber concurrency**: Lightweight isolated execution contexts
- **Zero-cost abstractions**: Compiles to efficient LLVM IR
- **Runs in the browser**: Compiles to WebAssembly (`--target=wasm32`) — see [Compiling to WebAssembly](#compiling-to-webassembly)

## Effect Safety Status

Effect operation arguments and results are checked statically. Complete effect-row propagation and missing-handler rejection are still in progress; today, a missing runtime handler exits with an explicit `unhandled effect` diagnostic.

## Syntax Example

Algebraic effects in the **Default flavor** (fully implemented). The same code
runs under different handlers — production writes to stdout, the test handler
stays silent:

```osprey
effect Logger {
    log: fn(string) -> Unit
}

fn greet(name: string) -> Unit !Logger =
  perform Logger.log("Hello, ${name}!")

// Production: write to stdout
handle Logger
  log msg => print(msg)
in greet("Alice")

// Test: stay silent — same code, new handler
handle Logger
  log msg => 0
in greet("Bob")
```

A taste of the **ML flavor** (`.ospml`) — curry-by-default and layout-based
`match`, runnable today (see [`examples/tested/ml/`](examples/tested/ml/)):

```text
adder : int -> int -> int
adder a b = a + b

// partial application falls straight out of currying:
addTen = adder 10
answer = addTen 32        // 42

classify n =
    match n
        0 => "zero"
        1 => "one"
        _ => "many"
```

## Project Structure

- `crates/` - Main Osprey compiler (Rust workspace: osprey-ast, osprey-syntax, osprey-types, osprey-codegen, osprey-runtime-sys, osprey-cli)
- `tree-sitter-osprey/` - Tree-sitter grammar (parser)
- `compiler/` - Pure-C runtime sources (`runtime/`)
- `examples/` - Example programs + golden test suites (`tested/`, `failscompilation/`, `tui/`, `wasm/`)
- `vscode-extension/` - VSCode language support
- `website/` - Documentation site
- `webcompiler/` - Browser-based compiler
- `homebrew-package/` - Homebrew tap
- `.devcontainer` - Configuration for the dev container

## Documentation

- [Language specification](docs/specs/)
- [API reference](website/src/docs/)
- [Contributing guide](CONTRIBUTING.md)
- [Release process](docs/RELEASING.md) — tag `v*` to release; CI runs only on PRs to `main`.

## Development

Built on proven tech: Rust for the compiler, tree-sitter for parsing, and LLVM for code generation.

**AI-Assisted Development**: Claude Sonnet 4 with Cursor makes implementing language features accessible. Check out [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow.

**Use VS Code Dev Containers** - strongly recommended. Open in VS Code and hit "Reopen in Container".

```bash
make build         # C runtime archives + cargo build --release + extension
make test          # Run all tests + coverage thresholds
make lint          # cargo clippy + extension lint
make ci            # lint + test + build (full CI simulation)
make install       # Install compiler + runtime archives locally
```

The compiler binary lands at `target/release/osprey`.

## Compiling to WebAssembly

Osprey compiles to `wasm32-wasip1` and runs in the browser, under `wasmtime`, or
under Node's built-in WASI. See [`docs/specs/0022-WebAssemblyTarget.md`](docs/specs/0022-WebAssemblyTarget.md)
for the design and [`examples/wasm/`](examples/wasm/) for a full example.

**Toolchain** (one-time): `clang` (any recent LLVM has the wasm32 backend),
`wasm-ld`, and a WASI sysroot.

```bash
brew install lld wasi-libc          # macOS (wasm-ld + WASI sysroot)
sudo apt-get install -y lld         # Linux: wasm-ld; sysroot via the wasi-sdk
```

**Build the wasm runtime + an example, then run it:**

```bash
# One target builds everything ready to go: the wasm runtime archive, the
# compiled example, validation, and smoke-runs under Node's WASI + the browser shim.
make wasm

# or drive the compiler directly:
osprey examples/wasm/hello.osp --target=wasm32 --compile -o hello.wasm
wasmtime hello.wasm                 # run under a standalone WASI runtime
osprey examples/wasm/hello.osp --target=wasm32 --run     # compile + run (uses wasmtime)
node scripts/wasm-smoke.mjs hello.wasm                   # run under Node's WASI
```

**In the browser** — `examples/wasm/index.html` ships a tiny shared WASI shim
(`wasi-shim.mjs`; no bundler, no npm). Serve the directory and open it — it runs
on load; from the devtools console, `await osprey.run()` re-runs it and prints to
the page:

```bash
cd examples/wasm && python3 -m http.server 8080   # then open http://localhost:8080/
```

The portable core (allocator, strings, lists, maps, JSON, effects) runs on wasm.
Fibers/`spawn`, HTTP/WebSocket, FFI/SQLite, processes, file I/O and `random` are
not ported — a program using them fails at link with a clear `undefined symbol`.

## Status

🚧 **Alpha**: Core language features are implemented, including typed algebraic-effect operations and lexical handlers. Static effect coverage, first-class handler values, and some advanced features remain in development.

See [docs/specs/](docs/specs/) for implementation status.

## Recent Major Updates

- **Algebraic Effects System**: Typed operations, lexical handlers, and native explicit `resume`
- **Effect Declarations**: `effect` keyword for defining effect operations
- **Perform Expressions**: `perform` keyword for effect operations
- **Handler Expressions**: `handle...in` syntax for effect handling
- **Current Safety Boundary**: operation signatures are checked statically; missing-handler coverage currently has a runtime guard

## License

MIT License - see [LICENSE](LICENSE)

---

⭐ **[Give us a star on GitHub](https://github.com/Nimblesite/osprey)** if you like what we're building! ⭐ 
