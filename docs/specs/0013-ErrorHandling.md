# Error Handling

Osprey has no exceptions, panics, or null. Any function that can fail returns a `Result`.

> **Flavor layer — shared core (AST and above).** Error semantics are flavor-blind after lowering to `osprey_ast::Program`; no later phase may inspect the source flavor ([FLAVOR-BOUNDARY]). [ARITH-PLAIN] applies identically to both flavors. Examples use the Default surface; see [ML Flavor Syntax](0024-MLFlavorSyntax.md) for ML spelling.

## Status

[ERR-PAYLOAD] conforms for `E = string`: the runtime Result block carries a
dedicated `i8* errmsg` slot, `Error { message }` binds the real reason, and
`toString` renders `Error(<reason>)`. Discriminated-union error payloads
(`Result<T, StringError>`) remain deferred behind recursive-union payload
support.

[ARITH-PLAIN] is **specified, not implemented**: `+ - *` still return
`Result<int, MathError>` today, `%` emits an unchecked `srem`, and
`checkedAdd`/`checkedSub`/`checkedMul` do not exist. Sequencing and the corpus
migration are in [plan 0019](../plans/0019-ml-elegance.md).

## The Result Type

```osprey
type Result<T, E> = Success { value: T } | Error { message: E }
```

The compiler rejects any direct access to the contained value. Callers must pattern-match the `Result` (see [Pattern Matching](0007-PatternMatching.md)) unless one of the auto-unwrap contexts applies ([Result Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping)) or the `?:` default form supplies a fallback ([Ternary Match](0007-PatternMatching.md#ternary-match-syntactic-sugar), which owns the rule).

```osprey
let result = someFunctionThatCanFail()
let value  = someFunctionThatCanFail() ?: 0

match result {
    Success { value }   => print("Success: ${value}")
    Error   { message } => print("Error: ${message}")
}
```

```osprey-ml
result = someFunctionThatCanFail
value  = someFunctionThatCanFail ?: 0

match result
    Success value   => print "Success: ${value}"
    Error   message => print "Error: ${message}"
```

## Arithmetic and Result — [ARITH-PLAIN]

An operator whose only failure mode is overflow returns the plain type, because overflow wraps two's complement and every wrapped result is representable; an operator that can be handed a value with **no** representable result — `/` and `%` by zero — keeps `Result<_, MathError>`.

| Operator    | int, int                   | float, float               | int, float / float, int                   |
| ----------- | -------------------------- | -------------------------- | ----------------------------------------- |
| `+ - *`     | `int`                      | `float`                    | `float` (int promoted)                    |
| `/`         | `Result<float, MathError>` | `Result<float, MathError>` | `Result<float, MathError>`                |
| `%`         | `Result<int,   MathError>` | `Result<float, MathError>` | `Result<float, MathError>` (int promoted) |

`/` always yields `float`. There is no implicit `int`/`float` conversion outside this table; use `toFloat` and `toInt` for explicit conversion. The builtins `checkedAdd`, `checkedSub`, and `checkedMul` (both flavors, following the `intDiv` precedent) return `Result<int, MathError>` via `llvm.s{add,sub,mul}.with.overflow`, making the overflow guarantee explicit and opt-in at the call site. A later [ARITH-EFFECT] phase MAY instead have these operators perform overflow as an algebraic effect; the surface syntax is identical either way, so nothing specified here changes.

Status: specified; not yet implemented. Today `+ - *` are still `Result`-wrapped, `%` emits a bare `srem` with no zero check (`10 % 0` is undefined), and `checkedAdd`/`checkedSub`/`checkedMul` do not exist.

```osprey
let sum       = 1 + 3      // int
let quotient  = 10 / 3     // Result<float, MathError>
let remainder = 10 % 3     // Result<int,   MathError>
let mixed     = 10 + 5.5   // float
let checked   = checkedAdd(a: 1, b: 3)   // Result<int, MathError>
let divZero   = 10 / 0     // Error(DivisionByZero)
```

```osprey-ml
sum       = 1 + 3      // int
quotient  = 10 / 3     // Result<float, MathError>
remainder = 10 % 3     // Result<int,   MathError>
mixed     = 10 + 5.5   // float
checked   = checkedAdd (1, 3)   // Result<int, MathError>
divZero   = 10 / 0     // Error(DivisionByZero)
```

#### Chaining Arithmetic

`(10 + 5) * 2` is plain `int`: there is no wrapper to nest and nothing to match. Where `/` or `%` appears, the `Result` **propagates**: the enclosing arithmetic expression is a single `Result<float, MathError>`, flattened rather than nested, an erroring operand makes the whole expression `Error`, and only the final value is matched. Arithmetic is deliberately not an auto-unwrap context ([Result Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping)) — unwrapping an operand would discard its error.

> **Defect, not yet fixed.** The implementation unwraps instead of propagating, and fabricates a success payload on the error path: `toString((10 / 0) + 1.0)` evaluates to `Success(1.0)` rather than `Error(division by zero)`. Tracked in [plan 0019](../plans/0019-ml-elegance.md).

```osprey
match (10 + 5) / 2 {
    Success { value }   => print("Final: ${value}")
    Error   { message } => print("error: ${message}")
}
```

```osprey-ml
match (10 + 5) / 2
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

When a function produces `Error { message: E }`, the value bound to `message` in the caller's `match` arm MUST be the exact `E` value that the producer wrote, not a placeholder, static string, or default.

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
