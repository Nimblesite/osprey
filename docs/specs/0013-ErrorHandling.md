# Error Handling

Osprey has no language-level exceptions. Every fallible language operation
uses `Result` or a statically handled algebraic effect. A raw foreign status is
ABI data and MUST be translated at the safe Osprey boundary.

**Arithmetic is not merely checked; it is total.** In an accepted Osprey program an arithmetic expression always evaluates to a defined value of its static type: it can never trap, panic, wrap silently, or produce an unspecified value, and a fault that cannot be proven impossible MUST be discharged — or the program is rejected at compile time. That guarantee is normative and spelling-independent; its clauses and conformance obligations are [ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total). The `Result` mechanism specified below is how the shipped compiler delivers it, and [plan 0027](../plans/0027-arithmetic-effects.md) changes the mechanism without weakening a single clause.

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

## Arithmetic — [ARITH-CHECKED]

Arithmetic is total. Every arithmetic expression evaluates to a defined value of its static type, and integer `+`, `-`, `*`, `%`, unary `-`, and `abs` have type `int`; `/` has type `float`. There is no `Result` in an arithmetic type.

An operation whose mathematical result is unrepresentable — integer overflow, a zero divisor — performs an operation of the compiler-declared `Arith` effect, and the handler the region installed substitutes the value its policy chooses. The full guarantee, the operation signatures, and the policy forms are [Arithmetic Effects](0037-ArithmeticEffects.md).

| Operator | int, int | float, float | int, float / float, int |
| --- | --- | --- | --- |
| `+ - *` | `int` | `float` | `float` (int promoted before operation) |
| `/` | `float` | `float` | `float` |
| `%` | `int` | `float` | `float` (int promoted) |

`/` always yields `float`. Unary integer `-` has type `int -> int`; unary float `-` has type `float -> float`. Floating-point `+`, `-`, `*`, and unary `-` are plain IEEE-754 operations. Integer `-9223372036854775808 % -1` yields `0`; the representable remainder is produced without executing LLVM's faulting `srem` case. The builtins `checkedAdd`, `checkedSub`, and `checkedMul` return `Result<int, Error>` and are the explicit value-level spelling for code that wants overflow as data.

```osprey
let sum       = 1 + 3      // int
let quotient  = 10 / 3     // float
let remainder = 10 % 3     // int
let mixed     = 10 + 5.5   // float
let checked   = checkedAdd(a: 1, b: 3)   // Result<int, Error>
```

```osprey-ml
sum       = 1 + 3      // int
quotient  = 10 / 3     // float
remainder = 10 % 3     // int
mixed     = 10 + 5.5   // float
checked   = checkedAdd (1, 3)   // Result<int, Error>
```

### Negated Literals — [ARITH-NEG-LITERAL]

A negated numeric *literal* is folded at parse time into a literal of the opposite sign, so `-1` has type `int` and `-1.5` has type `float`. The fold is total: the one overflowing case, `-(-9223372036854775808)`, is reachable only by double negation and performs `Arith.overflow` like any other unrepresentable negation.

### Arithmetic policies

A region states its arithmetic policy once, in a handler, instead of at every operation. Wrapping consumes the operation's two's-complement payload; a fault-recording policy writes handler-owned state and lets the boundary decide.

```osprey
fn djb2(bytes) = bytes |> fold(5381, fn(h, b) => h * 33 + b)

let digest = handle Arith
    overflow _ _ _ wrapped => wrapped
do djb2(payload)
```

```osprey-ml
djb2 bytes = bytes |> fold 5381 (fn (h, b) => h * 33 + b)

digest =
    handle Arith
        overflow _ _ _ wrapped => wrapped
    in djb2 payload
```

An unhandled arithmetic operation is a compile error naming the effect and operation, not a runtime surprise:

```text
unhandled effect operations at program entry: Arith.overflow; add a matching handle
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
