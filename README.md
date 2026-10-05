<p align="center">
  <img src="website/src/assets/images/logo.png" alt="Osprey logo" width="160" />
</p>

# Osprey Programming Language

Osprey is a functional language built around **algebraic effects**: application code requests an operation, and a handler supplies its implementation. Use the same logic with a real database, an in-memory test, or a different platform service.

Types and effect requirements are inferred. Osprey compiles through LLVM to native binaries, WebAssembly, and C ABI libraries for iOS and Android. It is alpha software.

## Choose the implementation where the work runs

```osprey
effect Log { write: fn(string) -> Unit }

fn placeOrder() = {
    perform Log.write("Order accepted")
    "accepted"
}

let console = handler Log { write message => print(message) }
mut captured = ""
let recording = handler Log { write message => { captured = message } }

let live = console(placeOrder)
let tested = recording(placeOrder)
print("${live}, ${tested}; recorded: ${captured}")
```

`placeOrder` asks to log without taking a logger parameter. `console` and `recording` are ordinary callable handler values. Each takes the work to run so its implementation is installed **before** the work performs an operation. The compiler rejects a call whose required operations have no handler.

The same example in ML flavor:

```osprey-ml
effect Log
    write : string => Unit

placeOrder () =
    perform Log.write "Order accepted"
    "accepted"

console = handler Log
    write message => print message
mut captured = ""
recording = handler Log
    write message => captured := message

live = console placeOrder
tested = recording placeOrder
print "${live}, ${tested}; recorded: ${captured}"
```

Use `handle E { ... }` inside a block to handle the rest of that block. Value operations return to their caller; operations declared `control` can resume or stop the remaining computation. `handle static E` removes effect dispatch during compilation. See the [effects guide](website/src/docs/algebraic-effects.md), [runnable comparisons](examples/handlers/README.md), and [implementation status](docs/plans/0016-algebraic-effects-and-handlers.md).

## Applications using effects

| Application | What the handlers provide |
| --- | --- |
| [Talon Bank: web, iOS, and Android](examples/projects/modules/README.md) | SQLite storage, audit logging, request metrics, and platform rendering handlers around shared screens and behavior |
| [Issue Inbox for iOS and Android](examples/mobile/README.md) | SQL and HTTP command descriptions for native hosts, with replaceable test implementations |
| [iPhone counter](examples/ios/README.md) | A Swift logging callback behind an Osprey effect |

Issue Inbox shares its state transitions, screen tree, GitHub decoding, SQLite statements, search, bookmarks, and notes across both platforms. SwiftUI and Android hosts render the screen and execute platform commands.

<p align="center">
  <img src="website/src/assets/images/mobile/issue-inbox-ios.png" alt="Issue Inbox with live GitHub issues on an iPhone simulator" width="280" />
  <img src="website/src/assets/images/mobile/issue-inbox-android.png" alt="Issue Inbox restored from SQLite on an Android emulator" width="280" />
</p>

## Language features

- **Algebraic effects:** reusable handlers, inferred requirements, explicit control operations, and static handler selection.
- **Inferred types:** algebraic data types and exhaustive pattern matching; expected failures use `Result`.
- **Two syntax flavors:** braces and `fn` in `.osp`, or layout and currying in `.ospml`. Project modules can mix them.
- **Isolated fibers:** typed message passing without a separate `async fn` kind.
- **Memory choices:** the default non-reclaiming allocator, tracing GC (`--memory=gc`), or Perceus reference counting (`--memory=arc`) on native builds.

Integer arithmetic returns plain values and requests the `Arith` effect on overflow or a zero divisor. The application chooses a policy; an unhandled fault is a compile error. See [arithmetic effects](docs/specs/0037-ArithmeticEffects.md).

Dynamic control operations currently require the native target. WebAssembly and mobile support value handlers and static discharge, but reject dynamic resumption. Generalized effect rows, reusable continuations, and other remaining work are tracked in [plan 0016](docs/plans/0016-algebraic-effects-and-handlers.md).

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
osprey examples/mobile/inbox --target=ios --compile -o libInbox.a
osprey examples/mobile/inbox --target=android-arm64 --compile -o libInbox.a
```

The WebAssembly target supports a portable runtime subset, with file access supplied by the WASI host and a defined browser bridge. Fibers, built-in HTTP/WebSocket operations, processes, general C FFI, and resumable effects are rejected. See the [WebAssembly specification](docs/specs/0022-WebAssemblyTarget.md) and [`examples/wasm/`](examples/wasm/).

The [iOS](docs/specs/0038-iOSTarget.md) and [Android](docs/specs/0039-AndroidTarget.md) targets emit an archive and C header for the platform application to link. They require their matching SDK/NDK and runtime builds. The mobile example includes these build steps. Mobile C ABI builds currently support the default allocator, which retains general allocations for the process lifetime.

## Documentation

- [Algebraic effects guide](website/src/docs/algebraic-effects.md)
- [Language and engineering specifications](docs/specs/)
- [Website documentation](website/src/docs/)
- [VS Code extension](vscode-extension/README.md)
- [Contributing guide](CONTRIBUTING.md)
- [Release process](docs/RELEASING.md)

The specifications define intended behavior. Individual chapters identify
implementation gaps. The [feature status page](website/src/status.md) summarizes
implementation limits.
