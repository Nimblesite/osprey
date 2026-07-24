# Introduction

Osprey is a statically-typed functional language in the ML family. It compiles to native code via LLVM.

> **Flavor layer — mixed.** Default (`.osp`, specs 0001–0022) and ML
> (`.ospml`, [ML Flavor Syntax](0024-MLFlavorSyntax.md)) lower to the canonical
> `osprey_ast::Program` before semantic analysis. Type inference, effect
> checking, and codegen are flavor-blind. See
> [Language Flavors](0023-LanguageFlavors.md) for the surface/core boundary.

## Core Features

- Hindley-Milner type inference; explicit annotations are optional.
- Pattern matching as the only conditional construct (no `if`/`else`).
- Immutable bindings by default; `mut` opts in to mutability.
- Typed algebraic-effect operations with lexical handlers; complete static handler/row coverage remains in progress.
- `Result<T, E>` for all fallible operations; no exceptions, panics, or null.
- In the Default flavor, named arguments are required for functions of two or more parameters (`f(x: a, y: b)`); the ML flavor uses whitespace application (`f a b`) or the uncurried grouping (`f (x, y)`) instead.
- Lightweight fibers and channel-based concurrency.
- Swappable memory backends with a non-reclaiming default plus opt-in tracing GC and Perceus ARC; the `--static-memory` subset remains a design target.
- Built-in HTTP and WebSocket support.

## Status

This specification is the authoritative source for Osprey syntax and behaviour. The language and compiler are under active development; implementation status is called out per chapter where it diverges from the specification.
