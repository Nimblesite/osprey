---
layout: page.njk
title: Feature Status
description: Current implementation boundaries for Osprey
date: "git Last Modified"
tags: ["status", "features", "roadmap"]
author: "Christian Findlay"
---

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production. The compiler is written in Rust, emits LLVM IR and
builds native executables, `wasm32-wasip1` modules, and app-logic libraries for
iOS, macOS and Android. Each target has an explicit runtime boundary.

Current version: **{% if releases.latest %}{{ releases.latest.tag }}{% else %}development build{% endif %}**.

## Algebraic effects

Start with the [practical guide](/docs/effects/). The Default and ML flavors support:

- Callable `handler` values, captured handler factories, composition and rest-of-block `handle`.
- Typed value operations and explicit `control` operations; native control handlers can resume once or abandon the computation.
- `return` clauses that transform normal completion.
- `handle static` and static handler values that remove handled effect dispatch after source validation.
- Compile-time rejection of unhandled operations, including requests through helpers and callbacks.
- Integer overflow and zero divisors request the built-in `Arith` effect; each region chooses its policy.

Value handlers and static interpretation have native, WebAssembly and mobile paths. Dynamic control handlers require the native target. Open `!e` annotations have a closed-program prototype; independently quantified effect rows in function types, reusable continuations, owned escaping continuations, named instances, masking and finalizers remain unfinished. [Plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) records the evidence and remaining work.

The [banking](/docs/web-apps/) and [mobile](/docs/mobile-apps/) examples use these handlers at their application boundaries.

## Releases

Every published version is listed on the [Releases](/docs/releases/) page.

## Implemented foundations

- Default (`.osp`), ML (`.ospml`) and Python-flavor (`.py`, with the Osprey marker)
  source parsing, all lowering to the same AST before semantic analysis
- Hindley–Milner type inference, algebraic data types and exhaustive pattern
  matching for supported patterns
- User-defined generics, declaration-site variance, generic effects and explicit
  call-site type arguments in Default and ML.
- Immutable persistent lists and maps
- [Dense computation buffers](/spec/0034-gpucomputation/) with typed kernels and explicit arithmetic policies. Kernels and matrix products run on Metal or OpenCL when `OSPREY_GPU` selects a device, and on the CPU otherwise; buffers stay on the device between kernels. Known let-bound kernels retain their original captures when extracted.
- Isolated native fibers and channels; the current runtime uses one OS thread per fiber
- Native HTTP, WebSocket, file, process and C FFI runtime APIs
- Tracing GC by default and ARC as the alternative managed native backend
- Native and `wasm32-wasip1` compilation, iOS, macOS and Android C ABI targets, and [Python-hosted native libraries](/spec/0042-pythoninterop/) with scalar calls, exception results and opaque object handles
- A shared modular mobile application with reactive native UI, SQLite cache, live GitHub requests, and local notes and priorities
- Compiler-backed formatting, documentation generation, testing, profiling and
  language-server commands
- Mixed-flavor projects with live cross-file editor analysis, project layout warnings, visible state boundaries and compiler-checked import repairs
- Module signatures and opaque types. Opaque records, unions and manifest aliases are abstract outside their module, and importers are checked against a signature's declared types
- [HTML API documentation](/docs/documentation/) for public modules in Default
  and ML, with executable examples, Markdown guides, custom CSS, three themes,
  offline search and responsive navigation

The runnable programs in
[`tests/regressions/`](https://github.com/Nimblesite/osprey/tree/main/tests/regressions)
are compiled and compared with checked-in expected output.

## Target support

| Target | Output | Runtime boundary |
| --- | --- | --- |
| `native` | Executable for the host platform | Native runtime, including resumable effects; GC by default, ARC, or explicit unmanaged `none` |
| `python` | Shared library and C header on Linux/macOS | Loaded by CPython through `osprey-interop`; scalar ABI and opaque Python handles; no dynamic control continuations, fibers, HTTP/WebSocket or process spawning |
| `wasm32` | WebAssembly module | WASI/browser host; ARC by default; tracing GC is rejected; unsupported features rejected before code generation |
| `ios`, `ios-sim` | ARM64 static archive and C header | Swift host, iOS 15 minimum; ARC by default; tracing GC and resumable effects are rejected |
| `macos` | ARM64 static archive and C header | Swift host, macOS 12 minimum; ARC by default; same C ABI restrictions as the iOS archive |
| `android-arm64`, `android-x64` | ARM64 or x86-64 static archive and C header | Android host and JNI, API 26 minimum; ARC by default; tracing GC and resumable effects are rejected |

The compiler rejects unsupported target operations during `--check`, `--llvm`, and compilation. The iOS, macOS and Android archive targets reject `resume` and unavailable built-in process, HTTP, and WebSocket operations. Their hosts provide platform networking through the C boundary. The `macos` archive target is separate from an ordinary native macOS executable; its [Swift host contract](/spec/0038-iostarget/#mac-host-ios-macos-host) applies. WebAssembly also rejects fibers, terminal control, and arbitrary C imports; approved browser imports and supported host filesystem operations remain available.

[Issue Inbox](/docs/mobile-apps/) demonstrates the same Osprey project on iOS and Android, including ML modules and a small Default-flavor C entry point. Its screenshots come from actual simulator/emulator runs. Earlier builds were verified on a physical iPhone and through the shared C ABI on both Android ABIs. Those platform runs predate the callable handler migration; current checks cover shared domain behavior, LLVM/ABI generation, and native C entrypoints on Linux. Simulator, emulator, device, and hosted CI execution must be refreshed for the migrated sources; see the [validation record](https://github.com/Nimblesite/osprey/blob/main/examples/mobile/README.md#validation).

## Source compatibility

The current development build rejects these older forms:

- `handle … in …` and ML `handle … do …` are removed. Use `let h = handler E { … }` then `h(work)`, or a rest-of-block `handle`. Operations that use `resume` must be declared `control`.
- Integer `+`, `-` and `*` return `int` instead of `Result`; overflow and zero divisors need an `Arith` handler.

- A comparison needs two operands of one type; `true < 1` is a type error.

- A `map`/`filter` pipeline cannot be returned or passed to a separately compiled function; collect it with `toList` first.

- `//!` must be the first item of the file, namespace or module it documents; elsewhere it is a compile error. Use `//` for ordinary comments.

## Current limits

- Tail-call optimisation is not implemented.
- The [package manager](/docs/package-manager/) runs a folder registry, dependency resolution, verified fetching, locked builds, local checkout overrides and folder publishing. Named ambient builtin surfaces are checked against root grants. Source-bundle validation and folder publication reject unclassified raw native `extern` declarations; these checks do not establish complete confinement of untrusted packages. Python interop dependencies use uv. Crash-safe activation remains blocked on filesystem primitives. The hosted package website and public trust services remain planned. Working project/module examples do not imply every module-system feature is complete.
- A project is assembled from source, so a signature item with no implementation is an error. A module constant of an opaque type reaches a client as a bare literal; export a function instead.
- The strict static-memory mode described in the memory specification is not a current CLI option. Native builds default to `gc`; WebAssembly and hosted libraries default to `arc` and reject tracing GC. Managed archive availability depends on the target runtime build. The old `--memory=default` spelling is rejected.
- Hosted archives preserve application globals across calls and expose `osprey_release_string` for owned string results, without a public library-wide teardown API.
- The mobile sample reads one public GitHub issue page. Authentication, pagination, background refresh, and posting changes to GitHub are not implemented.
- GPU kernels run in software binary64 with no kernel fusion, so small kernels are launch-bound: GPT-2 trains about three times faster on an M4 Max GPU than on its CPU and generates no faster. `gpuScan` and `gpuFilter` have no device form; a selected device stops the run on them. Recorded hardware acceptance covers Apple GPUs. iOS, Android and WebAssembly support the CPU buffer backend; an explicit device selection fails because these targets carry no GPU adapter.
- C integrations are outside Osprey's memory-safety guarantee.
- Symbolic bug hunting ships as [`osprey hunt`](/docs/hunt/) (symbolic execution by KLEE) with symbolic stdin and files and native replay; the typed function harness, `libkleeRuntest` replay, regression-test generation, Windows and the comparison benchmarks are not built. See [plan 0034](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0034-symbolic-execution.md).

For detailed intended behavior and chapter-specific implementation notes, read
the [specifications](/spec/). For compiler-generated built-in signatures, use
the [API reference](/docs/).
