<p align="center">
  <img src="https://www.ospreylang.dev/assets/images/logo.png" alt="Osprey logo" width="96" />
</p>

# Osprey for VS Code

Osprey is a functional language for apps, backends, ML and mathematics. **Safe, elegant, fast.** Nothing panics, types are inferred, and it compiles through LLVM to native code.

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/showcase.gif" alt="Apps written in Osprey: Prism Synth, the Casting Chromecast player and the Proof Workbench on macOS, then the signal-flow editor, Osprey Storage and Casting on Windows" width="960" />
</p>

Every app above is Osprey, running natively on macOS and Windows. [Try it in your browser](https://www.ospreylang.dev/playground/).

## The language

- **Safe.** Failures are `Result` values you must handle. Matches must cover every case. Integer overflow follows a policy you choose.
- **Elegant.** Types are inferred. Effects replace plumbing. Concurrent code needs no `async`.
- **Fast.** Native binaries, no VM, no JIT warm-up. Effect handlers are picked at compile time.

Declare what your code needs with `effect`. Supply it with `handle`. The app gets the real disk; the test gets a full one:

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

No type annotations. Drop the `Error` case or the handler and it won't compile. Write it with braces, ML indentation or the Python flavor; the [GPT-2 port](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/gpt2) is Python flavor, on CPU or GPU.

> Osprey is actively evolving. Try it on your projects and assess the risks before relying on it in production.

## Debug

Press `F5`. Breakpoints, stepping, the call stack and variables, on your Osprey source, through LLDB-DAP. Debug a single test from the Testing view and it stops inside that case. Fiber and effect-handler views are still to come.

## Profile

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/profiler.gif" alt="Run and Profile CPU on busy.osp: the live sample count runs in the status bar, the Osprey Profiler opens with samples, duration and a flame graph, and the source shows each function's share of the samples" width="960" />
</p>

**Run & Profile CPU** on the open file, or attach to a program that's already running. You get a flame graph, and every hot function shows its share of the time right in the source. [Watch the tutorial](https://www.ospreylang.dev/docs/tutorials/profiler/). Memory profiling and Windows sampling are still to come.

## Prove

<p align="center">
  <img src="https://media.githubusercontent.com/media/Nimblesite/osprey/main/src/toolchain/vscode-extension/media/workbench.gif" alt="The Proof Workbench inside VS Code: a checked proof that conjunction commutes, the goal pane listing hypotheses and what remains to show, a lesson answer marked Right, a step the kernel refuses with a type mismatch, and the rule reference" width="960" />
</p>

**Osprey: Open Workbench.** Write a proof step by step, with what you know and what's left to show beside it. Osprey's own kernel checks every step and tells you exactly why a wrong one fails. [Workbench guide](https://www.ospreylang.dev/docs/proof-workbench/).

## Also in the box

- **Language server:** errors across the whole repo, hover, go to definition, references, completion and formatting, from the compiler itself.
- **Tests:** run, cover, profile or debug any case from the Testing view. Skipped tests get flagged, never hidden.
- **Hunt for bugs:** KLEE searches for inputs that break your program and marks them in the source. [Details](https://www.ospreylang.dev/docs/hunt/).
- **Packages:** add and remove packages from the Command Palette. Until the hosted registry launches, they come from an unverified local one. [Walkthrough](https://www.ospreylang.dev/docs/packages-in-vscode/).

## Requirements

The compiler ships inside the extension. To build and run programs, install LLVM (`brew install llvm`, or `scoop install llvm gcc` on Windows); to debug, add `lldb-dap`. For `osprey` on your command line:

```bash
brew install nimblesite/tap/osprey                                                                # macOS / Linux
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket && scoop install osprey    # Windows
```

[Website and docs](https://www.ospreylang.dev) · [Source and issues](https://github.com/Nimblesite/osprey) · [License](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/vscode-extension/LICENSE)
