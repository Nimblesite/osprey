# Introduction

Osprey is a statically typed functional language built around algebraic effects. Application code requests typed operations; handlers supply their implementation. Osprey compiles through LLVM to native code, WebAssembly, and C ABI libraries for iOS and Android. Default (`.osp`) and ML (`.ospml`) are surface
flavors of the same language; both lower to `osprey_ast::Program` before type
checking and code generation. Their precise boundary is defined in
[Language Flavors](0023-LanguageFlavors.md).

## Language shape

- Callable algebraic-effect handlers: `handler E { ... }` creates a policy, and `h(work)` runs a callback under it. `handle E { ... }` handles the rest of its block.
- Hindley-Milner inference with optional, constraining type annotations.
- Immutable bindings and explicit mutable bindings.
- Expression-oriented branching through `match`, ternaries, and the Default
  flavor's `if`/`else` expression.
- Checked operation requirements and explicit value/control operation modes. The [effects contract](0017-AlgebraicEffects.md) defines scoped rows and continuation semantics; [plan 0016](../plans/0016-algebraic-effects-and-handlers.md) distinguishes current support from unfinished requirements.
- `Result<T, E>` values for structured failures.

## Failure safety — [FAILURE-EXPLICIT]

Every operation that can fail MUST expose that possibility in its static type,
either as `Result<T, E>` or as an algebraic effect that is discharged by a
statically known handler. A language operation MUST NOT panic, silently wrap,
substitute a zero value, or erase an error in order to produce a plain `T`.

There is no implicit conversion from `Result<T, E>` to `T`. In particular,
bindings and assignments, function arguments (including concurrency
operations), comparisons, interpolation, function-value calls, and declared
scalar returns preserve the `Result` wrapper or are rejected. A caller obtains
a `T` only by exhaustively matching the `Result` or by supplying an explicit
fallback with `?:`.

Arithmetic is total and carries no `Result`: an arithmetic expression always evaluates to a defined value of its static type. It can never trap, panic, wrap silently, or produce an unspecified value, and a fault that cannot be proven impossible is discharged by a handler the program installs, or the program is rejected at compile time ([ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total)).

Raw foreign declarations may expose a C integer status as ABI data. Safe
Osprey-facing APIs MUST translate a failing status into `Result` or a typed
effect before returning it to ordinary language code.

## Runtime and platforms

- Fibers and typed channel communication.
- Default, tracing-GC, and Perceus ARC memory backends. The default backend does
  not reclaim every allocation; static-memory checking is not implemented.
- Native C interoperability and built-in HTTP and WebSocket runtimes.

Each later chapter states narrower availability or platform limits beside the
feature it specifies.
