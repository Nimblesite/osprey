<p align="center">
  <img src="website/src/assets/images/logo.png" alt="Osprey logo" width="160" />
</p>

# Osprey Programming Language

Osprey is a practical functional language for building safe, fast native
programs without the usual clutter. It combines strong inferred types,
first-class effects and lightweight concurrency with a choice of familiar
brace syntax or clean ML syntax.

Osprey compiles through LLVM to native binaries and can also target
WebAssembly. The project is in alpha and some specified features remain under
development.

## Language features

- **One language, two first-class flavors** — Default (`.osp`) uses braces,
  `fn` and familiar calls; ML (`.ospml`) uses layout, currying and whitespace
  application. Both lower to the same program representation before type
  checking and code generation.
- **Strong inferred types** — algebraic data types and pattern matching make
  valid states and expected failures explicit without requiring obvious type
  annotations everywhere.
- **First-class effects** — typed operations and lexical handlers separate a
  request for work from the code that performs it.
- **Isolated fiber concurrency** — lightweight tasks communicate through
  channels without a separate `async fn` kind.
- **Selectable memory management** — native builds support the default
  non-reclaiming allocator, tracing garbage collection (`--memory=gc`) and
  Perceus reference counting (`--memory=arc`).
- **Native and WebAssembly output** — native builds can call C through the FFI;
  C code remains outside Osprey's memory-safety guarantee.

Effect operation inputs and outputs are checked statically. The compiler does
not yet reject every missing handler or undeclared effect row, so an unhandled
effect can still produce a runtime diagnostic. Resuming effects are currently
native-only.

Each file selects its flavor by extension, a source marker or `--flavor` for a
single-file build. Multi-file cross-flavor imports remain a design direction;
do not rely on them as a complete feature.

## Example

Default flavor:

```osprey
type Lookup = Found { value: int } | Missing

fn doubleFound(result) = match result {
  Found { value } => Success(value * 2)
  Missing => Error("value not found")
}

match doubleFound(Found { value: 21 }) {
  Success(value) => print("result: ${value}")
  Error(message) => print("error: ${message}")
}
```

ML flavor:

```osprey-ml
adder : int -> int -> int
adder a b = a + b

addTen = adder 10
answer = addTen 32
```

Runnable examples live in [`examples/tested/`](examples/tested/).

## Installation

Osprey invokes `clang` to compile and link native programs. Install LLVM/clang
before installing the compiler.

```bash
# macOS
xcode-select --install
brew install nimblesite/tap/osprey

# Debian/Ubuntu; lld is also used for WebAssembly
sudo apt-get install -y clang llvm lld
brew install nimblesite/tap/osprey

# Windows
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

Homebrew's LLVM package is keg-only. If Osprey cannot find clang, add
`$(brew --prefix llvm)/bin` to `PATH` or set
`OSPREY_CC=$(brew --prefix llvm)/bin/clang`.

See the [installation guide](https://www.ospreylang.dev/docs/installation/) for
platform-specific verification and troubleshooting.

## Build and test

```bash
make build
make test
make lint
make ci
make install
```

The compiler binary is written to `target/release/osprey`.

```bash
osprey program.osp --check
osprey program.osp --compile -o program
osprey program.osp --run
osprey program.osp --target=wasm32 --compile -o program.wasm
```

The WebAssembly target supports the portable runtime subset. Fibers, HTTP,
WebSockets, the C FFI, processes and file I/O are not available on that target.
See the [WebAssembly specification](docs/specs/0022-WebAssemblyTarget.md) and
[`examples/wasm/`](examples/wasm/) for details.

## Documentation

- [Language and engineering specifications](docs/specs/)
- [Website documentation](website/src/docs/)
- [VS Code extension](vscode-extension/README.md)
- [Contributing guide](CONTRIBUTING.md)
- [Release process](docs/RELEASING.md)

The specifications define intended behavior. Individual chapters identify
implementation gaps. The [feature status page](website/src/status.md) summarizes
the most important current limits.
