# String Interpolation [STRING-INTERPOLATION]

Both flavors use `${...}` inside a double-quoted string. Shared code splits the
literal into text and expression parts; the active flavor parses each embedded
expression. The result is one `Expr::InterpolatedStr`.

```osprey
let name = "Alice"
let age = 30
let message = "Hello ${name}, you are ${age}"
```

```osprey-ml
name = "Alice"
age = 30
message = "Hello ${name}, you are ${age}"
```

An interpolation expression may contain operators, calls, field access, or a
nested braced expression:

```osprey
fn double(n) = n * 2
let message = "value=${double(5)}, point=${point.x}, next=${match flag { true => 1 false => 0 }}"
```

## Rendering

Strings are inserted unchanged. Integers, floats, and booleans use their normal
scalar rendering.

A `Result` is never unwrapped at an interpolation hole. It renders as the
complete `Success(value)` or `Error(message)` value, preserving its failure
channel exactly as `toString` does:

```osprey
let result = intDiv(10, 5)
print("value=${result}")       // value=Success(2)
print(toString(result))       // Success(2)
```

Direct interpolation of collection, record, and function handles has no
specified textual representation.

## Escapes

String unescaping recognizes:

- `\n`, `\r`, and `\t`
- `\e` for the ASCII escape character and `\0` for NUL
- `\"` and `\\`

Unknown escape sequences retain their backslash. There is no dedicated escape
for the interpolation opener `${`; `\${...}` is still parsed as interpolation.
