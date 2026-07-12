---
layout: page
title: "Error Handling"
description: "Osprey Language Specification: Error Handling"
date: 2026-07-12
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0013-errorhandling/"
---

# Error Handling

Osprey has no exceptions, panics, or null. Any function that can fail returns a `Result`.

> **Flavor layer — shared core (AST and above).**  The `Result` type, the error model, and function-boundary auto-unwrap live entirely at or above the canonical AST (`osprey_ast::Program`) and are flavor-blind — they operate on the `Result<T, E>` union type, `Match` arms, and `Call` results identically no matter whether a program was spelled in the Default (`.osp`) or ML (`.ospml`) surface. Per [FLAVOR-BOUNDARY], no phase that observes errors (type inference, IR lowering, codegen, runtime) may inspect which flavor produced the program. Note the [Language Flavors](/spec/0023-languageflavors/) assumption that arithmetic stays `Result`-wrapped in **both** flavors (overflow-checked, yielding `Result<int, MathError>`); the clean `int` output programs see is the shared auto-unwrap erasing the wrapper, not a flavor rule. The code samples below use the Default surface (`.osp`) — `let` bindings and brace-delimited `match`; the same programs in the ML flavor (`.ospml`, offside layout, bare `name = expr`, see [ML Flavor Syntax](/spec/0024-mlflavorsyntax/)) lower to identical ASTs and obey these error rules unchanged.

## Status

[ERR-PAYLOAD] conforms for `E = string`: the runtime Result block carries a
dedicated `i8* errmsg` slot, `Error { message }` binds the real reason, and
`toString` renders `Error(<reason>)`. Discriminated-union error payloads
(`Result<T, StringError>`) remain deferred behind recursive-union payload
support.

## The Result Type

```osprey
type Result<T, E> = Success { value: T } | Error { message: E }
```

The compiler rejects any direct access to the contained value. Callers must pattern-match the `Result` (see [Pattern Matching](/spec/0007-patternmatching/)) unless one of the auto-unwrap contexts applies ([Result Auto-Unwrapping](/spec/0004-typesystem/#result-auto-unwrapping)):

```osprey
let result = someFunctionThatCanFail()

match result {
    Success { value }   => print("Success: ${value}")
    Error   { message } => print("Error: ${message}")
}
```

```osprey-ml
result = someFunctionThatCanFail

match result
    Success value   => print "Success: ${value}"
    Error   message => print "Error: ${message}"
```

## Arithmetic Returns Result

Every arithmetic operation returns `Result<T, MathError>` so overflow, underflow, and division by zero surface as values, not panics.

| Operator    | int, int                   | float, float               | int, float / float, int                   |
| ----------- | -------------------------- | -------------------------- | ----------------------------------------- |
| `+ - * %`   | `Result<int,   MathError>` | `Result<float, MathError>` | `Result<float, MathError>` (int promoted) |
| `/`         | `Result<float, MathError>` | `Result<float, MathError>` | `Result<float, MathError>`                |

`/` always yields `float`. There is no implicit `int`/`float` conversion outside this table; use `toFloat` and `toInt` for explicit conversion.

```osprey
let sum       = 1 + 3      // Result<int,   MathError>
let quotient  = 10 / 3     // Result<float, MathError>
let remainder = 10 % 3     // Result<int,   MathError>
let mixed     = 10 + 5.5   // Result<float, MathError>
let divZero   = 10 / 0     // Error(DivisionByZero)
```

```osprey-ml
sum       = 1 + 3      // Result<int,   MathError>
quotient  = 10 / 3     // Result<float, MathError>
remainder = 10 % 3     // Result<int,   MathError>
mixed     = 10 + 5.5   // Result<float, MathError>
divZero   = 10 / 0     // Error(DivisionByZero)
```

#### Chaining Arithmetic

Compound expressions auto-unwrap intermediate `Result`s — `(10 + 5) * 2` is a single `Result<int, MathError>`, never a nested one, and only the final value is matched ([Result Auto-Unwrapping](/spec/0004-typesystem/#result-auto-unwrapping)):

```osprey
match (10 + 5) * 2 {
    Success { value }   => print("Final: ${value}")
    Error   { message } => print("error: ${message}")
}
```

```osprey-ml
match (10 + 5) * 2
    Success value   => print "Final: ${value}"
    Error   message => print "error: ${message}"
```

### toString Format

A `Result` formats as `Success(<value>)` or `Error(<message>)`:

```osprey
print(toString(15 / 3))   // "Success(5.0)"  — division is always float
print(toString(10 / 0))   // "Error(division by zero)"
```

```osprey-ml
print (toString (15 / 3))   // "Success(5.0)"  — division is always float
print (toString (10 / 0))   // "Error(division by zero)"
```

## Error Payload Propagation — [ERR-PAYLOAD]

When a function produces `Error { message: E }`, the value bound to `message` in the caller's `match` arm MUST be the exact `E` value that the producer wrote — never a placeholder, never a static string, never a default. The discriminant ("this `Result` is an `Error`") and the payload ("what went wrong") are both part of the value; throwing away one defeats the type.

```osprey
match split("abc", "") {
    Success { value }   => forEach(value, print)
    Error   { message } => print(message)   // MUST print "separator is empty",
                                            // not "Error occurred"
}
```

```osprey-ml
match split ("abc", "")
    Success value   => forEach (value, print)
    Error   message => print message   // MUST print "separator is empty",
                                       // not "Error occurred"
```

This requirement applies uniformly across arithmetic, string, list, map, file-I/O, HTTP, and user-defined fallible functions, and to nested `Result` chains (auto-unwrap MUST preserve the original error payload). Implementations that lose the payload — for example by binding the pattern variable to a static global — are non-conforming.