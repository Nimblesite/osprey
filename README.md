<p align="center">
  <img src="website/src/assets/images/logo.png" alt="Osprey logo" width="120" />
</p>

# Osprey

A functional language built on three pillars: **safety, elegance and performance**. It is algebraic with no panics, so failures are `Result` values you must handle; inferred types and **algebraic effects** express more program with less text; and it compiles through LLVM to native code. Write your application logic once; handlers connect it to storage, platform services and deterministic tests.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production. It compiles through LLVM to native binaries, WebAssembly and C ABI libraries for iOS, macOS and Android.

## Choose storage once, at the application boundary

The save callback calls application logic, which requests storage. Neither function takes a storage object:

```osprey
effect Drafts { save: fn(string, string) -> Result<int, Error> }
fn saveDraft(path, text) = match perform Drafts.save(path, text) {
    Success _ => "Saved"
    Error { message } => "Retry: ${message}"
}
fn onSave() = saveDraft("draft.txt", "Ship it.")

let disk = handler Drafts {
    save path text => writeFile(path, text)
}

print(disk(onSave))
```

```text
Saved
```

`disk(onSave)` supplies storage for the entire call chain: `onSave` → `saveDraft` → `Drafts.save`. The draft is written once. Intermediate functions need no storage parameters. A test can replace the handler's `writeFile(path, text)` with `Error { message: "offline" }` and exercise the same retry logic without touching disk. The compiler rejects the call if no handler supplies storage.

Read the [effects guide, including the ML version](https://www.ospreylang.dev/docs/effects/), or [try Osprey online](https://www.ospreylang.dev/playground/).

## Get started

Install LLVM/clang, then install Osprey:

```bash
# macOS or Linux, with Homebrew
brew install nimblesite/tap/osprey

# Windows, with Scoop
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

```bash
osprey program.osp --run
osprey program.osp --check
osprey program.osp --target=wasm32 --memory=none --compile -o program.wasm
```

See the [installation guide](https://www.ospreylang.dev/docs/installation/) for LLVM setup, supported platforms and verification.

## Find your way around

| Start here | What you will find |
| --- | --- |
| [examples/](examples/README.md) | Runnable programs, Talon Bank, Issue Inbox and GPT-2 |
| [crates/](crates) | Rust compiler, type checker, code generation, formatter, language server and debugger |
| [packages/](packages/README.md) | Package manager, registry server and Osprey libraries in every syntax flavor |
| [compiler/runtime/](compiler/runtime) | C runtime and GPU adapters |
| [tree-sitter-osprey/](tree-sitter-osprey) | Default-flavor grammar |
| [tests/](tests) | Default and ML language suites with shared golden output |
| [docs/](docs/README.md) | Specifications, delivery plans and contributor architecture |
| [website/](website) | Language guides, reference pages and public website |
| [vscode-extension/](vscode-extension/README.md), [webcompiler/](webcompiler) | Editor integration and online compiler |
| [scripts/](scripts/README.md) | Build rules, validation, platform tooling and packaging support |
| [Book/](Book/README.md), [benchmarks/](benchmarks/README.md) | The Osprey book and measured comparisons |

The [repository map](docs/contributing/repository-map.md) explains the compilation pipeline, build entry points and where a change belongs. The [contributing guide](CONTRIBUTING.md) covers setup and coordination.

## Build and verify

```bash
make setup
make build
make test
make ci
```

Run these from the repository root. The compiler is written to `target/release/osprey`. The root Makefile defines every public target; their configuration and private steps live in `scripts/make/`.

[Hunt](website/src/docs/hunt.md) explores symbolic inputs with KLEE: `make hunt FILE=tests/hunt/crash.osp HUNT_ARGS="--input-bytes 1 --max-time 10"` finds input `"9"` and reproduces the demo's deliberately planted C abort. Open the printed HTML report; exit status 1 is expected. The [package guide](website/src/docs/package-manager.md) covers the working folder registry, locked builds and planned package website; `make registry-test` runs its separate suites.

## Language and applications

- **Effects:** typed requests, reusable value handlers, native single-shot control operations and static interpretation. [Guide](website/src/docs/algebraic-effects.md).
- **Types:** inference, algebraic data types, exhaustive matching and explicit `Result` failures.
- **Any syntax, one language:** every flavor's concrete syntax tree lowers into one canonical AST, so types, effects and project modules are shared. Default braces (`.osp`), ML layout (`.ospml`) and the [Python flavor](https://www.ospreylang.dev/docs/python-flavor/) (`.py`, Osprey that looks like Python) ship today and mix in one project.
- **Arithmetic:** integer faults require an explicit `Arith` policy. [Contract](docs/specs/language/0037-ArithmeticEffects.md).
- **Native concurrency and memory:** isolated fibers; tracing GC by default, or reference counting with `--memory=arc`. Unmanaged allocation requires explicit `--memory=none`.

GC is the native default to avoid ARC's per-ownership-change reference-count work, at the cost of deferred reclamation and collection pauses. Choose ARC for prompt reclamation, Windows or allocating fibers; neither backend is universally faster. See [memory choices and limits](docs/specs/compiler/0018-MemoryManagement.md#backend-selection-mem-backends).

[Talon Bank](examples/projects/modules/README.md) shares web and mobile application logic; [Issue Inbox](examples/mobile/README.md) shares iOS and Android state, UI descriptions and platform commands. Native hosts execute mobile SQL and HTTP commands and return completion events. [GPT-2](examples/gpt2/README.md) uses two static tensor handlers for host lists and the GPU surface.

Dynamic control effects require native execution. WebAssembly and mobile support value handlers and static discharge; unsupported capabilities are rejected. Full scoped effect rows and reusable continuations remain unfinished. See [feature status](website/src/status.md), [specifications](docs/specs/README.md) and [active plans](docs/plans/README.md) for the precise limits.

MIT licensed. See [LICENSE](LICENSE) and the [release process](docs/RELEASING.md).
