---
layout: page.njk
title: Feature Status
description: Current implementation status of Osprey language features
date: "git Last Modified"
tags: ["status", "features", "roadmap"]
author: "Christian Findlay"
---

Current version: **{% if releases.latest %}{{ releases.latest.tag }}{% else %}v0.9.0{% endif %}** (released). The compiler is written in Rust and emits LLVM IR.

## 📦 Releases

<em>Generated at build time from the [GitHub Releases](https://github.com/Nimblesite/osprey/releases) page.</em>

{% if releases.list.length %}
| Version | Released | |
| --- | --- | --- |
{% for r in releases.list -%}
| [{{ r.tag }}]({{ r.url }}){% if r.prerelease %} <sup>pre-release</sup>{% endif %} | {{ r.date }} | {% if loop.first %}**Latest**{% endif %} |
{% endfor %}
{% else %}
Release list unavailable at build time — see the [GitHub Releases](https://github.com/Nimblesite/osprey/releases) page.
{% endif %}

## ✅ Complete Features

### Core Language
- **Variables & Constants**: `let` declarations, immutable by default
- **Data Types**: `int`, `float`, `string`, `bool`, with Hindley-Milner type inference
- **Functions**: Function declarations, expression bodies, named arguments (2+ params)
- **String Interpolation**: `${}` syntax with arbitrary expressions
- **Pattern Matching**: `match` expressions, wildcards, union-type and type-annotation patterns with exhaustiveness checking
- **Block Expressions**: Local scoping, multi-statement blocks
- **Arithmetic Operations**: Safe arithmetic returning `Result` types
- **Boolean Operations**: Logical operators and boolean expressions

### Algebraic Effects
- **`effect` declarations**: name a set of typed operations, e.g. `effect Logger { log: fn(string) -> Unit }`
- **`perform`**: invoke an effect operation — `perform Logger.log("hi")`
- **`handle … in`**: supply handlers for an effect over an expression; nested handlers override outer ones
- **Effect annotations**: functions declare the effects they use — `fn f() -> T !Logger` or `![Logger, Metrics]`
- **Typed operations**: the checker validates operation arguments/results and resolves generic effect instantiations
- **Current coverage limit**: missing handlers and undeclared effect rows are not yet rejected in every case; a missing runtime handler aborts with an `unhandled effect` diagnostic
- **Resuming handlers**: explicit single-shot `resume` works on native targets; Wasm currently supports non-resuming handler-stack dispatch only

### Functional Programming
- Complete iterator family (`range`, `forEach`, `map`, `filter`, `fold`)
- Stream fusion optimization for zero-cost abstractions
- Pipe operator (`|>`) for elegant composition
- Function chaining with compile-time optimization
- **Union Types**: Algebraic data types with pattern matching and exhaustiveness checking
- **Any Type Handling**: Explicit `any` types with mandatory pattern matching
- **Result Types**: Error handling without exceptions
- **Type Safety**: No implicit conversions, full compile-time type checking

### Concurrency
- **Lightweight fibers**: `spawn` an expression, `await` the result (out-of-order awaits are fine)
- **`yield` / `fiber_yield`**: cooperatively hand control back to the scheduler
- **`fiberDone`**: non-blocking probe — `1` if a fiber has finished, `0` if still running
- **Channels**: `Channel(capacity)`, `send`, `recv` for message passing between fibers

### HTTP & Networking
- **HTTP Client**: `httpCreateClient`, `httpGet`, `httpPost`, `httpPut`, `httpDelete`
- **HTTPS**: TLS support via OpenSSL — `https://` URLs work out of the box
- **HTTP Server**: `httpCreateServer`, `httpListen`, concurrent request handling for all methods
- **JSON**: parse and traverse JSON responses (`jsonParse`, `jsonGet`, `jsonLength`)

### C Interoperability (FFI)
- **`// @link: <lib>`** directive links a C library at compile time
- **`extern fn`** declarations bind C functions with typed signatures
- **`Ptr`** type carries opaque C handles (no arithmetic or dereference — handles only)
- **Pointer cells** (`osprey_ffi_cell` / `osprey_ffi_deref` / `osprey_ffi_free`) handle C out-parameters
- **SQLite** is driven entirely through this FFI — see [`examples/tested/db`](https://github.com/Nimblesite/osprey/tree/main/examples/tested/db), including a capability-safe `Database` effect wrapper with bound parameters

### Terminal UIs
- A full TUI is built from effects + pure string composition + ANSI codes — no framework required
- Raw-mode key input, colored output, spinners, and live HTTPS/JSON data — see [`examples/tui`](https://github.com/Nimblesite/osprey/tree/main/examples/tui)

### Persistent Collections
- **`List<T>`**: 32-way bitmapped vector trie with tail buffer (Bagwell 2000; Hickey, *Clojure*). Append is O(log₃₂ n) amortised; structural sharing keeps old versions valid.
- **`Map<K, V>`**: 32-way Hash Array Mapped Trie (HAMT) with bitmap-packed children and collision nodes. Lookup, insert and remove are O(log₃₂ n) expected. Keys: `int`, `string`, `bool`.
- **Builtins**: `listLength`, `listAppend`, `listPrepend`, `listConcat`, `listReverse`, `listContains`, `forEachList`, `mapLength`, `mapContains`, `mapSet`, `mapRemove`, `mapMerge`, `mapKeys`, `mapValues`.
- **`+` operator**: `List<T> + List<T>` concatenates; `Map<K,V> + Map<K,V>` is a right-biased union.
- **Test coverage**: 33 C-level assertions (10k-element stress, hash collisions, structural-sharing invariants) plus 12 e2e Osprey programs in [`examples/tested/basics/lists`](https://github.com/Nimblesite/osprey/tree/main/examples/tested/basics/lists) with byte-exact output verification.

### Built-in Functions
- **I/O**: `print()`, `input()`, `toString()`
- **String Utilities**: `length`, `contains`, `substring`, `split`, `join`, `trim`, `replace`, and more
- **File System**: `writeFile()`, `readFile()` (with Result types)
- **Process Operations**: `spawnProcess()`, `sleep()`
- **Safe Math**: All arithmetic operations return `Result` types

## 🚧 Roadmap Features

### Type System Extensions
- **Record Types with Constraints**: `where` clause validation (partially implemented)
- **Generic Types**: User-defined type parameters and polymorphism
- **Module System**: Fiber-isolated modules with proper imports

### Advanced Language Features
- **Static effect coverage**: retain effect rows in function types, propagate them through calls, and reject every unhandled or undeclared effect before code generation
- **Wasm resumable handlers**: replace the native pthread continuation path with a thread-free continuation strategy
- **Advanced Pattern Matching**: list/`[head, ...tail]` patterns, constructor patterns with guards
- **Select Expressions**: channel multiplexing for concurrent operations
- **WebSockets**: client and server exist but are being hardened — the `Result`-typed API and some server bind scenarios are still in progress
- **Streaming Responses**: large HTTP response streaming

### Tooling & Ecosystem
- **Package Manager**: Dependency management system
- **Standard Library**: Broader built-in coverage
- **REPL**: Interactive development environment
- **Language Server**: Full IDE support beyond the VS Code extension
- **Richer FFI ergonomics**: higher-level bindings over the existing C interop

---

**Note**: Features marked as complete have working examples in the [`examples/tested/`](https://github.com/Nimblesite/osprey/tree/main/examples/tested) directory whose output is byte-compared against checked-in expectations on every build.
