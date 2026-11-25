# Error Handling

Every operation that can fail returns a `Result<T, E>`. Osprey has no exceptions, no panics and no hidden failure channel: `Result<T, E>` is the one type failure travels in, for the language, the standard library and every program. The only other form is a statically handled algebraic effect, which arithmetic uses. A raw foreign status is ABI data and MUST be translated to a `Result` at the safe Osprey boundary.

Every flavor shares these semantics. Examples show Default and ML syntax.

## `Result<T, E>` — [ERR-RESULT]

```osprey
type Result<T, E> = Success { value: T } | Error { message: E }
```

`T` is the type of the value on success. `E` is the type of the value on failure. Both are type parameters, and they obey the same rules:

1. **Both are generic.** `T` and `E` may each be any type: a record, a union, a number, a `bool`, a `string`, a list, another `Result`. `Result` builds in no error type.
2. **Both are inferred.** `Success { value: v }` fixes `T` to the type of `v`; `Error { message: e }` fixes `E` to the type of `e`. No annotation is needed. A side the program never builds stays open until a caller decides it.
3. **Both are exact — [ERR-PAYLOAD].** Matching `Success { value }` binds `value : T`, and matching `Error { message }` binds `message : E`: the very value the producer wrote, never a rendering, placeholder or default, for every operator, builtin and user function on every backend and memory mode.
4. **Both must agree.** Two `Result`s are the same type only when their `T`s agree and their `E`s agree. A `Result<int, ParseError>` is not a `Result<int, MathError>`.
5. **Neither is unwrapped implicitly.** A `Result<T, E>` never becomes a `T`; callers `match` it or supply a fallback with `?:`. [Result Preservation](0004-TypeSystem.md#result-preservation) lists where this applies, and [Result Default](0007-PatternMatching.md#result-default---pattern-result-default) defines `?:`.
6. **Neither is dropped.** See [ERROR-RESULT-DISCARD](#discarding-a-result--error-result-discard) for statements and [TYPE-RECORD-RESULT](0004-TypeSystem.md#result-fields--type-record-result) for fields.

There is no built-in error type. A fallible function, built-in or user-written, states the `E` its failure needs: `string` when the failure is only a message, a record such as `IndexOutOfBounds { index: int, length: int }` when it carries several values, a union when it has several cases. Built-ins follow it: a list read fails with [`IndexOutOfBounds`](../runtime/0012-Built-InFunctions.md#listgetlist-listt-index-int---resultt-indexoutofbounds--builtin-list-get), every other fallible built-in with a `string`.

```osprey
type ParseError = { code: int, why: string }
type MathError = DivZero | Overflow { by: int }

fn parse(s) = if s == "" { Error { message: ParseError { code: 1, why: "empty" } } } else { Success { value: length(s) } }
// inferred: fn parse(s: string) -> Result<int, ParseError>

fn ratio(n) = if n == 0 { Error { message: DivZero } } else { Success { value: n } }
// inferred: fn ratio(n: int) -> Result<int, MathError>

fn code(n) = if n < 0 { Error { message: n } } else { Success { value: n } }
// inferred: fn code(n: int) -> Result<int, int>

let shown = match parse("") {
    Success { value }   => "ok ${value}"
    Error   { message } => "parse ${message.code}: ${message.why}"
}
let size = parse("abc") ?: 0
```

```osprey-ml
type ParseError =
    code : int
    why : string

parse "" = Error(message = ParseError(code = 1, why = "empty"))
parse s  = Success(value = length s)

shown = match parse ""
    Success value   => "ok ${value}"
    Error   message => "parse ${message.code}: ${message.why}"
size = parse "abc" ?: 0
```

### Formatting — [ERR-RESULT-FORMAT]

A `Result` formats as `Success(<value>)` or `Error(<payload>)`, each side rendered at its own type as `toString` renders it ([BUILTIN-PRINT-FORM](../runtime/0012-Built-InFunctions.md#printvalue-printable---unit--builtin-print)): a record payload as `Error({ code: 1, why: "empty" })`, a union payload as `Error(Overflow { by: 3 })`. It is printable exactly when both `T` and `E` are.

```osprey
print(toString(checkedAdd(2, 3)))   // "Success(5)"
print(toString(checkedMul(4294967296, 4294967296)))  // "Error(integer overflow)"
print(toString(code(-7)))           // "Error(-7)"
```

```osprey-ml
print (toString (checkedAdd (2, 3)))   // "Success(5)"
print (toString (checkedMul (4294967296, 4294967296)))  // "Error(integer overflow)"
print (toString (code (-7)))           // "Error(-7)"
```

## Arithmetic — [ARITH-CHECKED]

**Arithmetic cannot fail silently.** A total operation produces a defined value of its static type. A fallible operation transfers to a required `Arith` handler, whose value arm supplies the typed result on normal completion. The operation MUST NOT trap, panic, wrap silently, or produce an unspecified value, and a fault that cannot be proven impossible MUST be discharged — or the program is rejected at compile time. The clauses and conformance obligations are [ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total). Host execution invokes the policy; current Metal/OpenCL kernels abort on faults instead, an unfinished [device-recovery requirement](0037-ArithmeticEffects.md#device-recovery-arith-device-recovery).

Integer `+`, `-`, `*`, `%`, unary `-`, and `abs` have type `int`; `/` has type `float`. There is no `Result` in an arithmetic type. A fallible operation requires an `Arith` handler, whose normal value-arm completion substitutes a defined value of that type.

An operation whose mathematical result is unrepresentable — integer overflow, a zero divisor — performs an operation of the compiler-declared `Arith` effect, and the handler the region installed substitutes the value its policy chooses. The full guarantee, the operation signatures, and the policy forms are [Arithmetic Effects](0037-ArithmeticEffects.md).

| Operator | int, int | float, float | int, float / float, int |
| --- | --- | --- | --- |
| `+ - *` | `int` | `float` | `float` (int promoted before operation) |
| `/` | `float` | `float` | `float` |
| `%` | `int` | `float` | `float` (int promoted) |

`/` always yields `float`. Unary integer `-` has type `int -> int`; unary float `-` has type `float -> float`. Floating-point `+`, `-`, `*`, and unary `-` are plain IEEE-754 operations; infinity, NaN, signed zero and gradual underflow follow [FLOAT-IEEE-RESULTS](0037-ArithmeticEffects.md#floating-point-results--float-ieee-results). Float `/` and `%` request `Arith.divideByZero` for either signed zero divisor, even with a NaN numerator. Integer `-9223372036854775808 % -1` yields `0`; the representable remainder is produced without executing LLVM's faulting `srem` case. The builtins `checkedAdd`, `checkedSub`, and `checkedMul` return `Result<int, string>` and are the explicit value-level spelling for code that wants overflow as data.

```osprey
let sum       = 1 + 3      // int
let quotient  = 10 / 3     // float
let remainder = 10 % 3     // int
let mixed     = 10 + 5.5   // float
let checked   = checkedAdd(a: 1, b: 3)   // Result<int, string>
```

```osprey-ml
sum       = 1 + 3      // int
quotient  = 10 / 3     // float
remainder = 10 % 3     // int
mixed     = 10 + 5.5   // float
checked   = checkedAdd (1, 3)   // Result<int, string>
```

### Negated Literals — [ARITH-NEG-LITERAL]

A negated numeric *literal* is folded at parse time into a literal of the opposite sign, so `-1` has type `int` and `-1.5` has type `float`. The fold is total: the one overflowing case, `-(-9223372036854775808)`, is reachable only by double negation and performs `Arith.overflow` like any other unrepresentable negation.

### Arithmetic policies

A region states its arithmetic policy once, in a handler, instead of at every operation. Wrapping consumes the operation's two's-complement payload; a fault-recording policy writes handler-owned state and lets the boundary decide.

[Arithmetic policies](0037-ArithmeticEffects.md#policies) gives the callable-handler forms.

An unhandled arithmetic operation is a compile error naming the effect and operation, not a runtime surprise:

```text
unhandled effect operations at program entry: Arith.overflow; add a matching `handle`
```


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
([BLOCK-DISCARD](0008-BlockExpressions.md#discarded-values-block-discard)), but
it does not discharge a `Result`. Handle the failure with `match` or `?:`, or
return it to a caller who will.

## Sequencing Results with `andThen` — [BLOCK-USE-RESULT]

`andThen(result, next)` is the one bind for `Result`, from the prelude. A `Success`
passes its value to `next`; an `Error` comes back unchanged, payload and all
([ERR-PAYLOAD]). It is what a `use` line names
([BLOCK-USE](0008-BlockExpressions.md#flat-binding-with-use-block-use)), so a
chain of fallible steps stops at the first failure and reports that failure:

```osprey
fn total(a, b) = {
    use x <- andThen(parseInt(a))
    use y <- andThen(parseInt(b))
    Success { value: x + y }
}
```

A program that defines its own `andThen` keeps its own.
