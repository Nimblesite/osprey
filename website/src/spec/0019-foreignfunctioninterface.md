---
layout: page
title: "Foreign Function Interface"
description: "Osprey Language Specification: Foreign Function Interface"
date: 2026-07-21
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0019-foreignfunctioninterface/"
---

# Foreign Function Interface

Osprey calls C (and any C-ABI library) through `extern fn` declarations. There are no per-library compiler builtins — SQLite, libpq, compression, crypto all bind through this one mechanism. Declaration grammar and the type ABI table are in [Syntax](/spec/0003-syntax/#extern-declarations); sandbox gating (`--no-ffi`, `--sandbox`) is in [Security and Sandboxing](/spec/0016-securityandsandboxing/).

> **Flavor layer — mixed.**  An `extern fn` *declaration* is a surface (CST) form: the spelling here is the Default flavor (`.osp`); the ML flavor (`.ospml`) spells the same declaration in offside layout, with the counterpart described in [ML Flavor Syntax](/spec/0024-mlflavorsyntax/). Both flavors lower to the single canonical `Stmt::Extern` (parameters as `ExternParameter`, signature via `TypeExpr`), and a callback argument lowers to `Expr::Identifier` or a capture-free `Expr::Lambda`. Everything below that node — the C-ABI type mapping, the `Ptr` type, link directives, and linking — is shared core and flavor-blind: per [FLAVOR-BOUNDARY] no phase after lowering can tell which flavor wrote the `extern`. See [Language Flavors](/spec/0023-languageflavors/).

## Link Directives [FFI-LINK-DIRECTIVES]

A source comment directive links a system library at compile time:

```osprey
// @link: sqlite3        → clang -lsqlite3
// @linkdir: /opt/lib    → clang -L/opt/lib
```

Directives are read from the source file and passed to the linker by both `--run` and `--compile`. Library names and paths are validated; shell injection through a directive is a compile-time error.

## The `Ptr` Type [FFI-PTR]

`Ptr` is an opaque C pointer (`i8*`). It may appear in `extern fn` and user function signatures, may be stored and passed, and supports no operations — no arithmetic, no dereference, no field access. It exists solely to carry C handles (a `sqlite3*`, a `FILE*`) through Osprey code.

C out-parameters (`sqlite3_open(path, &db)`) use the runtime's **pointer cells** — themselves plain `extern fn` declarations against the bundled runtime archive, not builtins:

```osprey
extern fn osprey_ffi_cell() -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern fn osprey_ffi_deref(cell: Ptr) -> Ptr   // read back the pointer C wrote
extern fn osprey_ffi_free(cell: Ptr) -> int    // release the cell
extern fn osprey_ffi_null() -> Ptr             // a NULL argument
```

```osprey-ml
extern osprey_ffi_cell -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern osprey_ffi_deref (cell : Ptr) -> Ptr   // read back the pointer C wrote
extern osprey_ffi_free (cell : Ptr) -> int    // release the cell
extern osprey_ffi_null -> Ptr             // a NULL argument
```

## Callbacks [FFI-CALLBACKS]

A named top-level function passed where an `extern fn` expects a function parameter lowers to a raw C code pointer. A capture-free lambda is accepted the same way; a **capturing** lambda is a compile-time error (captures cannot cross the C boundary; use a named function).

## Databases Are Libraries [FFI-NO-DB-BUILTINS]

Database access is not compiler surface. SQLite binds via `extern fn` declarations against `libsqlite3` (golden-tested in `examples/tested/db/`); an Osprey-level `Database` algebraic effect wraps the bindings (capability-gated `!Database`, swappable handlers). Postgres binds identically via `libpq`. A driver is an Osprey library, never compiler code.