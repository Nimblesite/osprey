# Pattern Matching

`match` is the only branching construct in Osprey's canonical AST — every
surface conditional (the ternary, `?:`, and the Default flavor's
`if`/`else` [GRAMMAR-IF-ELSE]) desugars to it at lowering. Record patterns
are matched structurally by field name, not by field order. See
[Type System](0004-TypeSystem.md) for type unification rules.

> **Flavor layer — mixed.**  A `match` lowers to `Expr::Match` over `MatchArm`s, each carrying a `Pattern` (`Wildcard`, `Literal`, `Constructor { name, fields, sub_patterns }`, `TypeAnnotated`, `Structural`, `List`, `Binding`). Only the *spelling* of these patterns is a surface (CST) concern: this chapter shows the Default flavor — a one-field variant is `Success { value }`, where the ML flavor writes `Success value` ([`[FLAVOR-ML-MATCH]`](0024-MLFlavorSyntax.md#match)) — but both flavors lower to the **same** `Pattern::Constructor { name, fields }`. Everything else here — exhaustiveness checking, `any`/union narrowing, and arm semantics — is shared-core: it runs on the canonical AST and is flavor-blind ([`[FLAVOR-BOUNDARY]`](0023-LanguageFlavors.md#the-one-law)). See [Language Flavors](0023-LanguageFlavors.md) and [ML Flavor Syntax](0024-MLFlavorSyntax.md).

## Basic Patterns

```osprey
let result = match value {
    0 => "zero"
    1 => "one"
    n => "other: " + toString(n)
}
```

```osprey-ml
result =
    match value
        0 => "zero"
        1 => "one"
        n => "other: " + toString n
```

## Union Type Patterns

A union pattern names the variant. Variants with fields are destructured using `{ field, ... }`; variants without fields are matched by name alone. Both forms lower to `Pattern::Constructor`; the brace destructuring shown here is the Default surface, spelled `Success value` in the ML flavor ([`[FLAVOR-ML-MATCH]`](0024-MLFlavorSyntax.md#match)).

```osprey
type Option = Some { value: int } | None

let message = match option {
    Some { value } => "Value: " + toString(value)
    None           => "No value"
}
```

```osprey-ml
type Option =
    Some
        value : int
    None

message =
    match option
        Some value => "Value: " + toString value
        None       => "No value"
```

## Wildcard Patterns

The underscore `_` matches any value:

```osprey
let category = match score {
    100 => "perfect"
    90 => "excellent"
    _ => "good"
}
```

```osprey-ml
category =
    match score
        100 => "perfect"
        90 => "excellent"
        _ => "good"
```

## Type Annotation Patterns

A pattern of the form `name: type` matches when the value has the named type and binds it. This is the required form for narrowing an `any` value. The grammar for all pattern forms is in [Syntax](0003-Syntax.md#match-expressions).

```osprey
// Narrowing an any value
match anyValue {
    n: int    => n + 1
    s: string => length(s)
    b: bool   => match b {
        true  => 1
        false => 0
    }
    _ => 0
}

// Structural matching: any type with these field names
match anyValue {
    { name, age }       => print("${name}: ${age}")
    p: { name, age }    => print("person ${p.name}: ${p.age}")   // bind whole + destructure
    u: User { id }      => print("user ${id}")                   // typed structural
    _                   => print("unknown")
}

// Type-narrowed structural fields
match anyValue {
    { x, y }                       => print("point: (${x}, ${y})")
    p: { name }                    => print("named: ${p.name}")
    { id, email, active: bool }    => print("active user: ${id}")
    _                              => print("no match")
}

// Type pattern with destructuring of a known constructor
match result {
    success: Success { value, timestamp } => processSuccess(value: value, timestamp: timestamp)
    error:   Error   { code, message }    => handleError(code: code, message: message)
    _                                     => defaultHandler()
}
```

> Type-narrowing arms (`n : int =>`) and structural field patterns
> (`{ name, age } =>`, `p : { name } =>`) on `any` have no ML-flavor surface
> syntax; use the Default flavor for structural and type-narrowed matching.
> Constructor-payload matching (`Success value`, `Error message`) works in both
> flavors — see the `Result` example below.

## Result Patterns

`Result<T, E>` is matched the same way as any other union. See [Error Handling](0013-ErrorHandling.md) for the type and arithmetic semantics.

```osprey
let calculation = 1 + 3 + (300 / 5)  // Result<int, MathError>

match calculation {
    Success { value }   => print("Result: ${value}")
    Error   { message } => print("Math error: ${message}")
}
```

```osprey-ml
calculation = 1 + 3 + (300 / 5)  // Result<int, MathError>

match calculation
    Success value   => print "Result: ${value}"
    Error   message => print "Math error: ${message}"
```

Compound arithmetic expressions yield a single `Result`, not nested `Result`s; the compiler unwraps intermediate values inside the chain. Only the final value needs to be matched.

## Ternary Match (Syntactic Sugar)

A two-arm match has a shorthand. Two equivalent forms exist:

```ebnf
ternary ::= expr "{" pattern "}" "?" expr ":" expr   (* structural form *)
          | expr "?:" expr                            (* Result default form *)
```

Structural form — pick out a field, fall back if the pattern fails:

```osprey
let calculation = 10 + 5
let value = calculation { value } ? value : -1   // 15
```

```osprey-ml
calculation = 10 + 5
value =
    match calculation
        Success value => value
        Error _       => -1   // 15
```

Desugars to:

```osprey
match calculation {
    { value } => value
    _         => -1
}
```

```osprey-ml
match calculation
    Success value => value
    Error _       => -1
```

Result-default form — extract `Success { value }` or use the default on `Error`:

```osprey
let safeValue = divide(a: 10, b: 2) ?: -1   // 5
let errorVal  = divide(a: 10, b: 0) ?: -1   // -1
```

```osprey-ml
orDefault r =
    match r
        Success value => value
        Error _       => -1

safeValue = orDefault (divide (10, 2))   // 5
errorVal  = orDefault (divide (10, 0))   // -1
```

A boolean expression with `?:` works because `true`/`false` desugar to the same match:

```osprey
let status = isActive ? "Active" : "Inactive"
```

```osprey-ml
status =
    match isActive
        true  => "Active"
        false => "Inactive"
```

## if / else (Syntactic Sugar) [GRAMMAR-IF-ELSE]

The Default flavor also spells the boolean two-arm match the way mainstream
languages do. `if` is an **expression** — it always yields a value, so the
`else` branch is mandatory and each branch is a single expression in braces:

```ebnf
if_expr ::= "if" expr "{" expr "}" "else" ( "{" expr "}" | if_expr )
```

```osprey
fn tier(score) = if score >= 2000 { Epic } else if score >= 500 { Solid } else { Starter }
```

Desugars to the boolean match — no new AST node crosses the
[FLAVOR-BOUNDARY], so it emits IR byte-identical to the explicit form:

```osprey
fn tier(score) = match score >= 2000 {
    true  => Epic
    false => match score >= 500 {
        true  => Solid
        false => Starter
    }
}
```

`else if` is simply a nested `if` expression in the `else` position, giving
flat multi-way chains without indentation creep. An `if` with no `else` is a
parse error — there is no value for the false path
(`examples/failscompilation/if_without_else.ospo`).

The ML flavor deliberately omits `if`/`else`: it writes the `match` directly.

```osprey-ml
tier score = match score >= 2000
    true  => Epic
    false => match score >= 500
        true  => Solid
        false => Starter
```