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

A union pattern names the variant. A **named-field** payload is destructured with `{ field, ... }`, a **positional** payload ([TYPE-UNION-POSITIONAL](0003-Syntax.md#type-declarations)) with `(a, b)` binding by slot, and a payload-free variant is matched by name alone. All three lower to `Pattern::Constructor`; the spellings shown here are the Default surface — the ML flavor drops the delimiters (`Success value`, `Node l r`) per [`[FLAVOR-ML-MATCH]`](0024-MLFlavorSyntax.md#match).

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
let calculation = 1 + 3 + (300 / 5)  // Result<float, MathError>

match calculation {
    Success { value }   => print("Result: ${value}")
    Error   { message } => print("Math error: ${message}")
}
```

```osprey-ml
calculation = 1 + 3 + (300 / 5)  // Result<float, MathError>

match calculation
    Success value   => print "Result: ${value}"
    Error   message => print "Math error: ${message}"
```

Only `/` and `%` produce a `Result` ([ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain), specified but not yet implemented — `+ - *` still wrap today): `1 + 3` is plain `int` and there is nothing to match. An expression containing `/` or `%` yields a single `Result`, not nested `Result`s — the wrapper propagates outward through the chain rather than being unwrapped at each operator, so an erroring operand errors the whole expression and only the final value is matched ([Chaining Arithmetic](0013-ErrorHandling.md#chaining-arithmetic)).

## Ternary Match (Syntactic Sugar)

A two-arm match has a shorthand. Two equivalent forms exist:

```ebnf
ternary ::= expr "{" pattern "}" "?" expr ":" expr   (* structural form *)
          | expr "?:" expr                            (* Result default form *)
```

Structural form — pick out a field, fall back if the pattern fails:

```osprey
let calculation = intDiv(a: 10, b: 5)   // Result<int, MathError> — [BUILTIN-INTDIV]
let value = calculation { value } ? value : -1   // 2
```

The structural form has no ML surface; the ML flavor writes the `match`.

```osprey-ml
calculation = intDiv (10, 5)
value =
    match calculation
        Success value => value
        _             => -1
```

The Default form desugars to a block that binds each named field from the
scrutinee and evaluates the `then` expression — a record always carries its
declared fields, so the structural test cannot fail and the `else` expression is
unreachable by construction:

```osprey
{ let value = calculation.value; value }
```

### Result Default `?:` — [PATTERN-RESULT-DEFAULT]

`?:` is **the** `Result`-default operator, and this section is its normative
home; other chapters cross-reference [PATTERN-RESULT-DEFAULT] rather than
restate it. `e ?: d` evaluates to `e`'s `Success` payload, otherwise to `d`. It
is right-associative and binds below every other operator
([Syntax](0003-Syntax.md#expressions)), so `1 + 2 ?: 0` parses as `(1 + 2) ?: 0`
and `f(x) ?: 0 ?: 1` as `f(x) ?: (0 ?: 1)`.

The scrutinee must be a `Result` or a `bool` — a consequence of the boolean-arm
lowering below, not a separate rule. `e` of any other type is a type error
(`5 ?: -1` reports `cannot unify int with bool`), so `?:` cannot be written
against a total value. A `bool` scrutinee is degenerate but legal:
`true ?: false` is `true`.

Lowering: `e ?: d` reuses the scrutinee as the `then` branch of the ternary's
two-arm `Expr::Match` — literally `match e { true => e  false => d }`, with
`Pattern::Literal(Bool)` arms, so `?:` needs no `Result`-specific node or
runtime. A `Result` scrutinee selects the arms by discriminant, which is what
makes `intDiv(a: 10, b: 0) ?: -1` yield `-1`. **The ML lowering must emit this
same shape**, not a `Success`/`Wildcard` pair, or ML and Default twins diverge
and [FLAVOR-IR-EQUIV](0023-LanguageFlavors.md#cross-flavor-equivalence-tests)
fails.

The spelling is `?:` in **both** flavors. The Default flavor implements it
today; the ML flavor does not — its lexer rejects `?` — so every ML `?:` in
this chapter is specified, not yet implemented. ML authors currently have no
spelling for the operation and write the two-arm match by hand.

```osprey
let safeValue = intDiv(a: 10, b: 2) ?: -1   // 5
let errorVal  = intDiv(a: 10, b: 0) ?: -1   // -1
```

```osprey-ml
safeValue = intDiv (10, 2) ?: -1   // 5
errorVal  = intDiv (10, 0) ?: -1   // -1
```

### Boolean Ternary

A boolean discriminant uses the `? :` ternary, because `true`/`false` desugar to the same match:

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