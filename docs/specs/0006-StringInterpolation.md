# String Interpolation

String interpolation provides convenient inline expression evaluation within string literals.

> **Flavor layer — mixed.**  `${...}` interpolation is flavor-neutral: BOTH the Default flavor (`.osp`) and the ML flavor (`.ospml`) spell it identically, and the scanning that splits a literal into text + `${...}` segments plus all escape resolution live in the shared `crate::strings` module — not in either flavor's frontend. Interpolated literals lower to one canonical `Expr::InterpolatedStr` whose `InterpolatedPart`s carry either literal text or an embedded expression, so the [shared core](0023-LanguageFlavors.md#the-one-law) never sees a flavor. Only the *embedded fragment* (`x + y` inside `${...}`) is parsed per-flavor — each flavor parses that expression in its own surface grammar — but the brace scanning and escapes are identical. See [Language Flavors](0023-LanguageFlavors.md) and [ML Flavor Syntax](0024-MLFlavorSyntax.md).

## Syntax

String interpolation uses `${}` syntax:

```osprey
let name = "Alice"
let age = 30
let message = "Hello ${name}, you are ${age} years old"
```

```osprey-ml
name = "Alice"
age = 30
message = "Hello ${name}, you are ${age} years old"
```

## Expression Support

Any expression can be interpolated:

```osprey
let x = 10
let y = 5
print("Sum: ${x + y}")
print("Product: ${x * y}")
print("Complex: ${(x + y) * 2 - 1}")

// Function calls
fn double(n) = n * 2
print("Doubled: ${double(5)}")

// Field access
type Person = { name: string, age: int }
let person = Person { name: "Bob", age: 25 }
print("Person: ${person.name}, age ${person.age}")
```

```osprey-ml
x = 10
y = 5
print "Sum: ${x + y}"
print "Product: ${x * y}"
print "Complex: ${(x + y) * 2 - 1}"

// Function calls
double n = n * 2
print "Doubled: ${double 5}"

// Field access
type Person =
    name : string
    age : int
person = Person(name = "Bob", age = 25)
print "Person: ${person.name}, age ${person.age}"
```

## Type Handling

Interpolated expressions are automatically converted to strings:

- **Primitive types**: int, float, bool converted directly
- **String types**: Inserted as-is
- **Result types**: interpolation auto-unwraps — the success payload is rendered (string interpolation is one of the auto-unwrap contexts in [Result Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping)); an `Error` currently renders its zero payload rather than the message — a known gap; `toString` renders `Error(<message>)` and preserves the payload per [ERR-PAYLOAD](0013-ErrorHandling.md#error-payload-propagation--err-payload). To render the wrapper of a success, likewise use `toString`. The `Result`-producing sources are the fallible operators `/` and `%` and fallible calls such as `intDiv`, `parseInt` and the checked-arithmetic builtins; per [ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain), `+ - *` yield plain scalars and never produce a `Result` to render.
- **Complex types**: Use `toString()` for explicit conversion

```osprey
let num = 42
let flag = true
print("Number: ${num}, Flag: ${flag}")

let result = intDiv(a: 10, b: 5)
print("Result: ${result}")        // "Result: 2"  (auto-unwrapped)
print(toString(result))           // "Success(2)" (wrapper kept)
```

```osprey-ml
num = 42
flag = true
print "Number: ${num}, Flag: ${flag}"

result = intDiv (10, 5)
print "Result: ${result}"        // "Result: 2"  (auto-unwrapped)
print (toString result)          // "Success(2)" (wrapper kept)
```

## Escaping

Use backslash to escape special characters:

```osprey
let literal = "Dollar sign: \${not interpolated}"
let newline = "Line 1\nLine 2"
let quote = "He said \"Hello\""
let backslash = "Path: C:\\Users\\Name"
```

```osprey-ml
literal = "Dollar sign: \${not interpolated}"
newline = "Line 1\nLine 2"
quote = "He said \"Hello\""
backslash = "Path: C:\\Users\\Name"
```

Supported escape sequences:
- `\n` - Newline
- `\t` - Tab
- `\r` - Carriage return
- `\\` - Backslash
- `\"` - Double quote
- `\${` - Literal `${` (prevents interpolation)
