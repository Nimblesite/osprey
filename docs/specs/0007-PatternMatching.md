# Pattern Matching

`match` evaluates one scrutinee and selects an arm. Its arms must produce values
that unify to one result type. Default and ML use different delimiters but lower
to the same `Expr::Match` and `Pattern` nodes.

## Literal and binding patterns

A scalar literal compares with the scrutinee. A lower-case name binds the
scrutinee and is therefore a catch-all.

```osprey
let label = match value {
    0 => "zero"
    1 => "one"
    n => "other: ${n}"
}
```

## Union patterns

A nullary variant is matched by name. A named payload binds fields by field
name, independent of their order in the pattern. A positional payload binds by
slot ([TYPE-UNION-POSITIONAL](0003-Syntax.md#positional-variants-type-union-positional)).

```osprey
type Option = Some { value: int } | None
type Tree = Leaf | Node(Tree, Tree)

let message = match option {
    Some { value } => "value=${value}"
    None           => "none"
}

let side = match tree {
    Node(left, _) => left
    Leaf          => Leaf
}
```

ML drops the payload delimiters (`Some value`, `Node left _`) but produces the
same constructor patterns.

## Wildcard patterns

`_` matches without binding:

```osprey
let category = match score {
    100 => "perfect"
    _   => "other"
}
```

## List patterns

List patterns may require an exact length or bind a remaining suffix. The rest
binder is permitted only once and only at the end
([TYPE-LIST-PATTERNS](0004-TypeSystem.md#patterns--type-list-patterns)).

```osprey
let first = match values {
    []              => 0
    [head, ...tail] => head
}
```

## Type annotation patterns

`name: Type` binds a value under the written compile-time type. It is not a
runtime type test: code generation treats the arm as a catch-all. Use it only
when the scrutinee already has that static type. Runtime narrowing of an `any`
value by type is not implemented.

```osprey
let label = match person {
    p: Person => p.name
}
```

Standalone structural record patterns such as `{ name, age } => ...` are also
not implemented by the backend.

## Exhaustiveness and unreachable arms [TYPE-MATCH-EXHAUSTIVE]

A match over `bool`, `Result`, or a known union must cover every case, either
explicitly or with a catch-all. A duplicate variant arm is unreachable, as is
every arm after `_` or a lower-case binding; the compiler rejects both.

Matches over open scalar domains such as `int` need a catch-all when total
behavior is required, but the compiler does not prove scalar exhaustiveness.

## Result patterns

`Result<T, E>` uses the built-in `Success { value }` and `Error { message }`
variants:

```osprey
let calculation = intDiv(10, 0)

match calculation {
    Success { value }   => print("result=${value}")
    Error { message }   => print("error=${message}")
}
```

The error type of `intDiv` and other fallible built-ins is `Error`. Arithmetic
operators `/` and `%` use `MathError`; `+`, `-`, and `*` return plain scalars
([ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain)).

## Ternary Match (Syntactic Sugar)

Default has a structural form:

```ebnf
structuralTernary ::= expression "{" field ("," field)* "}" "?" expression ":" expression
```

Lowering binds each named field by direct field access and evaluates
the then expression. It does not perform a runtime pattern test and does not
evaluate the else expression; a missing field is a type error.

```osprey
let value = record { value } ? value : 0
```

ML has no structural-ternary surface.

### Result Default `?:` — [PATTERN-RESULT-DEFAULT]

For a `Result`, `result ?: fallback` yields the `Success` payload or lazily
evaluates `fallback` for `Error`.

```osprey
let safe = intDiv(10, 2) ?: -1
let failed = intDiv(10, 0) ?: -1
```

`?:` is right-associative and binds below every other operator
([Syntax](0003-Syntax.md#expressions)). The same spelling is available in ML.

A boolean scrutinee is also accepted by the shared boolean-match lowering:
`true ?: false` is `true`. On that degenerate true path, a nontrivial scrutinee
expression is evaluated again; use the ordinary boolean ternary instead.

### Boolean ternary

Default `condition ? yes : no` lowers to a boolean match:

```osprey
let status = active ? "active" : "inactive"
```

ML writes the equivalent `match` directly.

## if / else (Syntactic Sugar) [GRAMMAR-IF-ELSE]

Default also provides a boolean `if` expression. `else` is mandatory, and each
branch is one expression. `else if` nests another `if` in the false branch.

```osprey
fn tier(score) = if score >= 2000 { Epic } else if score >= 500 { Solid } else { Starter }
```

Lowering produces nested two-arm boolean matches; no `if` node reaches type
checking or code generation. ML writes the matches directly.
