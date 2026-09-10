---
layout: page.njk
title: Feature Status
description: Current implementation boundaries for Osprey
date: "git Last Modified"
tags: ["status", "features", "roadmap"]
author: "Christian Findlay"
---

Osprey is an alpha language. The compiler is written in Rust, emits LLVM IR and
builds native executables, `wasm32-wasip1` modules, and app-logic libraries for
iOS and Android. Each target has an explicit runtime boundary.

Current version: **{% if releases.latest %}{{ releases.latest.tag }}{% else %}development build{% endif %}**.

## Releases

{% if releases.list.length %}
| Version | Released | |
| --- | --- | --- |
{% for r in releases.list -%}
| [{{ r.tag }}]({{ r.url }}){% if r.prerelease %} <sup>pre-release</sup>{% endif %} | {{ r.date }} | {% if loop.first %}Latest{% endif %} |
{% endfor %}
{% else %}
The release list was unavailable when this page was built. See
[GitHub Releases](https://github.com/Nimblesite/osprey/releases).
{% endif %}

## Implemented foundations

- Default (`.osp`) and ML (`.ospml`) source parsing, both lowering to the same
  AST before semantic analysis
- Hindley–Milner type inference, algebraic data types and exhaustive pattern
  matching for supported patterns
- User-defined generics, declaration-site variance, generic effects and explicit
  call-site type arguments in both flavors
- Typed effect operations, lexical handlers, compile-time rejection of missing handlers, and single-shot `resume` for `--target=native`
- Immutable persistent lists and maps
- Lightweight native fibers and channels
- Native HTTP, WebSocket, file, process and C FFI runtime APIs
- Default, tracing-GC and Perceus-ARC native memory backends
- Native and `wasm32-wasip1` compilation, plus iOS and Android C ABI targets
- A shared modular mobile application with reactive native UI, SQLite cache, live GitHub requests, and local notes and priorities
- Compiler-backed formatting, documentation generation, testing, profiling and
  language-server commands
- [HTML API documentation](/docs/documentation/) for public modules in both
  flavors, with executable examples, Markdown guides, custom CSS, three themes,
  offline search and responsive navigation

The runnable programs in
[`tests/regressions/`](https://github.com/Nimblesite/osprey/tree/main/tests/regressions)
are compiled and compared with checked-in expected output.

## Target support

| Target | Output | Runtime boundary |
| --- | --- | --- |
| `native` | Executable for the host platform | Native runtime, including resumable effects; default, GC, or ARC memory |
| `wasm32` | WebAssembly module | WASI/browser host; default memory; unsupported features rejected before code generation |
| `ios`, `ios-sim` | ARM64 static archive and C header | Swift host, iOS 15 minimum; default memory; no resumable effects |
| `android-arm64`, `android-x64` | ARM64 or x86-64 static archive and C header | Android host and JNI, API 26 minimum; default memory; no resumable effects |

The compiler rejects unsupported target operations during `--check`, `--llvm`, and compilation. iOS and Android reject `resume` and unavailable built-in process, HTTP, and WebSocket operations. Their hosts provide platform networking through the C boundary. WebAssembly also rejects fibers, terminal control, and arbitrary C imports; approved browser imports and supported host filesystem operations remain available.

[Issue Inbox](/docs/mobile-apps/) demonstrates the same Osprey project on iOS and Android, including ML modules and a small Default-flavor C entry point. Its screenshots come from actual simulator/emulator runs. The iOS app has also been installed and verified on a physical iPhone. Both Android ABIs execute the shared test corpus through the C ABI.

## Current limits

- Resumable effects are supported by `--target=native`. Mobile C ABI targets and WebAssembly reject them at compile time; supported handlers that return immediately remain usable.
- The effect checker follows operations through the closed program, including exported mobile functions. It does not yet provide general polymorphic effect-row variables in public higher-order signatures.
- Tail-call optimisation is not implemented.
- The package manager remains roadmap work. Working project/module examples do not imply every module-system feature is complete.
- The strict static-memory mode described in the memory specification is not a current CLI option. Native builds accept `default`, `gc`, and `arc`; mobile and WebAssembly accept `default` only.
- The initial mobile runtime retains general allocations for process lifetime and has no public library teardown or returned-string release API.
- The mobile sample reads one public GitHub issue page. Authentication, pagination, background refresh, and posting changes to GitHub are not implemented.
- C integrations are outside Osprey's memory-safety guarantee.

For detailed intended behavior and chapter-specific implementation notes, read
the [specifications](/spec/). For compiler-generated built-in signatures, use
the [API reference](/docs/).
