<p align="center">
  <img src="https://www.ospreylang.dev/assets/images/logo.png" alt="Osprey logo" width="96" />
</p>

# Osprey for VS Code

Osprey is a functional language for **apps, backends, ML and AI, and mathematics**, built on three pillars: **safety, elegance and performance**. Failures are `Result` values you must handle instead of panics, inferred types and algebraic effects say more with less text, and LLVM compiles it to native code. This extension brings the language into VS Code with a language server, a debugger, a CPU profiler, a test runner and the Proof Workbench.

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/showcase.gif" alt="Apps written in Osprey: Prism Synth, the Casting Chromecast player and the Proof Workbench on macOS, then the signal-flow editor, Osprey Storage and Casting on Windows" width="960" />
</p>

Real Osprey Native apps on macOS and Windows: a synth, a Chromecast player, the Proof Workbench, a signal-flow editor and a disk-space explorer. [Try Osprey in your browser](https://www.ospreylang.dev/playground/) or read the [effects guide](https://www.ospreylang.dev/docs/effects/).

> **Actively evolving.** Osprey is being used to build real applications. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.

## The language

- **Safety: fewer bugs.** No panics: expected failures are `Result` values, matching is exhaustive, integer arithmetic follows an explicit `Arith` policy, and effects make outside work visible and replaceable. Calling C crosses the memory-safety boundary.
- **Elegance: more program, less text.** Inferred types, effects instead of plumbing, and concurrency without a separate async form. Fewer tokens for people and AI agents to read.
- **Performance: native from the start.** LLVM native binaries with no VM or JIT warm-up, static handler selection, lightweight fibers and a memory backend chosen per build.

`effect` declares a need, `perform` requests it, and `handle` implements it. The app's edge picks the real disk; a test swaps in a full one without changing the app:

```osprey
effect Files { write: fn(string, string) -> Result<int, Error> }

fn install(name) = match perform Files.write("osprey.lock", "${name} = 2") {
    Success _ => "installed"
    Error { message } => "rolled back: ${message}"
}

let shipped = {
    handle Files { write path text => writeFile(path, text) }
    install("serde")
}
print(shipped)

test("a full disk rolls back", fn() => {
    handle Files { write _ _ => Error { message: "disk full" } }
    expect(install("serde"), "rolled back: disk full")
})
```

Types are inferred, the `match` must cover both `Success` and `Error`, and removing the handler is a compile error. Each **syntax flavor** writes the same language: Default braces (`.osp`), ML indentation (`.ospml`) or Python flavor (`.py` starting with `# osprey: flavor=python`), mixed freely in one project.

## Debug

Press `F5` on an `.osp` or `.ospml` file. The extension builds a native debug binary and runs it under LLDB-DAP, so breakpoints, stepping, the call stack, variables and pause work on your Osprey source. The Test Explorer's **Debug** profile stops on a breakpoint inside a `test(...)` body with that case's bindings live. Fiber, channel and effect-handler views are not built yet.

## Profile

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/profiler.gif" alt="Run and Profile CPU on busy.osp: the live sample count runs in the status bar, the Osprey Profiler opens with samples, duration and a flame graph, and the source shows each function's share of the samples" width="960" />
</p>

**Run & Profile CPU** samples the open file, or attach to an Osprey program that is already running from the **Processes** view. Results open as a flame graph, with each hot function's share of samples written beside its source. [Watch the profiler tutorial](https://www.ospreylang.dev/docs/tutorials/profiler/). Memory profiling and a Windows sampler are not built yet.

## Prove

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/workbench.gif" alt="The Proof Workbench inside VS Code: a checked proof that conjunction commutes, the goal pane listing hypotheses and what remains to show, a lesson answer marked Right, a step the kernel refuses with a type mismatch, and the rule reference" width="960" />
</p>

**Osprey: Open Workbench** opens the Proof Workbench in an editor tab. Write each step of an argument, see your hypotheses and what remains to show, and work through short lessons. A proof is Checked only when Osprey's own kernel accepts it; a wrong step is refused with the reason. See the [Workbench guide](https://www.ospreylang.dev/docs/proof-workbench/).

## Everything else

- **Language server:** diagnostics across the repository, hover, go-to-definition, references, symbols, signature help, completion and formatting, from the compiler's own parser and type checker. Python-flavor files use the CLI for now.
- **Run:** `Osprey: Compile Osprey File` (`Ctrl/Cmd+Shift+B`) and `Osprey: Compile and Run Osprey File`.
- **Tests:** `*.test.osp` and `*.test.ospml` cases appear in the Testing view with Run, Coverage, Profile and Debug profiles. A skipped case raises a warning you cannot miss, and `///` docs above a case show in the tree and on failure.
- **Hunt for bugs:** `Osprey: Hunt for Bugs` explores inputs with KLEE and places findings in the source. [Prerequisites and limits](https://www.ospreylang.dev/docs/hunt/).
- **Packages:** `Osprey: Add Package` and `Osprey: Remove Package` update `osprey.toml` and relock. Until the hosted registry is live, the extension runs a local registry of the repository's packages; its listings are unverified. [Walkthrough](https://www.ospreylang.dev/docs/packages-in-vscode/).

## Requirements

The extension bundles a version-matched compiler, so editing needs nothing else. Compiling and running need LLVM/clang (`brew install llvm`, or `scoop install llvm gcc` on Windows); debugging needs `lldb-dap` on `PATH` or in `osprey.debug.lldbDapPath`. To get `osprey` on your `PATH` too:

```bash
brew install nimblesite/tap/osprey                                                                # macOS / Linux
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket && scoop install osprey    # Windows
```

[Website and docs](https://www.ospreylang.dev) · [Source and issues](https://github.com/Nimblesite/osprey) · [License](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/vscode-extension/LICENSE)
