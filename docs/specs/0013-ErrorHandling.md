# Error Handling

Osprey has no language-level exceptions. Structured failures use `Result`;
low-level native APIs may instead declare an integer status convention in their
own specification.

The two language flavors share these semantics. Examples show both surfaces
where their syntax differs.

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

An operator whose only failure mode is overflow returns the plain type, because
overflow wraps two's complement. Division and remainder retain `Result<_,
MathError>` so they can report a zero divisor.

| Operator    | int, int                   | float, float               | int, float / float, int                   |
| ----------- | -------------------------- | -------------------------- | ----------------------------------------- |
| `+ - *`     | `int`                      | `float`                    | `float` (int promoted)                    |
| `/`         | `Result<float, MathError>` | `Result<float, MathError>` | `Result<float, MathError>`                |
| `%`         | `Result<int, MathError>`   | `Result<float, MathError>` | `Result<float, MathError>` (int promoted) |

`/` always yields `float`. The builtins `checkedAdd`, `checkedSub`, and
`checkedMul` return `Result<int, Error>` and make overflow checking explicit.
Integer `-9223372036854775808 % -1` returns `Success(0)`; the representable
remainder is produced without executing LLVM's overflowing `srem` case.


```osprey
let sum       = 1 + 3      // int
let quotient  = 10 / 3     // Result<float, MathError>
let remainder = 10 % 3     // Result<int, MathError>
let mixed     = 10 + 5.5   // float
let checked   = checkedAdd(a: 1, b: 3)   // Result<int, Error>
let divZero   = 10 / 0     // Error(division by zero)
```

```osprey-ml
sum       = 1 + 3      // int
quotient  = 10 / 3     // Result<float, MathError>
remainder = 10 % 3     // Result<int, MathError>
mixed     = 10 + 5.5   // float
checked   = checkedAdd (1, 3)   // Result<int, Error>
divZero   = 10 / 0     // Error(division by zero)
```

### Chaining Arithmetic

`(10 + 5) * 2` is plain `int`. Where `/` or `%` appears, the enclosing
expression has one flattened `Result<T, MathError>`; an erroring operand makes the
whole expression `Error`. Arithmetic is not an auto-unwrap context ([Result
Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping)).

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
user function, including nested auto-unwrap chains.
