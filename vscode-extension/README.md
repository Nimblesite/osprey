<p align="center">
  <img src="https://ospreylang.dev/assets/images/logo.png" alt="Osprey logo" width="96" />
</p>

# Osprey for VS Code

**Safe, elegant, fast.** Osprey is a functional language built for safety, elegance and performance: algebraic effects and `Result` instead of panics, inferred types that say more with less text, and native LLVM compilation.

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/vscode-extension/media/language-tour.gif" alt="Osprey tour: a function requests storage through an effect, a disk handler saves the draft, an offline handler simulates failure, the same code in Python flavor, and a GPT-2 port whose Tensor handler chooses CPU or GPU" width="960" />
</p>

One function requests storage. One handler writes the file; another simulates being offline. The function stays the same. [Try Osprey in your browser](https://ospreylang.dev/playground/) or read the [effects guide](https://ospreylang.dev/docs/algebraic-effects/).

> **Actively evolving.** Osprey is being used to build real applications. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.

## The language

- **Safety: fewer bugs and failures.** No panics: expected failures are `Result` values the caller must handle. Data models list every case and matching is exhaustive. Integer arithmetic follows an explicit `Arith` policy instead of silently wrapping. Effects make outside work visible and replaceable. Calling C crosses the memory-safety boundary, and GPU device-kernel faults currently abort.
- **Elegance: more program for less text.** Inferred types, effects instead of plumbing, and direct concurrent code without a separate async form. Fewer tokens for people to hold in their heads and for AI agents to read, write and review.
- **Performance: native from the start.** LLVM native binaries with no VM or JIT warm-up, static handler selection that removes runtime effect dispatch, lightweight fibers, and a memory backend chosen per build.

### Algebraic effects

Declare a need with `effect`, request it with `perform`, and choose its implementation with a callable `handler` or a block-scoped `handle`. Swap a disk handler for an offline one in tests; the function never changes.

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

Output: `Saved`. A callable handler runs the zero-argument function passed to it, so the work waits until the handler is active.

### Three syntax flavors. One language.

A **syntax flavor** is a way to write Osprey: Default braces (`.osp`), ML indentation (`.ospml`) or Python-style syntax (`.py` starting with `# osprey: flavor=python`). Each keeps its idioms while sharing Osprey's types, effects and compiler. Choose per file and import across flavors in one project.

```ocaml
effect Drafts
    save : (string, string) => Result<int, Error>
saveDraft path text = match perform Drafts.save path text
    Success _ => "Saved"
    Error message => "Retry: ${message}"
onSave () = saveDraft "draft.txt" "Ship it."

disk = handler Drafts
    save path text => writeFile path text

print (disk onSave)
```

```python
# osprey: flavor=python
@effect
class Drafts:
    def save(path: str, text: str) -> Result[int, Error]: ...

def saveDraft(path, text):
    match perform(Drafts.save(path, text)):
        case Success():
            return "Saved"
        case Error(message=message):
            return f"Retry: {message}"

def onSave():
    return saveDraft("draft.txt", "Ship it.")

@handler(Drafts)
class disk:
    def save(path, text):
        return writeFile(path, text)

def main():
    print(disk(onSave))
```

### And more

- **Types:** inference removes repeated annotations; exhaustive matching and `Result` keep cases and failures visible.
- **Deployment:** LLVM native programs, WebAssembly, and C ABI libraries for native mobile hosts.
- **Concurrency and memory:** isolated fibers, tracing GC by default, and reference counting (ARC) where supported.
- **Python to GPU:** port computation to Python flavor and keep `def`, `lambda` and f-strings. The [GPT-2 port](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2) has one `Tensor` effect with CPU and GPU handlers; the handler decides where the arithmetic runs.

## In the editor

The extension registers Default (`.osp`) and ML (`.ospml`) documents for its language server, commands, debugger and Test Explorer. Use the CLI for Python-flavor files while dedicated editor coverage remains unfinished; see [plan 0031](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0031-python-flavor.md). Select a flavor per file using its extension or a leading `// osprey: flavor=ml` marker. The Rust language server (`osprey lsp`, built on [lspkit](https://github.com/Nimblesite/lspkit)) runs the compiler frontend in-process.

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
- **Hunt for bugs** — `Osprey: Hunt for Bugs` runs KLEE, prints the report in the Osprey Hunt output channel and places findings at source locations. [Hunt prerequisites and limits](https://ospreylang.dev/docs/hunt/) apply.
- **CPU profiling** — run the current file or attach to a supported native process, then inspect samples in the shared profiler UI. Memory profiling and a Windows sampler remain unfinished.
- **Bracket matching, auto-closing, and comment toggling.**

## Packages

See the [screenshot walkthrough](https://www.ospreylang.dev/docs/packages-in-vscode/) for setup, release warnings and the separate fetch/build steps.

Run **Osprey: Add Package** from the Command Palette, search the registry by the complete `@org/package` name, and confirm the selection. **Osprey: Remove Package** lists the project's direct dependencies and removes the selected dependency through the same bundled client. Both operations update the manifest and relock through Osprey; unsaved manifest or lock editors block changes. Multi-folder workspaces prompt for the project first.

Set `osprey.packages.registry` to your registry API origin. HTTPS is required; loopback HTTP supports the local folder registry (`make registry-serve`, default `http://127.0.0.1:8787`). Folder-registry listings provide no audit evidence, so the picker labels releases unverified and requires confirmation. Hosted listings without security eligibility and advisory evidence cannot offer an installation bypass. Hosted authentication and public trust enforcement remain separate delivery work.

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

Debugging also requires `lldb-dap`. Set `osprey.debug.lldbDapPath` when it is outside `PATH`. Integrated and external terminal consoles require LLDB-DAP 21 or newer; the default Debug Console has no stdin. See the [debugger contract](https://github.com/Nimblesite/osprey/blob/main/docs/specs/tooling/0021-Debugger.md).

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

See [LICENSE](https://github.com/Nimblesite/osprey/blob/main/vscode-extension/LICENSE).
