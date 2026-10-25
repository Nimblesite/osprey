# Boolean Operations

Default and ML boolean expressions use `true`, `false`, comparisons, `&&`, `||`, and `!`.
Conditionals are expressions: Default and ML support `match`, Default also
supports `cond ? then : else` and `if ... else`, and each form lowers to a
two-arm `Expr::Match`.

[Python flavor](0041-PythonFlavorSyntax.md#expressions-and-iteration--flavor-python-expressions) spells booleans `True`/`False` and operators `and`/`or`/`not`. Its conditional expressions and `if`/`elif`/`else` suites lower to the same boolean matches; its `match`/`case` uses Osprey's exhaustive matching rules.

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

## Comparison operands [BOOL-COMPARE-OPERANDS]

The two operands of a comparison have one type. Two unrelated types are rejected: ``cannot compare `bool` with `int`: `<` needs two operands of one type``. A `Result` operand is rejected as well; handle it with `match` or `?:` first.

```osprey
let valid = age >= 18 && isAuthorized
let fallback = isAdmin || hasInvite
let blocked = !isActive
```

## What `==` and `!=` compare [BOOL-EQUALITY]

`==` and `!=` are defined for these operands. Numeric, boolean and variant-tag checks take constant time. String comparisons scan bytes and can take time proportional to the string length.

| Operand type | `==` is true when |
| --- | --- |
| `int` | the numbers are equal |
| `float` | the numbers are equal, under [FLOAT-COMPARE](0004-TypeSystem.md#floating-point-comparison--float-compare): NaN is unequal to everything |
| `string` | the bytes are equal |
| `bool` | both are `true` or both are `false` |
| a union whose variants all carry no payload (`type Area = Apps \| Files`) | both are the same variant |
| any union, when one operand is written as a payload-free constructor (`shape == Dot`) | the other operand is that variant |

The last row is a variant test. It reads only the variant, never a payload,
so it is defined even when other variants of the union carry payloads.

Other operands are compile errors: records, tuples, lists, maps, union values that may carry a payload (`Circle { r: 2 } == Circle { r: 2 }`), functions, runtime handles such as fibers and channels, and `Unit`. The error names the operand type and suggests alternatives:

```
cannot compare `Route` with `==`: equality is decided only for int, float, string, bool and payload-free union variants; compare a key, id or revision field, or call `structuralEquals(a, b)` for a deliberate deep comparison
```

Use [`structuralEquals`](../runtime/0012-Built-InFunctions.md#structural-comparison--builtin-structural-equals) for a supported deep content comparison. It traverses the values being compared. Comparing an integer ID or revision field is a constant-time alternative when that field defines the identity you need.

A generic function may compare values of a type parameter. The requirement
travels with the function's type, and each call site is checked at its own
type: `fn same(a, b) = a == b` accepts `same(1, 2)` and rejects
`same(route, route)`.

## What `<`, `<=`, `>` and `>=` compare [BOOL-ORDERING]

The ordering operators are defined for `int`, `float` (under
[FLOAT-COMPARE](0004-TypeSystem.md#floating-point-comparison--float-compare)),
`string` (byte order) and `bool` (`false < true`). Every other operand type is
a compile error, for the same reason as [BOOL-EQUALITY]: no order for it is
decided. That includes payload-free unions, whose declaration order is not an
ordering.

The Default/ML twins `tests/regressions/basics/operators/equality_domain.test.*`
pin every accepted row in both flavors. Each rejected kind has a program under
`examples/failscompilation/` (`equality_*.ospo`), which the must-reject ratchet
keeps rejected.
