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
([Result Default](0007-PatternMatching.md#result-default---pattern-result-default)).
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

> **Status:** this section describes the shipped compiler and remains authoritative. A replacement model — arithmetic returning plain `int`/`float` with failure dispatched to a statically required `Arith` effect handler — is specified as a normative target in [Arithmetic Effects](0037-ArithmeticEffects.md) and delivered by [plan 0027](../plans/0027-arithmetic-effects.md), motivated by [#230](https://github.com/Nimblesite/osprey/issues/230): the corpus discharges these `Result`s with fabricated `?:` fallbacks, which silently produce wrong answers.

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

### Negated Literals — [ARITH-NEG-LITERAL]

A negated numeric *literal* is folded at parse time into a literal of the
opposite sign, so `-1` has type `int` and `-1.5` has type `float` — neither is a
`Result`. Without the fold, `let x: int = -1` would fail to typecheck and a
negative literal would be unusable wherever a plain `int` is required.

The fold is total, never partial: the one genuinely overflowing case,
`-(-9223372036854775808)`, is reachable only by double negation and stays a
checked unary operation returning `Result<int, MathError>`. Negation of any
non-literal operand is ordinary checked arithmetic under [ARITH-CHECKED].

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

### Choosing and preserving a policy

`?:` is the concise form only when replacing every error with one fallback is
the intended policy. Use an exhaustive `match` when the diagnostic must remain
available. An expected `string` never causes a successful integer payload to
be coerced to text and never turns the error branch into a string; both branches
must be mapped explicitly into a new `Result`.

```osprey
fn renderSum(a, b) = match a + b {
    Success { value } => Success { value: toString(value) }
    Error { message } => Error { message: message }
}
```

```osprey-ml
renderSum (a, b) = match a + b
    Success value => Success(value = toString value)
    Error message => Error(message = message)
```

An algebraic effect may centralize a larger region's policy. The effect must
carry the original diagnostic and return a `Result`; its handler is statically
required and may preserve or deliberately recover from the error. Function
return types and effect rows in this example are inferred.

```osprey
effect ArithmeticFailure {
    decide: fn(string) -> Result<int, MathError>
}

fn addThroughPolicy(a, b) = match a + b {
    Success { value } => Success { value: value }
    Error { message } => perform ArithmeticFailure.decide(message)
}

fn preserveFailure(a, b) = handle ArithmeticFailure
    decide message => Error { message: message }
in addThroughPolicy(a, b)
```

```osprey-ml
effect ArithmeticFailure
    decide : string => Result<int, MathError>

addThroughPolicy (a, b) = match a + b
    Success value => Success(value = value)
    Error message => perform ArithmeticFailure.decide message

preserveFailure (a, b) =
    handle ArithmeticFailure
        decide message => Error(message = message)
    in addThroughPolicy (a, b)
```

Executable preserve-and-recover cases for both flavors live in
`tests/core/arithmetic/effect_policies.test.osp{,ml}`.

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

## Discarding a Result — [ERROR-RESULT-DISCARD]

A `Result` left in statement position is rejected, because dropping it drops the
error channel with it — the one thing the type exists to make visible:

```
an unhandled `Result` cannot be discarded; use `match` or `?:`
```

```osprey
fn go() -> int = {
    risky(1)                    // rejected: the Error arm would vanish
    let _ = risky(1)            // rejected: `_` cannot consent to losing an error
    let ok = risky(1) ?: 0      // accepted: the failure has an answer
    ok
}
```

`let _ =` is the sanctioned way to discard an ordinary value
([BLOCK-DISCARD](0008-BlockExpressions.md#discarded-values--block-discard)), but
it does not discharge a `Result`. Handle the failure with `match` or `?:`, or
return it to a caller who will.
