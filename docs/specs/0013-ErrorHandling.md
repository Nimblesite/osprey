# Error Handling

Osprey has no language-level exceptions. Every fallible language operation
uses `Result` or a statically handled algebraic effect. A raw foreign status is
ABI data and MUST be translated at the safe Osprey boundary.

The two language flavors share these semantics. Examples show both surfaces
where their syntax differs.

## The Result Type

```osprey
type Result<T, E> = Success { value: T } | Error { message: E }
```

The compiler rejects direct access to the contained value and never implicitly
converts `Result<T, E>` to `T`. Callers must pattern-match the `Result` (see
[Pattern Matching](0007-PatternMatching.md)) or use `?:` to supply a fallback
([Result Default](0007-PatternMatching.md#result-default--pattern-result-default)).
Assignments, arguments, comparisons, interpolation, and declared plain return
types do not erase the wrapper ([Result Preservation](0004-TypeSystem.md#result-preservation)).

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

## Arithmetic and Result — [ARITH-CHECKED]

Integer arithmetic is overflow checked. Integer `+`, `-`, `*`, unary `-`, and
`abs` return `Result<int, MathError>`; overflow returns
`Error("integer overflow")` and never wraps or panics. Floating-point `+`, `-`,
`*`, and unary `-` are plain IEEE-754 operations. Division and remainder return
`Result<_, MathError>` so they can report a zero divisor (and integer division's
unrepresentable minimum-value divided by `-1` case).

| Operator    | int, int                   | float, float               | int, float / float, int                   |
| ----------- | -------------------------- | -------------------------- | ----------------------------------------- |
| `+ - *`     | `Result<int, MathError>`   | `float`                    | `float` (int promoted before operation)   |
| `/`         | `Result<float, MathError>` | `Result<float, MathError>` | `Result<float, MathError>`                |
| `%`         | `Result<int, MathError>`   | `Result<float, MathError>` | `Result<float, MathError>` (int promoted) |

`/` always yields `float`. Unary integer `-` has type
`int -> Result<int, MathError>`; unary float `-` has type `float -> float`.
The legacy builtins `checkedAdd`, `checkedSub`, and `checkedMul` remain safe
aliases returning `Result<int, Error>`.
Integer `-9223372036854775808 % -1` returns `Success(0)`; the representable
remainder is produced without executing LLVM's overflowing `srem` case.


```osprey
let sum       = 1 + 3      // Result<int, MathError>
let quotient  = 10 / 3     // Result<float, MathError>
let remainder = 10 % 3     // Result<int, MathError>
let mixed     = 10 + 5.5   // float
let checked   = checkedAdd(a: 1, b: 3)   // Result<int, Error>
let divZero   = 10 / 0     // Error(division by zero)
```

```osprey-ml
sum       = 1 + 3      // Result<int, MathError>
quotient  = 10 / 3     // Result<float, MathError>
remainder = 10 % 3     // Result<int, MathError>
mixed     = 10 + 5.5   // float
checked   = checkedAdd (1, 3)   // Result<int, Error>
divZero   = 10 / 0     // Error(division by zero)
```

### Chaining Arithmetic

An arithmetic chain such as `(10 + 5) * 2` has one flattened
`Result<int, MathError>`, not a nested `Result`. Each operation runs only after
its `Result` operands succeed; the first `Error` is propagated unchanged and
later operations are not evaluated. This failure-preserving flattening is the
only context that may consume a `Result` payload without an explicit `match` or
`?:`, and it may only flatten the common `MathError` channel. It is not a
conversion from `Result<T, E>` to `T` ([Result Preservation](0004-TypeSystem.md#result-preservation)).

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
    Success { value }   => forEachList(value, print)
    Error   { message } => print(message)   // "split: separator must not be empty"
}
```

```osprey-ml
match split ("abc", "")
    Success value   => forEachList value print
    Error   message => print message   // "split: separator must not be empty"
```

This requirement applies to every `Result`-returning operator, builtin, and
user function, including failure-preserving arithmetic chains.
