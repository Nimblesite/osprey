# Introduction

Osprey is a statically typed functional language that compiles through LLVM to
native code or WebAssembly. Default (`.osp`) and ML (`.ospml`) are surface
flavors of the same language; both lower to `osprey_ast::Program` before type
checking and code generation. Their precise boundary is defined in
[Language Flavors](0023-LanguageFlavors.md).

## Language shape

- Hindley-Milner inference with optional, constraining type annotations.
- Immutable bindings and explicit mutable bindings.
- Expression-oriented branching through `match`, ternaries, and the Default
  flavor's `if`/`else` expression.
- Typed algebraic-effect operations and lexical handlers. The compiler checks
  operation inputs and outputs but does not reject every missing handler.
- `Result<T, E>` values for structured failures; native APIs that return integer
  status codes document that convention explicitly.

## Runtime and platforms

- Fibers and typed channel communication.
- Default, tracing-GC, and Perceus ARC memory backends. The default backend does
  not reclaim every allocation; static-memory checking is not implemented.
- Native C interoperability and built-in HTTP and WebSocket runtimes.

Each later chapter states narrower availability or platform limits beside the
feature it specifies.
