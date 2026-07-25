---
layout: page.njk
title: Feature Status
description: Current implementation boundaries for Osprey
date: "git Last Modified"
tags: ["status", "features", "roadmap"]
author: "Christian Findlay"
---

Osprey is an alpha language. The compiler is written in Rust, emits LLVM IR and
builds native binaries through clang. It also targets `wasm32-wasip1` with a
smaller runtime surface.

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
- Typed effect operations, lexical handlers and native single-shot `resume`
- Immutable persistent lists and maps
- Lightweight native fibers and channels
- Native HTTP, WebSocket, file, process and C FFI runtime APIs
- Default, tracing-GC and Perceus-ARC native memory backends
- Native and `wasm32-wasip1` compilation
- Compiler-backed formatting, documentation generation, testing, profiling and
  language-server commands

The runnable programs in
[`examples/tested/`](https://github.com/Nimblesite/osprey/tree/main/examples/tested)
are compiled and compared with checked-in expected output.

## Current limits

- Effect inputs and outputs are checked, but missing handlers and undeclared
  effect rows are not rejected in every case. Missing runtime handlers abort
  with an `unhandled effect` diagnostic.
- Resuming effects are native-only. WebAssembly supports handlers that return
  immediately.
- Tail-call optimisation is not implemented.
- User-defined generics and the package manager remain roadmap work.
- Multi-file modules are under development. Cross-flavor imports must not be
  treated as complete.
- The strict static-memory mode described in the memory specification is not a
  current CLI option. Native builds accept `default`, `gc` and `arc`.
- WebAssembly excludes fibers, HTTP, WebSockets, file and process operations,
  and the C FFI.
- C integrations are outside Osprey's memory-safety guarantee.

For detailed intended behavior and chapter-specific implementation notes, read
the [specifications](/spec/). For compiler-generated built-in signatures, use
the [API reference](/docs/).
