---
layout: page
permalink: /docs/installation/
title: Installing Osprey
description: Install the Osprey compiler and its required LLVM/clang toolchain on macOS, Linux, and Windows. Step-by-step prerequisites, verification, and troubleshooting.
tags:
- installation
- llvm
- clang
- getting-started
---

# Installing Osprey

*By Christian Findlay · Updated 25 October 2026*

Osprey compiles your program to **textual LLVM IR** and then hands that IR to
**`clang`** to produce and link a native executable. The `osprey` binary does
**not** bundle LLVM — it shells out to a `clang` already on your system. So
installing Osprey is two steps:

1. Install the **LLVM/clang toolchain** (the prerequisite).
2. Install the **`osprey` compiler**.

> **This is required.** The release tarballs and the Homebrew/Scoop packages ship
> only the `osprey` binary and its C runtime archives — never LLVM itself. If
> `clang` is not on your `PATH`, compiling any program fails with an error like
> `clang: command not found` or `failed to spawn C compiler`. Install the
> toolchain first.

## What Osprey needs

| Task | Required tools |
|------|----------------|
| Compile & run native programs | `clang` (a full LLVM clang, **15 or newer**) |
| Compile & link on Windows | A MinGW-compatible `clang` and MinGW runtime libraries; Scoop installs `llvm` and `gcc` |
| Native HTTP or WebSocket programs | OpenSSL libraries (`libssl` and `libcrypto`) available to the linker |
| Compile to WebAssembly (`--target=wasm32`) | `clang`, `wasm-ld` (from `lld`), a WASI sysroot |
| Debug info (`--debug`, DWARF line tables) | LLVM **19 or newer** recommended |

Any recent LLVM release works for everyday compilation — there is no exact
version to match. When in doubt, install your platform's current LLVM.

Native HTTP and WebSocket programs additionally link OpenSSL, even when they use plain HTTP or `ws://`. On macOS, install it with `brew install openssl@3`; the compiler checks the standard Homebrew library paths. On Debian/Ubuntu, use `sudo apt-get install libssl-dev`; on Fedora/RHEL, `sudo dnf install openssl-devel`; on Arch, `sudo pacman -S --needed openssl`. Windows needs OpenSSL libraries compatible with the MinGW toolchain and discoverable by its linker; the Scoop dependencies listed below do not install them. Programs without HTTP or WebSocket calls do not require OpenSSL.

## macOS

The quickest path uses Apple's bundled clang from the Xcode Command Line Tools,
which is enough to compile and run native Osprey programs:

```bash
xcode-select --install
```

For a **current, full LLVM** (recommended for WebAssembly, debug info, or if you
hit an IR-compatibility error with Apple's older clang), install it with
Homebrew:

```bash
brew install llvm
```

Homebrew's `llvm` is **keg-only** — its `clang` is deliberately kept off your
`PATH` so it does not shadow Apple's. Point Osprey at it in one of two ways:

```bash
# Option A — put Homebrew LLVM ahead of Apple's clang for this shell:
export PATH="$(brew --prefix llvm)/bin:$PATH"

# Option B — leave PATH alone; tell Osprey which compiler to use per-invocation:
OSPREY_CC="$(brew --prefix llvm)/bin/clang" osprey app.osp --run
```

Then install the compiler:

```bash
brew install nimblesite/tap/osprey
```

## Linux

Install `clang`, `llvm`, and (for WebAssembly) `lld` from your distribution:

```bash
# Debian / Ubuntu
sudo apt-get update && sudo apt-get install -y clang llvm lld

# Fedora / RHEL
sudo dnf install -y clang llvm lld

# Arch
sudo pacman -S --needed clang llvm lld
```

Osprey is distributed for Linux via the Homebrew tap (or the release tarball):

```bash
brew install nimblesite/tap/osprey
```

## Windows

Osprey uses `clang` to compile and link. The Windows release includes a MinGW-built C runtime, so the selected clang must link compatible MinGW libraries, including winpthreads. Scoop installs `llvm` and `gcc` as dependencies of `osprey`:

```powershell
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

If you install Osprey another way (e.g. the release tarball), install the
toolchain yourself:

```powershell
scoop install llvm gcc
```

Alternatively, install the official [LLVM release](https://github.com/llvm/llvm-project/releases)
and a MinGW-w64 toolchain, and make sure both `clang.exe` and `gcc.exe` are on
your `PATH`.

## Verify the toolchain

After installing, confirm `clang` resolves and is a full LLVM clang:

```bash
clang --version
```

Then compile and run a program:

```bash
echo 'fn main() = print("Osprey is installed")' > hello.osp
osprey hello.osp --run
```

You should see `Osprey is installed`.

## Debugging in VS Code

Install the Osprey extension and LLDB, including its `lldb-dap` executable. Open an `.osp` or `.ospml` file, set a breakpoint and press **F5**. The extension builds a native debug executable and launches LLDB-DAP. If the adapter is outside your `PATH`, set `osprey.debug.lldbDapPath` to its full path in VS Code settings.

Native debugging exposes primitive parameters, initialized locals, captures, shared mutable cells and expandable records, including inside handlers and host-executed GPU kernels. Collections, unions, Results and closures still need dedicated displays; optimized-away value reporting remains unfinished. See the [debugger contract](/spec/0021-debugger/) for scope and source-mapping details.

To build a debug executable from the command line:

```bash
osprey hello.osp --debug --debug-opt=none --compile --debug-out hello-debug --debug-preserve-ir
```

This keeps the executable and its generated IR (`hello-debug.ll`) for inspection. Add `"preserveArtifacts": true` to a VS Code launch configuration to keep the IR from an editor build, or set `"compilerPath"` to select a compiler for that launch. Editor launches always build without optimization. Optimized debugging and heap inspection are still planned; unsupported CLI modes report an error.

For programs that read input, add `"console": "integratedTerminal"` to the launch configuration. `"externalTerminal"` uses VS Code’s configured external terminal. Both terminal modes require LLDB-DAP 21 or newer; an older or unrecognised adapter produces a clear error. The default `"internalConsole"` displays output in the Debug Console and does not provide stdin.

## The `OSPREY_CC` override

When several clangs coexist — for example a keg-only Homebrew LLVM, or the MinGW
clang on Windows that must match the MinGW-built runtime archive — set
`OSPREY_CC` to the exact driver Osprey should invoke instead of the default
`clang`:

```bash
OSPREY_CC=/opt/homebrew/opt/llvm/bin/clang osprey app.osp --run
```

## Why native builds default to GC

Tracing GC avoids updating reference counts each time a managed value gains or loses an owner: ordinary retain/release calls do no work, and collection reclaims unreachable values later. That is Osprey's default tradeoff over ARC's per-ownership-change bookkeeping. It does not promise lower execution time or memory use for every program.

Use `--memory=arc` when prompt reclamation or avoiding tracing pauses matters. Both modes manage memory automatically without ownership annotations. Measure your application's time and peak memory before choosing. Files and sockets still need their explicit cleanup APIs under either mode.

The current GC supports Apple and glibc-Linux. On Windows, select ARC. GC also stops collecting after another allocator thread appears; use ARC when allocating fibers need continued reclamation. WebAssembly, mobile and Python-library targets default to ARC and reject tracing GC. Linking requires the managed runtime for the selected target; runtime archive availability depends on the build. See the [memory contract](/spec/0018-memorymanagement/#backend-selection-mem-backends) for details.

## Troubleshooting

- **`library not found for -lssl` / `cannot find -lssl` or `-lcrypto`** — install the OpenSSL libraries listed above for HTTP/WebSocket programs. On Windows, match the libraries to the MinGW toolchain.
- **`clang: command not found` / `failed to spawn C compiler`** — LLVM is not
  installed, or its `clang` is not on your `PATH`. Install it (above) or point
  `OSPREY_CC` at a working `clang`.
- **Homebrew says `osprey` is installed but compiling fails** — Homebrew's
  `llvm` is keg-only. Add `$(brew --prefix llvm)/bin` to your `PATH`, use
  `OSPREY_CC`, or install the Xcode Command Line Tools for Apple's clang.
- **An IR / bitcode version error when compiling** — your `clang` is older than
  the IR Osprey emits. Install a current LLVM and select it with `PATH` or
  `OSPREY_CC`.
- **WebAssembly build can't find `wasm-ld`** — install `lld` (`brew install lld`
  on macOS, your distro's `lld` package on Linux). See the
  [Web Apps guide](/docs/web-apps/) for the full Wasm toolchain.

## No install required

Use the [Playground](/playground/) without installing a local toolchain. It sends Default, ML or Python-flavor source to the hosted compiler service and displays the result; compilation and execution happen on the server.
