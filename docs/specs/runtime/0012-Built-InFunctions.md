# Built-in Functions

Reference for built-in functions available in every Osprey program. Fallible
language operations use `Result` or a statically handled effect. Raw foreign
status values must be translated at a safe Osprey boundary. See
[Error Handling](../language/0013-ErrorHandling.md).

Built-ins are shared by both language flavors. Examples use the Default
surface unless an ML example clarifies different call syntax.

## Basic I/O Functions

### `print(value: printable) -> Unit` — [BUILTIN-PRINT]

`printable` is documentation shorthand, not a user-declared type. It includes
`int`, `float`, `bool`, `string`, `Unit`, explicitly erased `any`, and
`Result<T, E>` when both payloads are printable. Concrete records, collections, functions,
iterators, fibers, channels, and pointers are rejected. Results render as
`Success(value)` or `Error(message)`; `Unit` renders as `0`; `print` appends a
newline. An explicitly erased `any` value renders through its shape descriptor: records show their named fields, unions their selected variant and Results their success/error case. Opaque runtime handles use kind labels; the underlying pointer word is never interpreted as a string or shown as the value. An explicit `any` local may retain a concrete backend representation until rendering, which must produce the same result as a boxed `any` parameter. See [TYPE-ANY](../language/0004-TypeSystem.md#the-any-type--type-any).

```osprey
print("Hello World")
print(42)
print(true)
```

```osprey-ml
print "Hello World"
print 42
print true
```

### `input() -> string` — [BUILTIN-INPUT]
Reads one line from standard input (without its trailing newline) and returns it
as a string. End-of-file ends a line just as a newline does, and at end-of-file
with nothing read — including when stdin is closed or not connected — the result
is the empty string `""` rather than a failure. Elapsed silence is not
end-of-file: a producer that is connected but has not written yet is waited for,
so a slow writer's line still arrives whole. A launcher that cannot supply input
must close the child's standard input. Parse the line with
`parseInt`/`parseFloat` when a number is wanted.

```osprey
let line = input()                 // "" if there is no input
let n    = parseInt(input()) ?: 0  // a number, or 0 when absent/unparseable
```

```osprey-ml
line = input ()                    // "" if there is no input
n =
    match parseInt (input ())      // a number, or 0 when absent/unparseable
        Success value => value
        Error _       => 0
```

### `toString(value: printable) -> string` — [BUILTIN-TOSTRING]
Uses the same accepted types and formatting as `print`, without writing output.

## Testing Functions

```osprey
test(name: string, body: fn() -> a) -> Unit
expect(actual: any, expected: any) -> Unit
check(label: string, expected: any, actual: any) -> Unit
```

Their behavior, shadowing rule, TAP output, filtering, discovery, coverage, and
editor integration are specified in [Testing Framework](../language/0027-TestingFramework.md).

## Numeric Functions

The numeric builtins are inside the arithmetic totality guarantee: none may trap, panic, wrap silently, or return an unspecified value ([ARITH-TOTAL](../language/0037-ArithmeticEffects.md#the-guarantee--arith-total)). `abs` preserves the input numeric type and `intDiv` returns `int`; an unrepresentable integer result performs an `Arith` operation whose handler supplies the value ([ARITH-EFFECT](../language/0037-ArithmeticEffects.md#the-model--arith-effect)). `checkedAdd`/`checkedSub`/`checkedMul` are the explicit value-level form for code that wants overflow as data, and keep the runtime's generic `Error` channel. The wrapping and saturating helpers are total and perform nothing.

### `abs(n: T) -> T`, where `T` is `int` or `float` — [BUILTIN-ABS]
Returns the absolute value with the input's numeric type. Because `2^63` is not representable, `abs(-9223372036854775808)` performs `Arith.overflow` and the enclosing policy supplies the result; it never wraps or panics. A constant integer argument other than the minimum is total.

For a float, `abs` clears the sign: `abs(-0.0)` is positive zero, either infinity becomes positive infinity, and NaN remains NaN. This overload is total and requires no `Arith` policy. Both overloads remain usable through function values, including aliases, callbacks and returned or stored functions. Numeric overload inference follows the existing arithmetic rule: a helper or alias with an unresolved numeric type retains one overload selected by its uses; separate builtin uses can select different numeric types. Nonnumeric arguments and Result wrappers are rejected. See [FLOAT-IEEE-RESULTS](../language/0037-ArithmeticEffects.md#floating-point-results--float-ieee-results).

### `intDiv(a: int, b: int) -> int` — [BUILTIN-INTDIV]
Truncates toward zero. A zero divisor performs `Arith.remainderByZero`, and `intDiv(-9223372036854775808, -1)` performs `Arith.overflow`; every other input yields the quotient. A literal divisor other than `0` and `-1` is total, so `half` below needs no handler, while `ratio` requires an `Arith` handler for both operations. The `/` operator instead returns `float`.

```osprey
intDiv(7, 2)        // 3
intDiv(255643, 10)  // 25564
fn half(n) = intDiv(n, 2)
fn ratio(a, b) = intDiv(a, b)
```

```osprey-ml
intDiv (7, 2)        // 3
intDiv (255643, 10)  // 25564
half n = intDiv (n, 2)
ratio (a, b) = intDiv (a, b)
```

### `wrapAdd` / `wrapSub` / `wrapMul` / `satAdd` / `satSub` / `satMul` — [BUILTIN-TOTAL-ARITH]
Each has signature `(a: int, b: int) -> int` and never performs an `Arith` operation. The `wrap` helpers return the two's-complement result; the `sat` helpers clamp to the `int` range. They are the sanctioned spelling for code where wraparound or clamping is the definition rather than a fault — hashes, checksums, PRNGs — and for `Arith` arms that must not require an outer policy ([ARITH-EFFECT-TOTAL-HELPERS](../language/0037-ArithmeticEffects.md#total-helpers--arith-effect-total-helpers)).

```osprey
wrapAdd(9223372036854775807, 1)   // -9223372036854775808
wrapMul(4294967296, 4294967296)   // 0
satAdd(9223372036854775807, 1)    // 9223372036854775807
satSub(-9223372036854775808, 1)   // -9223372036854775808
```

```osprey-ml
wrapAdd (9223372036854775807, 1)   // -9223372036854775808
wrapMul (4294967296, 4294967296)   // 0
satAdd (9223372036854775807, 1)    // 9223372036854775807
satSub (-9223372036854775808, 1)   // -9223372036854775808
```

### `toFloat(n: int) -> float` — [BUILTIN-TOFLOAT]
Widens an integer to the nearest `double`, rounding ties to even. Exact for
`|n| <= 2^53`; larger magnitudes round, which is the IEEE-754 result and not
an error. The conversion is total — every `int` has a nearest `double` — so
it returns a bare `float`, not a `Result`.

This is the explicit element conversion GPU kernels use; a buffer never
widens implicitly at its boundary ([GPU-CONVERT](0034-GPUComputation.md)).
It may be passed by name as a kernel, not only called directly.

```osprey
toFloat(7)                        // 7.0
toFloat(-3)                       // -3.0
gpuIota(1000) |> gpuMap(toFloat)  // GpuBuffer<float>, 0.0 .. 999.0
```

### `sqrt` / `exp` / `ln` / `tanh` — [BUILTIN-MATH]
Each has signature `(x: float) -> float` and is total over IEEE doubles, so
it returns a bare `float` and never performs an `Arith` operation
([FLOAT-IEEE-RESULTS](../language/0037-ArithmeticEffects.md#floating-point-results--float-ieee-results)):
a domain error yields NaN (`sqrt(-1.0)`, `ln(-1.0)`), `ln(0.0)` is negative
infinity, an `exp` too large for a double is positive infinity, and `tanh`
stays within `[-1.0, 1.0]`. The natural logarithm is spelled `ln`, not `log`,
so a program keeps `log` for its own logger. `sqrt`, `exp`, `ln` and `tanh`
are reserved builtin names like every other builtin: a program may not define
its own function under one of them (`cannot redefine built-in function`).

The argument is a `float` and nothing else. There is no implicit widening:
`exp(3)` is a type error (`cannot unify float with int`), and the program
writes `exp(toFloat(3))`. Each function is pure, so a GPU kernel may call it
or *be* it, passed by name ([GPU-KERNEL-FORM](0034-GPUComputation.md)):
`scores |> gpuMap(exp)` extracts a kernel whose body is the function.

Lowering: `sqrt`, `exp` and `ln` are the LLVM `llvm.sqrt.f64`,
`llvm.exp.f64` and `llvm.log.f64` intrinsics; `tanh` is the C library's
`tanh`. A program whose IR references `exp`, `ln`, `tanh` or the float `%`
operator is linked against `libm`.

```osprey
sqrt(2.0)                       // 1.4142135623730951
fn gelu(x) = 0.5 * x * (1.0 + tanh(0.7978845608 * (x + 0.044715 * x * x * x)))
let probabilities = scores |> gpuMap(exp)
```

```osprey-ml
sqrt 2.0                        // 1.4142135623730951
gelu x = 0.5 * x * (1.0 + tanh (0.7978845608 * (x + 0.044715 * x * x * x)))
probabilities = scores |> gpuMap exp
```

### Trigonometric functions — [BUILTIN-MATH-TRIG]
`sin`, `cos`, `tan`, `asin`, `acos`, `atan` and the two-argument `atan2`
extend [BUILTIN-MATH] with the elementary trigonometric functions. Each
unary one has signature `(x: float) -> float`; `atan2` is
`(y: float, x: float) -> float`. All are total over IEEE doubles and return a
bare `float`, never an `Arith` operation ([FLOAT-IEEE-RESULTS]): `asin` and
`acos` of an argument outside `[-1.0, 1.0]` yield NaN, `tan` at a pole yields
the large finite neighbour (the argument is an approximation of π/2, so the
exact pole is unreachable). `atan2(y, x)` returns the angle of `(x, y)` in `[-π, π]`, including signed-zero quadrants: `atan2(0.0, 0.0)` is `0.0`, `atan2(±0.0, -1.0)` is `±π`, and `atan2(±0.0, 1.0)` is `±0.0`. Each function can be called inside a GPU kernel or passed by name, as in `gpuMap(sin)` ([GPU-KERNEL-FORM]).

Lowering: `sin` and `cos` are the LLVM `llvm.sin.f64` and `llvm.cos.f64`
intrinsics; `tan`, `asin`, `acos`, `atan` and `atan2` are the C library's
symbols, like `tanh`. A program whose IR references any of them is linked
against `libm`.

```osprey
sin(1.0)     // 0.8414709848078965
cos(1.0)     // 0.5403023058681398
atan2(1.0, -1.0)  // 2.356194490192345
```

### Hyperbolic functions — [BUILTIN-MATH-HYPERBOLIC]
`sinh`, `cosh`, `asinh`, `acosh` and `atanh` complete the hyperbolic family
started by `tanh` in [BUILTIN-MATH]. Each has signature
`(x: float) -> float`, is total over IEEE doubles and returns a bare `float`:
`sinh` and `cosh` of a large argument yield infinity, `acosh` of an argument
below `1.0` yields NaN, `atanh` of a magnitude above `1.0` yields NaN and
`atanh(±1.0)` yields `±infinity`. Pure, so a kernel may call or name them.

Lowering: all five are the C library's symbols. Their IR references link the
program against `libm`.

```osprey
sinh(1.0)    // 1.1752011936438014
cosh(1.0)    // 1.5430806348152437
asinh(1.0)   // 0.881373587019543
```

### Logarithmic and exponential families — [BUILTIN-MATH-LOG]
`log2`, `log10`, `log1p` and `expm1` extend [BUILTIN-MATH] with the base-2
and base-10 logarithms and the cancellation-avoiding companions of `ln` and
`exp`. Each has signature `(x: float) -> float` and is total: `log2(0.0)` and
`log10(0.0)` are negative infinity, `log2`/`log10` of a negative argument are
NaN, `log1p(-1.0)` is negative infinity, `log1p` below `-1.0` is NaN, and
`expm1` of a large argument is infinity. `log1p(x)` is `ln(1.0 + x)` computed
without the cancellation that makes `ln(1.0 + 1e-16)` equal `0.0`;
`expm1(x)` is `e^x - 1.0` computed without the cancellation that makes
`exp(1e-16) - 1.0` equal `0.0`. Pure, so a kernel may call or name them.

Lowering: `log2` and `log10` are the LLVM `llvm.log2.f64` and
`llvm.log10.f64` intrinsics; `log1p` and `expm1` are the C library's symbols.
Their IR references link the program against `libm`.

```osprey
log2(8.0)     // 3.0
log10(1000.0) // 3.0
log1p(1e-16)  // 1e-16
```

### Error functions — [BUILTIN-MATH-SPECIAL]
`erf` and `erfc` are the Gaussian error function and its complement. Each has
signature `(x: float) -> float` and is total: `erf(±infinity)` is `±1.0`,
`erfc(+infinity)` is `0.0`, `erfc(-infinity)` is `2.0`. `erfc` is computed
as `1.0 - erf(x)` without cancellation, so the tail keeps its relative
precision. Pure, so a kernel may call or name them. Both are the C library's
symbols; their IR references link the program against `libm`.

```osprey
erf(1.0)   // 0.8427007929497148
erfc(1.0)  // 0.15729920705028516
```

### Rounding and floating-point classification — [BUILTIN-MATH-ROUNDING]
`floor`, `ceil`, `trunc` and `round` have signature `(x: float) -> float`;
`pow`, `atan2` (see [BUILTIN-MATH-TRIG]), `hypot`, `fmod` and `copysign`
have signature `(x: float, y: float) -> float`. All are total and pure, so a
kernel may call or name them.

- `floor` is the greatest integer `<= x`, `ceil` the least `>= x`, `trunc`
  the integer part toward zero. Each returns a `float`, so
  `floor(-2.7)` is `-3.0`. `floor`/`ceil`/`trunc` of an infinity or NaN is
  that value itself; a signed zero is preserved.
- `round` rounds to the nearest integer, with **halfway cases rounded to
  even** — Python's `round` and numpy's rounding, never C's
  half-away-from-zero. `round(2.5)` is `2.0`, `round(3.5)` is `4.0`,
  `round(-2.5)` is `-2.0`.
- `pow(x, y)` is C `pow`: `pow(0.0, 0.0)` is `1.0`, a negative base needs an
  integer exponent (else NaN), overflow is infinity, underflow is zero. The
  total contract means it never performs the `Arith` division/overflow
  policies; the result is an IEEE value.
- `hypot(x, y)` is `sqrt(x*x + y*y)` without intermediate overflow or
  underflow: `hypot(3.0, 4.0)` is `5.0`, `hypot(1e308, 1e308)` is about
  `1.414e308` rather than infinity.
- `fmod(x, y)` is C `fmod`: the remainder of `x/y` truncated toward zero,
  with `x`'s sign. `fmod(-7.5, 2.0)` is `-1.5`. A zero divisor yields NaN —
  unlike integer `%`, which performs `Arith.remainderByZero`.
- `copysign(x, y)` is `x`'s magnitude with `y`'s sign: `copysign(3.0, -1.0)`
  is `-3.0`. A NaN keeps its payload and takes `y`'s sign bit.

Like every builtin, `round` is a reserved name: a program may not define its
own function under it (`cannot redefine built-in function`).

Lowering: `floor`, `ceil`, `trunc` and `round` are the LLVM `llvm.floor.f64`,
`llvm.ceil.f64`, `llvm.trunc.f64` and `llvm.roundeven.f64` intrinsics
(`roundeven` is what makes halfway cases round to even); `pow` and `copysign`
are the LLVM `llvm.pow.f64` and `llvm.copysign.f64` intrinsics; `hypot` is the
C library's symbol; `fmod` is the `frem` instruction itself, which for doubles
*is* C `fmod`. Their IR references link the program against `libm`.

```osprey
floor(-2.7)      // -3.0
round(2.5)       // 2.0
pow(2.0, 10.0)   // 1024.0
fmod(-7.5, 2.0)  // -1.5
copysign(3.0, -1.0)  // -3.0
```

### `float32(x: float) -> float` — [BUILTIN-FLOAT32]
Rounds a double to the nearest binary32 (ties to even) and widens back, so the
result is exactly the value a float32 pipeline holds: `float32(0.1)` is
`0.10000000149011612`. Total and pure, so a kernel may call or name it.

Arithmetic outside the conversion still uses binary64. When porting a float32 pipeline, apply `float32` at its rounding boundaries and compare the results with the original implementation; the conversion alone does not establish bit-for-bit agreement for the complete calculation. Lowering is one `fptrunc double … to float` and one `fpext float … to double`.

### `floatBits` / `floatFromBits` / `float32Bits` / `float32FromBits` — [BUILTIN-FLOAT-BITS]
Bit-level views of the two float formats, as `int` words. All are total and
pure.

- `floatBits(x: float) -> int` is the binary64 word of `x`: a bitcast, so it
  preserves NaN payloads, signed zeros and infinities exactly.
  `floatBits(1.0)` is `4607182418800017408`.
- `floatFromBits(bits: int) -> float` is the binary64 the word `bits` holds.
  It is the exact inverse of `floatBits`.
- `float32Bits(x: float) -> int` is the binary32 pattern of the nearest
  float32 to `x` (the low 32 bits of the float32 word), as an int.
  `float32Bits(0.1)` is `1036831949`.
- `float32FromBits(bits: int) -> float` is the double the binary32 pattern
  `bits` holds, widened exactly — the inverse of `float32Bits` on float32
  values.

### `floatToInt(x: float) -> Result<int, Error>` — [BUILTIN-FLOATTOINT]
Checked truncation toward zero. `Success(n)` when `x` is finite and `n` fits
in the `int` range (`-2^63 <= n < 2^63`); `Error` with a clear message when
`x` is NaN, an infinity, or a finite value whose truncation falls outside the
range. `floatToInt(-2.9)` is `Success(-2)`; `floatToInt(sqrt(-1.0))` is `Error`. Because it returns a `Result`, it cannot occupy a GPU combinator's scalar return slot. Convert on the host before creating a buffer, or keep a rounded value as a float with `trunc`.

### Bitwise integer builtins — [BUILTIN-BITWISE]
`bitAnd`, `bitOr`, `bitXor` (`(a: int, b: int) -> int`), `bitNot`
(`(a: int) -> int`), and the three shifts `shiftLeft`, `shiftRight`,
`shiftRightLogical` (`(a: int, n: int) -> int`) are the two's-complement word
operations. None of them performs an `Arith` operation and none of them can
fault; the shifts are **total** over every shift count, defined as the limit
of repeated shifting:

- `shiftLeft(a, n)` and `shiftRightLogical(a, n)` are `0` for a count outside
  `0..63`.
- `shiftRight(a, n)` is the arithmetic shift: `0` for `a >= 0` and `-1` for
  `a < 0` when the count is outside `0..63`.

`shiftRightLogical(-16, 2)` is `4611686018427387900` (the bits move, the sign
is not copied); `shiftRight(-16, 2)` is `-4` (the sign is copied). All are
pure, so a kernel may call or name them; the device forms are the Metal/OpenCL
long operations with the same total-clamp contract.

```osprey
bitAnd(12, 10)             // 8
bitOr(12, 10)              // 14
bitXor(12, 10)             // 6
bitNot(12)                 // -13
shiftLeft(1, 40)           // 1099511627776
shiftRight(-16, 2)         // -4
shiftRightLogical(-16, 2)  // 4611686018427387900
```

### `checkedAdd` / `checkedSub` / `checkedMul` — [BUILTIN-CHECKED-ARITH]
Each has signature `(a: int, b: int) -> Result<int, Error>`. Overflow-checked
integer addition, subtraction, and multiplication, lowering to
`llvm.sadd.with.overflow`, `llvm.ssub.with.overflow`, and
`llvm.smul.with.overflow` respectively. An overflowing operation returns
`Error`; otherwise `Success(result)`. They are the value-level form of overflow checking, for code that wants the failure as data rather than as an `Arith` operation ([ARITH-CHECKED](../language/0013-ErrorHandling.md#arithmetic--arith-checked)). Their `Success` payload is never implicitly unwrapped.

```osprey
checkedAdd(2, 3)                      // Success(5)
checkedMul(4294967296, 4294967296)    // Error — "integer overflow"
fn twice(n) -> Result<int, Error> = checkedMul(n, 2)
```

```osprey-ml
checkedAdd (2, 3)                      // Success(5)
checkedMul (4294967296, 4294967296)    // Error — "integer overflow"

twice : int -> Result<int, Error>
twice n = checkedMul (n, 2)
```

### `random() -> int` — [BUILTIN-RANDOM]
A cryptographically-secure uniform random non-negative integer in `[0, 2^63-1]`,
drawn fresh from the operating system's CSPRNG (`arc4random_buf` on macOS/BSD,
`getrandom(2)` on Linux, falling back to `/dev/urandom`, `rand_s` on Windows,
`getentropy` on wasm). It carries no userspace seed or state, so calls are not
reproducible.

A draw either carries OS entropy or the process stops. `random()` answers `int`,
not `Result<int, Error>`, so a source that cannot supply the bytes has nowhere to
report — and a predictable value returned from this call is undetectable by the
caller and worst exactly where it matters, since the WebSocket handshake nonce
draws from it too. When no source can fill the request the runtime prints
`FATAL: the OS entropy source gave <n> of the <m> bytes random() needs` and
aborts; it never substitutes zeros or leaves the caller's word unwritten. On the
supported platforms no source fails, so this is unreachable in practice and is
reached in the test suite only by taking the source away.

```osprey
let token = random()        // e.g. 7240982340198 (varies every call)
fn coinFlip() = randomBelow(2) ?: 0   // 0 or 1
```

```osprey-ml
token = random ()        // e.g. 7240982340198 (varies every call)
coinFlip () =
    match randomBelow 2   // 0 or 1
        Success value => value
        Error _       => 0
```

### `randomBelow(n: int) -> Result<int, Error>` — [BUILTIN-RANDOM-BELOW]
A cryptographically-secure uniform random integer in the half-open range
`[0, n)`. The result is **unbiased**: it is drawn by rejection sampling, so every
value in the range is equally likely (a plain `random() % n` is not). A
non-positive `n` returns `Error`; otherwise `Success(value)` with
`0 <= value < n`. Compose for an arbitrary range: `lo + (randomBelowhi - lo)`.

```osprey
let die = randomBelow(6) ?: 0          // a fair face 0..5
match randomBelow(0) { Success { value } => value  Error { message } => 0 - 1 }  // Error
```

```osprey-ml
die =
    match randomBelow 6           // a fair face 0..5
        Success value => value
        Error _       => 0
match randomBelow 0
    Success value => value
    Error message => 0 - 1       // Error
```

## Structural Comparison — [BUILTIN-STRUCTURAL-EQUALS]

### `structuralEquals(a: T, b: T) -> bool`

`structuralEquals` compares two values by their contents. It walks both values
completely, so its cost grows with their size. It is meant to be rare and
explicit. `==` deliberately does not do this
([BOOL-EQUALITY](../language/0009-BooleanOperations.md#what--and--compare-bool-equality)).
Before reaching for it, compare a key, an id or a revision field instead,
which answers most "did this change?" questions in constant time.

Two values are equal when:

| Type | Equal when |
| --- | --- |
| `int`, `string`, `bool` | `==` says so |
| `float` | `==` says so, so NaN is unequal to itself and a value holding NaN is unequal to itself |
| a record or tuple | every field is equal |
| a union | both are the same variant and every payload field is equal |
| `List<T>` | both have the same length and equal elements in the same order |
| `Map<string, V>` | both have the same keys and equal values for each key |

`T` must be a type whose every part has a decided equality. A type that
contains a function, a runtime handle (a fiber, a channel, an HTTP or file
handle, a GPU buffer), `any` or `Result` is a compile error. A function's
equality is not decided, and a handle names an external resource, so neither
has contents to compare. As with `==`, a generic caller carries the
requirement to each call site, which is checked at its own type.

```osprey
type Route = { tab: string, depth: int }
let same = structuralEquals(Route { tab: "home", depth: 1 }, Route { tab: "home", depth: 1 })   // true
let differ = structuralEquals([1, 2], [1, 2, 3])                                                 // false
```

The Default/ML twins `tests/regressions/basics/operators/structural_equals.test.*`
pin every row; `examples/failscompilation/structural_equals_*.ospo` pin the
rejected types.

## String Functions

Strings are immutable, NUL-terminated UTF-8 byte sequences. String operations
return new values and do not mutate their arguments.

### Rules

Total operations return plain values; invalid indices, arguments, or parses
return `Result`. The subject is the first argument so calls compose with `|>`.
Except for the explicit UTF-8 cursor functions, lengths and indices are byte
based. Case conversion and whitespace handling cover ASCII.

### Calling Style — [BUILTIN-STRING-UFCS]

String functions support pipe, direct-call, and Default-flavor method syntax.

```osprey
// Pipe chain
"  Hello, World  " |> trim |> toLowerCase |> split(", ")

// Direct call
toLowerCase(trim("  Hello  "))

// Method-call (UFCS) — sugar, equivalent to the direct form
"  Hello  ".trim().toLowerCase()
```

```osprey-ml
// Pipe chain
"  Hello, World  " |> trim |> toLowerCase |> split ", "

// Direct call
toLowerCase (trim "  Hello  ")

// Chained UFCS (`.trim().toLowerCase()`) has no ML surface; use the pipe form:
"  Hello  " |> trim |> toLowerCase
```

All three desugar to the same call. Rules:

- **Pipe (`x |> f`)** rewrites to `f(x)`. With extra args, `x |> f(a, b)` becomes `f(x, a, b)`. A bare identifier on the right (`x |> f`) is auto-promoted to a call — no parens needed for single-arg functions. See [Iterators](../language/0010-LoopConstructsAndFunctionalIterators.md#pipe-operator--builtin-iter-pipe).
- **UFCS (`x.f(args)`)** rewrites to `f(x, args)`. **Parens are required** to disambiguate from field access — `x.f` always means field access, never a method call. If a record has a field named `f`, field access wins; UFCS is the fallback.
- **Direct call** is ordinary function application.

For dotted calls, the receiver's resolved type determines the choice. A
declared field named `f` is selected even when a free function with that name
is in scope. The field value must be callable; a non-callable field is an
error and does not enable fallback. Calling the field passes only the written
arguments. UFCS passes the receiver first, followed by the written arguments,
and is considered only when the receiver has no such field.

This rule also applies inside generic functions. If the receiver is still a
type variable when the body is checked, selection remains deferred and is
resolved for each instantiation. For example, `dispatch<T>(x:T) = x.m()` may
select a record's `m` field for one call and a free `m(x)` for a scalar call.
The receiver, argument, result, and callback type constraints remain linked
through generalization and instantiation. An annotation cannot be required
solely to compensate for losing those links.

Written type arguments apply to the selected callable's declaration binders.
A function-valued field has no declaration binders, so `record.f<int>()` is
rejected even if an in-scope free `f` declares a type parameter. The selected
callable also determines the call's effect requirements: an ignored free
function contributes none, and selecting a field cannot discard its effects.
Evaluation of the receiver and written argument expressions still contributes
their own effects exactly once.

Multi-argument functions in this spec are documented subject-first (e.g. `split(s: string, separator: string)`) so all three forms work uniformly.

### Inspection (total) — [BUILTIN-STRING-INSPECTION]

#### `length(s: string) -> int` — [BUILTIN-STRING-LENGTH]
Returns the number of bytes. It is equivalent to `byteLength` for strings.

#### `isEmpty(s: string) -> bool` — [BUILTIN-STRING-ISEMPTY]
True iff `length(s) == 0`. Equivalent to `length(s) == 0` but constant-time.
The same names accept `List<T>` and `Map<string, V>` as described under
[Collection Functions](#collection-functions); no other receiver type is
accepted.

### Search (total) — [BUILTIN-STRING-SEARCH]

#### `contains(s: string, needle: string) -> bool` — [BUILTIN-STRING-CONTAINS]
True if `needle` occurs anywhere in `s`. An empty `needle` returns `true`.

```osprey
contains("hello world", "world")  // true
contains("hello", "")             // true
```

```osprey-ml
contains ("hello world", "world")  // true
contains ("hello", "")             // true
```

#### `startsWith(s: string, prefix: string) -> bool` — [BUILTIN-STRING-STARTSWITH]
#### `endsWith(s: string, suffix: string) -> bool` — [BUILTIN-STRING-ENDSWITH]

```osprey
"GET /api/users" |> startsWith("GET ")   // true
"image.png"      |> endsWith(".png")     // true
```

```osprey-ml
"GET /api/users" |> startsWith "GET "   // true
"image.png"      |> endsWith ".png"     // true
```

#### `indexOf(s: string, needle: string) -> Result<int, Error>` — [BUILTIN-STRING-INDEXOF]
Returns the byte index of the first occurrence of `needle`, or
`Error` with `"indexOf: substring not found"` if absent. An empty `needle`
returns `Success { value: 0 }`.

### Cursor Access — [BUILTIN-STRING-CURSOR]

These primitives give indexed access to UTF-8 bytes and codepoints. Only
`byteLength` is total; `byteAt`, `codePointAt`, and `codePointWidth` are
fallible and return `Result` on a bad index, a non-boundary index, or malformed
UTF-8. Only `fromCodePoint` allocates.

A runtime string is a bare NUL-terminated `char*` carrying no stored length, so
every entry point here begins with a `strlen` and is **O(n) in the string's byte
length**, not O(1). A scan that re-derives `byteLength` per step is therefore
quadratic. Making these O(1) requires a length-carrying string ABI.

#### `byteLength(s: string) -> int` — [BUILTIN-STRING-BYTELENGTH]
Byte length of the underlying UTF-8 storage. Equivalent to `length(s)`.

#### `byteAt(s: string, i: int) -> Result<int, Error>` — [BUILTIN-STRING-BYTEAT]
Returns the UTF-8 byte at index `i` as an `int` in `[0, 255]`, or `Error` if
`i < 0` or `i >= byteLength(s)`. Does **not** allocate.

#### `codePointAt(s: string, byteIndex: int) -> Result<int, Error>` — [BUILTIN-STRING-CODEPOINTAT]
Decodes the UTF-8 codepoint starting at `byteIndex` and returns it as an `int`.
Returns `Error` if the index is out of range, does not land on a codepoint
boundary, or begins a truncated, overlong, surrogate, out-of-range, or otherwise
malformed UTF-8 sequence. Decoding reads at most 4 bytes once the index is
bounds-checked. Pair with `codePointWidth` to advance:

```osprey
type CharStep = { codePoint: int, nextIndex: int }

fn nextChar(s, i) = match codePointAt(s, i) {
    Success { value: cp } => match codePointWidth(cp) {
        Success { value: w } => Success { value: CharStep { codePoint: cp, nextIndex: i + w } }
        Error   { message }  => Error { message }
    }
    Error { message } => Error { message }
}
```

```osprey-ml
type CharStep =
    codePoint : int
    nextIndex : int

nextChar (s, i) =
    match codePointAt (s, i)
        Success cp =>
            match codePointWidth cp
                Success w => Success(value = CharStep(codePoint = cp, nextIndex = i + w))
                Error message => Error(message = message)
        Error message => Error(message = message)
```

#### `codePointWidth(codepoint: int) -> Result<int, Error>` — [BUILTIN-STRING-CODEPOINTWIDTH]
Returns the number of UTF-8 bytes the codepoint encodes to (1–4), or `Error`
if `codepoint` is not a valid Unicode scalar value.

#### `fromCodePoint(codepoint: int) -> Result<string, Error>` — [BUILTIN-STRING-FROMCODEPOINT]
Builds a single-codepoint `string`. Inverse of `codePointAt`. Returns `Error`
for surrogates, values outside `0..0x10FFFF`, and U+0000, which the
NUL-terminated string ABI cannot represent.

### Substrings — [BUILTIN-STRING-SUBSTRINGS]

#### `substring(s: string, start: int, end: int) -> Result<string, Error>` — [BUILTIN-STRING-SUBSTRING]
Extracts bytes in `[start, end)`. Returns `Error` if
`start < 0`, `end > length(s)`, or `start > end`.

#### `take(s: string, n: int) -> string` — [BUILTIN-STRING-TAKE]
Returns at most the first `n` bytes. If `n <= 0`, returns `""`; if
`n >= length(s)`, returns `s`.

#### `drop(s: string, n: int) -> string` — [BUILTIN-STRING-DROP]
Returns `s` without its first `n` bytes, with the same clamping rules as
`take`.

### Splitting and Joining — [BUILTIN-STRING-LIST]

#### `split(s: string, separator: string) -> Result<List<string>, Error>` — [BUILTIN-STRING-SPLIT]
Splits `s` on every occurrence of `separator`. Returns `Error` with
`"split: separator must not be empty"` if `separator` is empty.

```osprey
match split("a,b,c", ",") {
    Success { value }   => forEachList(value, print)   // "a" "b" "c"
    Error   { message } => print("split error")
}
```

```osprey-ml
match split ("a,b,c", ",")
    Success value   => forEachList (value, print)   // "a" "b" "c"
    Error message   => print "split error"
```

#### `join(parts: List<string>, separator: string) -> string` — [BUILTIN-STRING-JOIN]
Concatenates `parts` with `separator` between each pair. Returns `""` if `parts` is empty. `parts` may be a list in either representation — a flat list literal or a runtime list built by `listAppend`, `toList`, `lines`, `words` or `split` — and the result is the same string.

#### `lines(s: string) -> List<string>` — [BUILTIN-STRING-LINES]
Splits on `"\n"`. A trailing newline does not produce an empty final element.

#### `words(s: string) -> List<string>` — [BUILTIN-STRING-WORDS]
Splits on runs of ASCII whitespace, dropping empty results.

### Transformation (total) — [BUILTIN-STRING-TRANSFORM]

#### `toUpperCase(s: string) -> string` — [BUILTIN-STRING-TOUPPERCASE]
#### `toLowerCase(s: string) -> string` — [BUILTIN-STRING-TOLOWERCASE]
ASCII case conversion. Other bytes are copied unchanged.

#### `trim(s: string) -> string` — [BUILTIN-STRING-TRIM]
#### `trimStart(s: string) -> string` — [BUILTIN-STRING-TRIMSTART]
#### `trimEnd(s: string) -> string` — [BUILTIN-STRING-TRIMEND]
Remove leading, trailing, or both runs of ASCII whitespace.

#### `replace(s: string, needle: string, replacement: string) -> Result<string, Error>` — [BUILTIN-STRING-REPLACE]
Replaces every occurrence of `needle` with `replacement`. Returns `Error` if
`needle` is empty.

#### `repeat(s: string, n: int) -> Result<string, Error>` — [BUILTIN-STRING-REPEAT]
Concatenates `s` with itself `n` times. Returns `Error` if `n < 0`.
`repeat(s, 0) == ""`.

#### `reverse(s: string) -> string` — [BUILTIN-STRING-REVERSE]
Reverses byte order.

#### `padStart(s: string, targetLength: int, fill: string) -> Result<string, Error>` — [BUILTIN-STRING-PADSTART]
#### `padEnd(s: string, targetLength: int, fill: string) -> Result<string, Error>` — [BUILTIN-STRING-PADEND]
Pads `s` on the left or right with repeated bytes from `fill` until it reaches
`targetLength` bytes. Returns `s` unchanged if already long enough and `Error`
if `fill` is empty.

### Parsing — [BUILTIN-STRING-PARSING]

#### `parseInt(s: string) -> Result<int, Error>` — [BUILTIN-STRING-PARSEINT]
Parses a base-10 signed integer. Leading/trailing whitespace is rejected;
callers must `trim` first. Returns `Error` on invalid or out-of-range input.

#### `parseFloat(s: string) -> Result<float, Error>` — [BUILTIN-STRING-PARSEFLOAT]
Parses a finite base-10 number with an optional sign, decimal point, and
decimal exponent. At least one digit is required; surrounding whitespace,
NaN, infinity, hexadecimal floats, malformed exponents, and non-finite results
return `Error`.

### Concatenation Operator — [BUILTIN-STRING-CONCAT]

The `+` operator on two `string` values returns `string` directly. String concatenation cannot fail and is never `Result`-wrapped.

```osprey
let greeting = "Hello, " + name + "!"
```

```osprey-ml
greeting = "Hello, " + name + "!"
```

## File System Functions — [BUILTIN-FILE]

### `writeFile(path: string, content: string) -> Result<int, Error>`
Writes or replaces a file and returns the number of bytes written. `Success`
means every byte reached the file: a partial write and a failed flush are both
`Error`. Buffering means the bytes leave for the file when it is closed, so a
full disk or a hung-up pipe is discovered there rather than at the write, and
either one is reported.

### `readFile(path: string) -> Result<string, Error>`
Reads a complete stream. The length is whatever the stream produced, not what
seeking to the end claimed it would be — a FIFO, socket or character device
cannot be seeked and reports its size as `-1`, so a seek-derived length reads
those sources as empty and truncates or overruns their contents.

### `loadTensor(path: string, name: string) -> Result<List<float>, Error>` — [BUILTIN-FILE-TENSOR]

Reads the tensor `name` out of the safetensors file at `path` as a host list. Every `F64`, `F32`, `F16` or `BF16` element widens exactly to the `float` it holds, in file order. A missing name, an unsupported dtype, malformed `data_offsets`, and a file that ends before the tensor does are each an `Error` naming the reason; none of them yields a shorter list. The reader is the one `gpuLoadTensor` uses ([GPU-BUFFER-TENSOR](0034-GPUComputation.md)), so the two loaders accept and refuse the same files, but `loadTensor` builds no `GpuBuffer` and calls nothing on the GPU surface. A program that loads its weights this way and never names a GPU built-in links no GPU code. Like `gpuLoadTensor`, it may answer an operation of a static handler; `--no-fs` refuses it.

```osprey
match loadTensor("model.safetensors", "wte.weight") {
    Success { value } => print("${listLength(value)} floats")
    Error { message } => print(message)  // loadTensor: wte.weight: no tensor of that name in the header
}
```

### Failure reasons — [BUILTIN-FILE-ERRMSG]

An `Error` from a file operation carries the operation, the subject and the
operating system's own explanation:

```osprey
match writeFile("out/report.txt", body) {
    Success { value } => print("wrote ${toString(value)} bytes")
    Error { message } => print(message)  // writeFile: out/report.txt: No such file or directory
}
```

Discarding that reason makes a missing directory, a permissions denial and a
full disk indistinguishable at the point they are handled, so a program cannot
choose to create the directory, ask for access, or free space. The reason
travels on a thread-local channel that the caller clears immediately before the
operation and takes ownership of immediately after, which is what lets a
`Result` outlive later I/O without inheriting an unrelated failure's message. A
runtime operation that reports no reason falls back to a fixed description.

## Process Operations — [BUILTIN-PROCESS]

### `spawnProcess(command: string, callback: fn(int, int, string) -> Unit) -> Result<int, Error>`
Starts a process and returns its handle. The callback receives the handle,
event kind (`1` stdout, `2` stderr, `3` exit), and event text.

```osprey
fn processEventHandler(processID, eventType, data) = match eventType {
    1 => print("[STDOUT] ${data}")
    2 => print("[STDERR] ${data}")
    3 => print("[EXIT] Code: ${data}")
    _ => print("[UNKNOWN] ${data}")
}

let result = spawnProcess("echo 'Hello'", processEventHandler)
```

### `awaitProcess(processId: int) -> int`
Waits for process completion and returns the exit code, or `-1` if the handle
is outside the valid range or has no process.

### `cleanupProcess(processId: int) -> Unit`
Releases process resources.

### Failure and resource release — [BUILTIN-PROCESS-FAILURE]

`spawnProcess` yields `Success` only for a handle a monitor is already
watching; every other outcome is `Error`. The runtime distinguishes them as
`-1` a missing command or callback, `-2` the handle space is used up, `-3` the
process record could not be allocated, `-4` a pipe could not be opened, `-5`
the monitor thread could not be started, and `-6` the process could not be
created. The argument check runs before the capacity check, so a call that is
both malformed and unaffordable reports `-1`.

Every failing path leaves the process as the call found it: no descriptor
opened for the attempt stays open, no child stays unreaped, no table slot stays
occupied, and a later `spawnProcess` succeeds once the condition clears. A
failed spawn does consume a handle number — handle numbers only ever increase —
but never a table slot. `awaitProcess` on a handle whose slot was released
returns `-1`.

## JSON Document Functions — [BUILTIN-JSON]

```osprey
jsonParse(text: string) -> Result<int, Error>
jsonGet(document: int, path: string) -> Result<string, Error>
jsonLength(document: int, path: string) -> int
jsonFree(document: int) -> Result<int, Error>
```

`jsonParse` returns a positive opaque document handle. Paths use dotted object
keys and bracketed array indices, such as `user.items[0].name`; `""` addresses
the root. `jsonGet` converts a string, number, boolean, or null scalar to a
string and returns `Error` for an invalid path, handle, array, or object.
`jsonLength` returns an array length or object member count and returns `-1`
for an invalid path, handle, or scalar. A successful handle must be released
once with `jsonFree`; an invalid handle or double free returns `Error`.

### Malformed documents and string grammar — [BUILTIN-JSON-STRING]

`jsonParse` returns `Error` for any text that is not one complete JSON value,
and it returns exactly that one failure: a document is either parsed whole or
rejected whole, never truncated at the first surprise and reported as a
success. Trailing text after the value, a missing separator, an unterminated
container, and an empty document are all rejections.

Inside a string the accepted grammar is RFC 8259's, with no extensions:

- the only escapes are `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and
  `\uXXXX`. Any other character after a backslash is a rejection — the
  backslash is never dropped and the character never taken literally;
- `\u` must be followed by exactly four hexadecimal digits. Fewer digits, a
  non-hexadecimal digit, or the end of the text is a rejection;
- a `\u` escape naming a high surrogate (U+D800–U+DBFF) must be immediately
  followed by a `\u` escape naming a low surrogate (U+DC00–U+DFFF); the pair
  decodes to the one code point they spell. A high surrogate followed by
  anything else, and a low surrogate that no high surrogate precedes, are both
  rejections; and
- the characters U+0000 through U+001F must appear escaped. A raw one is a
  rejection; and
- the text is UTF-8. A literal byte sequence that is not valid UTF-8 — an
  overlong form, a surrogate spelled in UTF-8, a code point past U+10FFFF, a
  truncated sequence, or a continuation byte with no lead — is a rejection,
  exactly as the equivalent `\u` escape is; and
- `\u0000` is rejected outright, escaped or not. Every scalar is delivered as a
  NUL-terminated string, so a decoded U+0000 would not embed a NUL — it would
  truncate the value, and `"a\u0000b"` would read back as `a` with nothing
  reporting the loss.

### Number grammar — [BUILTIN-JSON-NUMBER]

A number is stored as its source text and `jsonGet` returns that text
unchanged, so whatever the grammar accepts is what a caller reads back. The
accepted form is RFC 8259's, and nothing else:

```
number = [ "-" ] int [ frac ] [ exp ]
int    = "0" / ( digit1-9 *DIGIT )
frac   = "." 1*DIGIT
exp    = ( "e" / "E" ) [ "+" / "-" ] 1*DIGIT
```

So `-`, `+1`, `.5`, `1.`, `1.e5`, `1e`, `1e+`, `--1` and `1..2` are all
rejected, and a leading zero ends the integer part: `01` is `0` followed by
text that is not part of the number. Inside a container that unfinished number
makes the whole document malformed; at the root a value that is complete and
followed by more text is the distinct trailing-text failure instead.

Allocation failure at any point during a parse is reported the same way as
malformed input — `Error`, no handle, and nothing retained — because a
partially built document is not a document. A parse that fails, for either
reason, frees everything it allocated.

`jsonDecode` reads numbers as values. A number with neither fraction nor
exponent is an integer and must fit in 64 bits; any other number must be a
finite double. A number outside those ranges is an `Error` naming its byte,
never a rounded or clamped value. The handle API keeps such a number as text.

### Typed values — [BUILTIN-JSON-TYPED]

```osprey
type Json = JsonNull | JsonBool(bool) | JsonInt(int) | JsonFloat(float)
          | JsonString(string) | JsonArray(List<Json>) | JsonObject(List<JsonMember>)
type JsonMember = JsonMember(string, Json)

jsonEncode(json: Json) -> Result<string, Error>
jsonDecode(text: string) -> Result<Json, Error>
jsonQuote(text: string) -> Result<string, Error>
jsonField(value: Json, name: string) -> Result<Json, Error>
jsonIndex(value: Json, index: int) -> Result<Json, Error>
jsonText(value: Json) -> Result<string, Error>
jsonInt(value: Json) -> Result<int, Error>
jsonFloat(value: Json) -> Result<float, Error>
jsonBool(value: Json) -> Result<bool, Error>
```

`Json` is an ordinary union with positional payloads, so a program builds a
document as data and reads one with `match`. Object members keep their order.
Integers and floats stay distinct in both directions: `jsonDecode` gives `1` as
`JsonInt` and `1.0` or `1e0` as `JsonFloat`, and `jsonEncode` writes every float
with a decimal point or exponent, so the text decodes back to a float.

`jsonDecode` accepts exactly the texts `jsonParse` accepts, under
[BUILTIN-JSON-STRING], [BUILTIN-JSON-NUMBER] and [BUILTIN-JSON-LIMITS], and
holds no handle once it returns.

`jsonEncode` writes compact RFC 8259 text into one buffer and returns it as one
string. It escapes `"`, `\` and U+0001–U+001F with the shortest escape (`\n`,
`\t`, … or `\u00XX`) and writes all other characters as UTF-8. A float that is
NaN or infinite, or a string that is not valid UTF-8, has no JSON spelling and
is an `Error`. `jsonQuote(text)` is `jsonEncode(JsonString(text))`.

`jsonField` returns the first member with the given name. `jsonIndex` returns
an array element. `jsonText`, `jsonInt`, `jsonFloat` and `jsonBool` return a
scalar's value; `jsonFloat` also accepts `JsonInt` and widens it. Each accessor
returns `Error` with a message naming the accessor and the reason, such as
`jsonField: no member email` or `jsonIndex: not an array`.

These declarations are one unit of Osprey source that the compiler attaches
to a program that names any of them; the prelude is a list of such
independent units, each attached on its own demand. A program that names
none of a unit compiles exactly as written. A program that declares a type,
constructor or effect with one of a unit's names keeps its own declarations
and gets none of that unit. A program that declares a function with one of
these names keeps its own function in place of that one only. A unit written
against another unit's declarations — the signals unit is written against
these — attaches only beside it, so naming a signals declaration attaches
this unit too ([STAGE-SIGNALS-STD]).

### Limits and failure reasons — [BUILTIN-JSON-LIMITS]

Containers nest at most 128 levels. A deeper document is rejected before the
parser recurses, so hostile input cannot exhaust the stack. An object that
repeats a member name is rejected whole: lookups would otherwise depend on
which duplicate a reader keeps. Names compare after unescaping, so `"ab"` and
`"a\u0062"` are the same name. Neither parser imposes a byte limit on the
text.

A rejection's `Error` message names the operation, the byte offset where the
text stopped being JSON, and the reason:
`jsonDecode: byte 3: expected ',' or ']' in array`. A repeated name is
reported at its object's opening brace. A text that is absent entirely reports
`document: no text`.

## Terminal Functions — [BUILTIN-TERM]

```osprey
termReadKey() -> Result<string, Error>
termRawMode(enabled: int) -> Unit
termCols() -> int
termRows() -> int
termClear() -> int
termMoveCursor(row: int, column: int) -> int
termHideCursor() -> int
termShowCursor() -> int
```

On POSIX terminals, `termRawMode(1)` disables canonical input and echo and
enters the alternate screen; `termRawMode(0)` restores the saved mode and
screen. Its native status is not exposed. `termReadKey` returns normalized key
names such as `Enter`, `Up`, or `Ctrl-C`, or the literal input byte.
`termCols` and `termRows` return `-1` when the terminal size is unavailable.
The remaining functions write ANSI control sequences and return `0`; cursor
coordinates below `1` are clamped to `1`. Windows implementations return
`Error` from `termReadKey`, do nothing for raw mode, and return `-1` from the
integer functions.

## Collection Functions

Collection operations return new values without changing their inputs. Except
for `length` and `isEmpty`, public names are prefixed with `list` or `map`.

### Common (`List` and `Map`)

#### `length(list: List<T>) -> int` &nbsp; / &nbsp; `length(map: Map<string, V>) -> int` — [BUILTIN-COLLECTION-LENGTH]
Returns the element count. `listLength` and `mapLength` are equivalent
type-specific spellings.

#### `isEmpty(list: List<T>) -> bool` &nbsp; / &nbsp; `isEmpty(map: Map<string, V>) -> bool` — [BUILTIN-COLLECTION-ISEMPTY]
Returns whether the element count is zero. The receiver type selects string,
list, or map behavior for both common functions.

### `List<T>` — [BUILTIN-LIST]

`List()` creates an empty list. List literals create populated lists.
[`toList`](../language/0010-LoopConstructsAndFunctionalIterators.md#tolistiterator-iteratort---listt--builtin-iter-tolist)
collects an iterator pipeline into one ([BUILTIN-ITER-TOLIST]).

#### `listGet(list: List<T>, index: int) -> Result<T, Error>` — [BUILTIN-LIST-GET]
Equivalent to `list[index]`. An out-of-range index returns `Error`. A string list bound to a name — the result of `lines`, `words` or `split` — is settled into the runtime representation once, at the binding, so every `listGet` on the bound name is a constant-time read and never a copy of the list.

#### `listPrepend(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-PREPEND]
Returns a list with `value` at the front.

#### `listAppend(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-APPEND]
Returns a list with `value` at the end.

#### `listConcat(left: List<T>, right: List<T>) -> List<T>` — [BUILTIN-LIST-CONCAT]
Concatenates two lists. `left + right` is equivalent.

#### `listReverse(list: List<T>) -> List<T>` — [BUILTIN-LIST-REVERSE]
Returns the elements in reverse order.

#### `listContains(list: List<T>, value: T) -> bool` — [BUILTIN-LIST-CONTAINS]
Compares `value` with each element by `==`
([BOOL-EQUALITY](../language/0009-BooleanOperations.md#what--and--compare-bool-equality)):
ints, strings and bools by value, floats under [FLOAT-COMPARE] (so NaN is
never found), and payload-free union values by variant. An element type with
no decided equality, such as a record, a list, a union that may carry a
payload, or a function, is a compile error. Use
`structuralEquals` in a search of your own when a deep comparison is really
needed.

#### `forEachList(list: List<T>, function: fn(T) -> Unit) -> Unit` — [BUILTIN-LIST-FOREACH]
Calls `function` once per element in index order.

#### `mapList(list: List<T>, function: fn(T) -> U) -> List<U>` — [BUILTIN-LIST-MAP]
Returns a new list with `function` applied to every element, in index order.

#### `filterList(list: List<T>, predicate: fn(T) -> bool) -> List<T>` — [BUILTIN-LIST-FILTER]
Returns a new list keeping the elements that satisfy `predicate`, in index order.

#### `foldList(list: List<T>, initial: U, function: fn(U, T) -> U) -> U` — [BUILTIN-LIST-FOLD]
Reduces a list to a single value by repeatedly applying `function`, starting from `initial`.

#### `listSort(list: List<T>) -> List<T>` — [BUILTIN-LIST-SORT]
Returns a new list sorted in ascending order by the elements' natural order: integers compare by value and strings by byte content. The sort is stable. `T` must be `int` or `string`; any other element type is a compile-time type error, never a runtime crash.

#### `listSortBy(list: List<T>, compare: fn(T, T) -> int) -> List<T>` — [BUILTIN-LIST-SORT-BY]
Returns a new list sorted by `compare`, for any element type. `compare(a, b)` returns a negative number when `a` comes first, a positive number when `b` comes first, and zero to keep the two in their original order: the sort is stable. It performs O(n log n) comparisons. `compare` may be a lambda that captures its scope, a named function or a function value, and its effects are those of the call.

```osprey
type Evidence = { title: string, at: int }
fn order(a, b) = if a < b { -1 } else { if a > b { 1 } else { 0 } }
listSortBy(evidence, fn(x, y) => order(x.title, y.title))
```

### `Map<string, V>` — [BUILTIN-MAP]

`Map()` and map literals create string-keyed maps. Map iteration order is
unspecified.

#### `mapGet(map: Map<string, V>, key: string) -> Result<V, Error>` — [BUILTIN-MAP-GET]
Equivalent to `map[key]`. A missing key returns `Error`.

#### `mapContains(map: Map<string, V>, key: string) -> bool` — [BUILTIN-MAP-CONTAINS]
Returns whether `key` is present.

#### `mapSet(map: Map<string, V>, key: string, value: V) -> Map<string, V>` — [BUILTIN-MAP-SET]
Returns a map with `key` bound to `value`, replacing any prior binding.

#### `mapRemove(map: Map<string, V>, key: string) -> Map<string, V>` — [BUILTIN-MAP-REMOVE]
Returns a map without `key`. A missing key leaves the map unchanged.

#### `mapMerge(left: Map<string, V>, right: Map<string, V>) -> Map<string, V>` — [BUILTIN-MAP-MERGE]
Returns the right-biased union. `left + right` is equivalent.

#### `mapKeys(map: Map<string, V>) -> List<string>` — [BUILTIN-MAP-KEYS]
Returns all keys in unspecified order.

#### `mapValues(map: Map<string, V>) -> List<V>` — [BUILTIN-MAP-VALUES]
Returns all values in the same traversal order as `mapKeys`.

## Iterators and Pipe

`range`, `forEach`, `map`, `filter`, `fold`, `toList`, and `|>` are documented in
[Iterators and Iteration](../language/0010-LoopConstructsAndFunctionalIterators.md).

## HTTP

See [HTTP](0014-HTTP.md).

## WebSockets

See [WebSockets](0015-WebSockets.md).

## Fibers and Channels

`spawn`, `await`, `send`, `recv`, `yield`, `Fiber<T>`, `Channel<T>` are documented in [Fibers and Concurrency](../language/0011-LightweightFibersAndConcurrency.md).

## GPU Computation

`toGpu`, `fromGpu`, `gpuLength`, `gpuMap`, `gpuFold`, and `GpuBuffer<T>` are documented in [GPU Computation](0034-GPUComputation.md).
