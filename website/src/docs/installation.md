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

*By Christian Findlay · Last verified 25 July 2026*

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
| Link the C runtime on Windows | `clang` **plus** MinGW `gcc` |
| Compile to WebAssembly (`--target=wasm32`) | `clang`, `wasm-ld` (from `lld`), a WASI sysroot |
| Debug info (`--debug`, DWARF line tables) | LLVM **19 or newer** recommended |

Any recent LLVM release works for everyday compilation — there is no exact
version to match. When in doubt, install your platform's current LLVM.

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

Osprey shells out to `clang` and links the C runtime with MinGW's `gcc`, so you
need **both** LLVM and MinGW. Scoop installs them as dependencies of the
`osprey` package:

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

## The `OSPREY_CC` override

When several clangs coexist — for example a keg-only Homebrew LLVM, or the MinGW
clang on Windows that must match the MinGW-built runtime archive — set
`OSPREY_CC` to the exact driver Osprey should invoke instead of the default
`clang`:

```bash
OSPREY_CC=/opt/homebrew/opt/llvm/bin/clang osprey app.osp --run
```

## Troubleshooting

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

To try Osprey without any toolchain, use the browser-based
[Playground](/playground/) — it compiles and runs Osprey entirely in your
browser, no LLVM needed.
