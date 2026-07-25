# Boolean Operations

Boolean expressions use `true`, `false`, comparisons, `&&`, `||`, and `!`.
Conditionals are expressions: both flavors support `match`, Default also
supports `cond ? then : else` and `if ... else`, and each form lowers to a
two-arm `Expr::Match`.

```osprey
let status = match isValid {
    true  => "valid"
    false => "invalid"
}

let maximum = a > b ? a : b
let label = if enabled { "on" } else { "off" }
```

```osprey-ml
status = match isValid
    true  => "valid"
    false => "invalid"

maximum = match a > b
    true  => a
    false => b
```

The branch expressions must unify to one result type. Default `if` requires an
`else` branch. See [Pattern Matching](0007-PatternMatching.md) for the lowering
and exhaustiveness rules.

## Operators [BOOL-SHORT-CIRCUIT]

- `&&` evaluates its right operand only when the left operand is `true`.
- `||` evaluates its right operand only when the left operand is `false`.
- `!` negates a boolean.
- `==`, `!=`, `<`, `>`, `<=`, and `>=` return a boolean.

```osprey
let valid = age >= 18 && isAuthorized
let fallback = isAdmin || hasInvite
let blocked = !isActive
```
