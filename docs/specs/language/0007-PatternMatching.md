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

## Binding scope — [PATTERN-BINDING-SCOPE]

Every pattern binding belongs to its own arm. It shadows an enclosing binding only while that arm is evaluated. Sibling arms and code following the match retain their original bindings, including mutable cells and function values. Nested matches restore the containing arm's bindings on exit. A closure returned from an arm retains the binding it captured there. A function-valued binder retains its complete parameter and return types, including when extracted from a union, record, Result or list. A Result join preserves its callable Success payload even when the error type remains polymorphic.

This applies equally to literal catch-all bindings, Result and union payloads, list elements and tails, structural fields and erased structural matches. The paired `src/osprey/tests/flavors/matching/matching.test` suites assert isolation between arms, restoration after nested matches, escaping captures, shadowing of mutable cells and callable payloads (including float-returning functions through Result joins) in both flavors.

### Result binder roles — [PATTERN-RESULT-BINDERS]

A Result binder's spelling never changes the constructor's payload role. A `Success` binder named `message` retains the successful value's type, including records and callable values; an `Error` binder named `value` retains the error payload ([ERR-PAYLOAD](0013-ErrorHandling.md#resultt-e--err-result)). Python keyword labels select payload slots and create only their written binders, so `Success(value=message)` never creates a hidden `value` binding.

## Union patterns

A bare variant name matches that variant and binds nothing, whether or not the variant carries a payload. A payload has two destructuring forms, and the **form written** — not how the payload was declared — decides how binders map onto slots:

- `Ctor { a, b }` binds each binder to the field of that same name, independent
  of their order in the pattern.
- `Ctor(a, b)` binds by slot: the binder in column *i* takes payload slot *i*,
  whatever it is spelled
  ([TYPE-UNION-POSITIONAL](0003-Syntax.md#positional-variants-type-union-positional)).

A named payload accepts either form, so `Some(n)` is legal beside
`Some { value }`. A positionally declared payload accepts only the second in
practice, because its slots have no spellable names.

Deciding this from the declaration instead is what let the type checker and the
code generator disagree: the checker read `Ctor(a, b)` by column while codegen
read it by name for any named payload, so a binder that named no field bound
nothing, and one that named the *wrong* field silently loaded another slot —
returning a payload pointer out of an `-> int` function.

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

ML drops the payload delimiters (`Some value`, `Node left _`). It has only the
one form, and that form is the positional one — ML `Some value` is the same
constructor pattern as Default `Some(value)`, so an ML binder always takes its
column and may be renamed freely. `Success`/`Error` are the single exception:
each has one payload that binds by ROLE, not by spelling — ML `Success section`
and `Error reason` bind the success value and the error payload under the
chosen names, exactly as Default's `Success { value }` and `Error { message }`
(or the positional `Success(section)` and `Error(reason)`) do — so the binder's type is the payload's type wherever it is used, including
as the argument of a generic call.

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
when the scrutinee already has that static type; to discriminate at runtime,
match the structure instead.

```osprey
let label = match person {
    p: Person => p.name
}
```

## Structural patterns — [PATTERN-STRUCTURAL]

A brace pattern matches a value's row ([TYPE-ROW](0004-TypeSystem.md#rows-and-record-type-unification--type-row))
and binds each named field. It is the runtime counterpart of structural
unification, and the only way to discriminate an `any`
([TYPE-ANY](0004-TypeSystem.md#the-any-type--type-any)):

```osprey
let described = match value {
    { message }     => "just=${message}"
    { code, .. }    => "coded=${code}"
    _               => "unknown"
}
```

A pattern is **closed** by default — `{ message }` selects a row of exactly
`message` — and trailing `..` opens it to any row carrying at least the named
fields. Closed-by-default is what keeps arm order from changing meaning: an open
`{ x, y }` would select a three-dimensional point and shadow a later
`{ x, y, z }` arm. The compiler rejects a `..`-opened arm that shadows a later
one rather than resolving it by order.

A binder always takes its field's own name. Nested rows
(`{ origin: { x, y }, .. }`) and `field: binder` renames do not parse — see the
status note below.

## Tuple patterns — [PATTERN-TUPLE]

A tuple pattern binds by position ([TYPE-TUPLE](0004-TypeSystem.md#tuples--type-tuple)).
Because a tuple is a row with decimal field names, it is the positional spelling
of a structural pattern, not a separate mechanism:

```osprey
match pair {
    (n, label) => "${label}=${n}"
}
```

`(x)` is grouping, not a one-element tuple. A tuple pattern is closed: its
length must match, and `..` does not apply.

Closed `{ f }` and open `{ f, .. }` arms select concrete records statically
and erased values by runtime shape descriptor, and the tuple spelling `(a, b)`
works the same way over positional rows. A match over `any` requires its
catch-all, and the `..`-shadowing rule above is enforced. A row slot is a
pattern ([PATTERN-NESTED](#nested-patterns--pattern-nested)): `{ origin: { x, y }, .. }`
tests a nested row, `{ name: who }` renames, and `(0, label)` tests a tuple
slot. A field of an erased `any` row is itself `any`, so its slot binds or
ignores; matching it takes its own structural match.

## Nested patterns — [PATTERN-NESTED]

A pattern may appear wherever a pattern slot takes a binding: a positional or named constructor argument, a `Success`/`Error` payload, a tuple component, a list element and a structural row field. Patterns nest to any depth, and every form composes with every other: literal, binding, `_`, constructor, list, tuple and row.

```osprey
type Tree = Leaf | Node(Tree, Tree)
type MathError = DivZero | Overflow { by: int }

fn leftDepth(tree) = match tree {
    Node(Node(inner, _), _) => 2 + leftDepth(inner)
    Node(Leaf, _)           => 1
    Leaf                    => 0
}

fn explain(result) = match result {
    Success { value: 0 }               => "zero"
    Success { value }                  => "got ${value}"
    Error { message: Overflow { by } } => "over by ${by}"
    Error { message: DivZero }         => "divided by zero"
}
```

```osprey-ml
type Tree = Leaf | Node Tree Tree
type MathError = DivZero | Overflow(by : int)

leftDepth tree = match tree
    Node (Node inner _) _ => 2 + leftDepth inner
    Node Leaf _           => 1
    Leaf                  => 0

explain result = match result
    Success(value = 0)              => "zero"
    Success value                   => "got ${value}"
    Error(message = Overflow by)    => "over by ${by}"
    Error(message = DivZero)        => "divided by zero"
```

ML writes a pattern as it builds the value: positional slots by juxtaposition, each a binder, `_`, a payload-free variant, a literal or a parenthesised pattern, and named fields as `Ctor(field = pattern)`, which names every slot it lists. A Default brace slot is `field` or `field: pattern`.

A nested pattern tests and binds exactly as it would as the pattern of its own `match` on that slot's value: the slot's static type is the nested scrutinee's type, a binding in it follows [PATTERN-BINDING-SCOPE](#binding-scope--pattern-binding-scope), and a name may be bound once per arm. Arms are tried in order; an arm is selected when its whole pattern matches, and no binding of a failed arm is visible.

Exhaustiveness and reachability ([TYPE-MATCH-EXHAUSTIVE](#exhaustiveness-and-unreachable-arms-type-match-exhaustive)) are decided over the nested patterns as a whole. A `match` is rejected when some value reaches no arm, and the diagnostic shows one such value as a pattern, for example `Node(Leaf, Node(_, _))`. An arm that no value can reach because earlier arms cover it is rejected as unreachable.

## Exhaustiveness and unreachable arms [TYPE-MATCH-EXHAUSTIVE]

A match over `bool`, `Result`, or a known union must cover every case, either
explicitly or with a catch-all. A duplicate variant arm is unreachable, as is
every arm after `_` or a lower-case binding; the compiler rejects both.

A match over `any` is never exhaustive — its row is not known until run time —
so it always requires a catch-all
([TYPE-ANY](0004-TypeSystem.md#the-any-type--type-any)). Among structural arms,
a `..`-opened arm makes every later arm whose row extends it unreachable, and
the compiler rejects that shadowing rather than resolving it by arm order
([PATTERN-STRUCTURAL](#structural-patterns--pattern-structural)).

Matches over open scalar domains such as `int` need a catch-all when total
behavior is required, but the compiler does not prove scalar exhaustiveness.

## Result patterns

`Result<T, E>` uses the built-in `Success { value }` and `Error { message }`
variants:

```osprey
let calculation = checkedAdd(9223372036854775807, 1)

match calculation {
    Success { value }   => print("result=${value}")
    Error { message }   => print("error=${message}")
}
```

Arithmetic operators produce no `Result` and are matched no differently from any other total expression ([ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total)).

### Non-Result Scrutinees Auto-Wrap — [PATTERN-RESULT-AUTOWRAP]

A `Success`/`Error` pattern may be matched against a value that is not a
`Result`: the scrutinee is treated as if wrapped in `Success`, so `Success`
binds the value itself and the `Error` arm is unreachable. Both the type
checker and code generation apply this rule identically.

The `Success` arm is therefore taken **unconditionally** for a non-`Result`
scrutinee. A value's magnitude or sign MUST NOT select the arm — a negative
scalar is a `Success` payload, not an error sentinel. Under a
negative-means-error heuristic `match -1 { Success { value } => value ... }`
would take the `Error` arm and `abs(-1)` would observe `0`, silently wrong on
every negative value.

This rule governs hand-written `Success`/`Error` arms only. `?:` does **not**
inherit it: its scrutinee must be a real `Result`, and a plain value on its
left is a compile-time error
([PATTERN-RESULT-DEFAULT](#result-default---pattern-result-default)).

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
let safe = checkedAdd(10, 2) ?: 0
let failed = checkedAdd(9223372036854775807, 1) ?: 0
```

`?:` is right-associative and binds below every other operator
([Syntax](0003-Syntax.md#expressions)). The same spelling is available in ML.
It is an explicit handling operation: the result of `?:` is the unwrapped
success type, and the fallback must have that same plain type.

The scrutinee must be a `Result`; `?:` is not a boolean operator and never
reinterprets a plain value as `Success`. Use the ordinary boolean ternary for a
boolean condition. This separation prevents Result handling from becoming an
implicit truthiness conversion.

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
