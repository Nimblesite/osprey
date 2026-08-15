# ML Flavor Syntax

The ML flavor is Osprey's layout-based source syntax. Indentation delimits
blocks, whitespace applies curried functions, and all forms lower to the shared
`osprey_ast::Program` described by
[Language Flavors](0023-LanguageFlavors.md).

Select ML with `--flavor ml`, a `.ospml` extension, or a leading
`// osprey: flavor=ml` marker.

## Layout Model

`[FLAVOR-ML-LAYOUT]` The lexer derives `Indent`, `Dedent`, and `Newline` tokens
from an indentation stack. A line indented under a header continues its block;
a line at a lower column closes blocks until its indentation matches. Blank and
comment-only lines do not affect layout. Parentheses and brackets suppress
layout tokens until their matching delimiter.

The implementation is in `crates/osprey-syntax/src/ml/lexer.rs` and
`parser.rs`. Every token carries a source position.

## Comments

`[FLAVOR-ML-COMMENTS]` ML accepts:

- `//` line comments;
- nested `(* ... *)` block comments; and
- `(** ... *)` documentation comments attached to the following declaration.

An unterminated block comment is a syntax error. Empty and all-star block
comments are ordinary comments, not documentation.

## Bindings and Mutation

`[FLAVOR-ML-BIND]` `name = expression` introduces an immutable binding.
`mut name = expression` introduces a **mutable cell**, and `name := expression`
assigns to it. As in the Default flavor ([Bindings](0003-Syntax.md#bindings)),
`mut` is **not** a general imperative variable: a `mut` cell exists to back
**handler-owned state for algebraic effects**
([EFFECTS-HANDLER-STATE](0017-AlgebraicEffects.md#handler-owned-state)), mutated
*through* an effect handler rather than by free procedural `:=` reassignment.
The checker rejects every `:=` outside an effect handler arm.

```osprey-ml
answer = 42

mut requests = 0
total = handle Counter
    tick => requests := (requests + 1) ?: requests
in run ()
```

These lower to `Stmt::Let { mutable: false }`,
`Stmt::Let { mutable: true }`, and `Stmt::Assignment` respectively. Assignment
to an immutable binding is a type error.
Assignment to a mutable binding is also a type error unless it occurs in an
effect handler arm; the handled `in` body remains ordinary client code.

## Functions and Currying

`[FLAVOR-ML-FN]` A signature precedes its binding. Function arrows associate to
the right. Checked integer arithmetic keeps its `Result` return in both written
and inferred signatures
([ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked)).

```osprey-ml
inc : int -> Result<int, MathError>
inc x = x + 1

add : int -> int -> Result<int, MathError>
add x y = x + y
```

`[FLAVOR-ML-CURRY]` Whitespace parameters curry. `add x y = body` lowers to a
one-parameter `Stmt::Function` whose body is a one-parameter `Expr::Lambda`.
`add 1 2` lowers to nested one-argument calls. `add 1` therefore returns the
remaining function.

[Critical issue #184](https://github.com/Nimblesite/osprey/issues/184) currently
qualifies this rule for effects: an unannotated four-argument curried ML
function can silently skip operations performed through its body. The
equivalent flat parameter form works. Until the lowering bug is fixed, write
effectful functions of that shape with parenthesised comma-separated
parameters.

Parenthesised comma-separated parameters are explicitly flat:

```osprey-ml
add : (int, int) -> Result<int, MathError>
add (x, y) = x + y

sum = add (10, 20)
```

The flat binding lowers to one two-parameter `Stmt::Function`; the call lowers
to one two-argument `Expr::Call`. `sum` retains the complete
`Result<int, MathError>` return; neither flat nor curried application unwraps
it. The parenthesised list is a tuple
([FLAVOR-ML-TUPLE](#match)), and a tuple applied to a
known head is exactly this flat call — which is why the ML and Default twins
share IR.

Lambdas follow the same split: `\x y => body` is curried and
`\(x, y) => body` is flat. `name () = body` is a zero-parameter function;
`name = body` is a value binding.

`[FLAVOR-ML-CLAUSES]` Adjacent same-name bindings with a refutable parameter
form one function by cases:

```osprey-ml
make 0 = Leaf
make depth =
    next = (depth - 1) ?: 0
    Node (make next) (make next)
```

The clause group lowers to one function whose body is `Expr::Match`. A group
must have one arity, one optional signature before the first clause, and at most
one parameter column containing refutable patterns. Separated same-name
bindings are not merged.

## Function Calls

`[FLAVOR-ML-CALL]` Whitespace application is left-associative:

```text
f a b       -> Call(Call(f, [a]), [b])
f (a, b)    -> Call(f, [a, b])
f (a)       -> Call(f, [a])
```

Parentheses are also used for grouping and to delimit lambdas passed as
arguments.

## Collections and Indexing

`[FLAVOR-ML-LIST]` Lists use `[a, b, c]`; `[]` is empty. A trailing comma is
accepted. They lower to `Expr::List`.

`[FLAVOR-ML-MAP]` Maps use `[key => value, ...]`; `[=>]` is the explicit empty
map. They lower to `Expr::Map`, the same node as a Default `{ key: value }`
literal.

`[FLAVOR-ML-INDEX]` A bracket indexes only when it is adjacent to its receiver:
`xs[0]`. `xs [0]` is application with a list argument. Indexing lowers to
`Expr::Index`.

## Modules and Namespaces

`[FLAVOR-ML-MODULES]` Module semantics are specified in
[Modules and Namespaces](0025-ModulesAndNamespaces.md). ML uses layout for
namespace, module, state-module, signature, and import bodies.

```osprey-ml
namespace billing

signature TaxApi
    addTax : int -> Result<int, MathError>

module Tax : TaxApi
    addTax cents = cents + 1

import billing::Tax
    addTax

gross = addTax 100
```

A namespace without an indented body is file-scoped. An ascribed module exports
exactly its signature; explicit `export` inside it is rejected. An unascribed
module marks public declarations with `export`. `state Name` is the ML spelling
of a state module. `::` qualifies logical symbols; `.` accesses a value field.
Here `gross` is `Result<int, MathError>`; module ascription and import boundaries
preserve the exported failure channel.

Imports support whole targets, `as` aliases, indented member selection with
optional member aliases, and an indented `*` wildcard. Quoted namespace labels
must be imported with an alias.

## External Functions

`[FLAVOR-ML-EXTERN]` An external declaration names each parameter inside its
own parentheses. The return type is optional.

```osprey-ml
extern puts (text : string) -> int
extern log (message : string)
```

This lowers to `Stmt::Extern`, including the written parameter names and order.

## Effects

`[FLAVOR-ML-EFFECT]` Effect operations use `=>` between payload and result.
Zero-payload operations use `Unit`.

```osprey-ml
effect Db
    add : string => int
    count : Unit => int

created = perform Db.add "buy milk"
total = perform Db.count ()
```

An effect declaration lowers to `Stmt::Effect`; a performance lowers to
`Expr::Perform`. `resume` and `resume value` lower to `Expr::Resume` inside a
handler arm.

## Handlers

`[FLAVOR-ML-HANDLER]` A handler is lexical: it names an effect,
declares its arms, and handles one body after `in`.

```osprey-ml
result =
    handle Db
        add task => resume 1
        count => resume 0
    in
        perform Db.add "buy milk"
```

This lowers directly to `Expr::Handler { effect, arms, body }`.

`handler Effect` values, the `Handler Effect` type, and `handle values do body`
do not exist in the canonical AST. `handler` and `do` are reserved and produce
a `not yet supported` syntax error
([FLAVOR-HANDLER-VALUE](0023-LanguageFlavors.md#shared-core-additions)).

## Generics ([FLAVOR-ML-GENERICS])

Generic declarations lower to the same variance-carrying `TypeParam` and
`EffectRef` nodes as Default syntax.

- Types use juxtaposed binders: `type Box T`, `type Feed out T`,
  `type Sink in T`.
- Effects use the same binder form: `effect Stash T`.
- Function binders appear on a signature: `pick<T> : (T, T) -> T`.
- Effect rows apply arguments with angles: `! Stash<int>` or
  `! [Read<T>, Write<T>]`.
- Construction-site type arguments use `Box<int>(item = 7)`.

Function binders do not accept variance. A binding without a signature cannot
declare function type parameters.

## Type Declarations

`[FLAVOR-ML-TYPE]` A type may be a record, a manifest alias, or a union.

```osprey-ml
type Point =
    x : int
    y : int

type UserId = int
```

Function-typed fields parenthesise the input list: `check : (int) -> bool`.

`[FLAVOR-ML-UNION-INLINE]` Inline union variants are separated by `|`.
Payloads may be positional or named:

```osprey-ml
type Tree = Leaf | Node Tree Tree
type Shape = Circle float | Rect(width : float, height : float)
```

`|` is a type-declaration separator, not an expression operator or or-pattern.
Layout variants remain available. Positional payloads lower to numeric internal
field names shared with Default positional unions.

## Match

`[FLAVOR-ML-MATCH]` A match has an indented list of `pattern => body` arms.

```osprey-ml
label result =
    match result
        Success value => value
        Error message => message
```

Patterns include `_`, literals, bindings, constructor payload binders, and list
patterns such as `[]`, `[one]`, and `[head, ...tail]`. Nested constructor
patterns and or-patterns are rejected; bind the inner payload and match again.

A constructor payload binder takes its **column**, not the field that shares its
spelling, so `Node l r` may rename freely against a named payload as well as a
positional one — ML's one form is Default's `Node(l, r)`
([Union patterns](0007-PatternMatching.md#union-patterns)). `Success`/`Error` are
the exception, binding by role.

`[FLAVOR-ML-PATTERN-GROUP]` Parentheses around a **single** pattern group it and
disappear during parsing. They allow a constructor pattern in a clause head:

```osprey-ml
size : Tree -> Result<int, MathError>
size Leaf = Success(value = 0)
size (Node left right) = 1 + size left + size right
```

`[FLAVOR-ML-TUPLE]` A parenthesised comma list is a tuple, as in ML: `f (a, b)`
applies `f` to one tuple and `f a b` is curried application of two arguments.
In *pattern* position the same spelling reads a positional row
([TYPE-TUPLE](0004-TypeSystem.md#tuples--type-tuple)):

```osprey-ml
type Pair = Pair int string

describe : any -> string
describe v =
    match v
        (n, label) => "${label}=${n}"
        _ => "unknown"
```

ML's `*` type spelling (`int * string`) is not implemented — `*` is not a type
operator, and a tuple type has no surface syntax in either flavor.

A tuple destructured in a clause head lowers to a flat parameter list, so ML's
`pair (n, label)` and Default's `fn pair(n, label)` are the same function and
emit the same IR — the tupled head is what keeps the twins equivalent, where
curried application lowers to closures ([FLAVOR-IR-EQUIV],
[FLAVOR-ML-CURRY](#functions-and-currying)).

## Records

`[FLAVOR-ML-RECORD]` Named records and variants may use layout or an inline
field list:

```osprey-ml
point =
    Point
        x = 10
        y = 20

other = Point(x = 30, y = 40)
updated = point(x = 50)
```

Uppercase heads lower to `Expr::TypeConstructor`. A lowercase inline head is a
non-destructive record update and lowers to `Expr::Update`.

`[FLAVOR-ML-RECORD-ANON]` A headless brace literal is an anonymous record
([TYPE-RECORD-ANON](0004-TypeSystem.md#anonymous-records--type-record-anon)),
written ML-style with `=` between field and value. Braces are unambiguous in
this flavor because ML maps use `[k => v]` ([FLAVOR-ML-MAP](#collections-and-indexing)):

```osprey-ml
origin = { x = 0, y = 0 }
```

> **Status:** the inline brace record is not implemented — the ML expression
> parser rejects `{` (`unexpected token LBrace in expression`, pinned by
> `examples/failscompilation/ml_brace_record_and_question_sigil.ospo`).
> Construct records with the layout or parenthesised `Name(field = value)`
> forms. The brace structural *pattern* below is implemented, including `..`.

Structural patterns use the same spelling with binders in place of values, and
`..` opens the row
([PATTERN-STRUCTURAL](0007-PatternMatching.md#structural-patterns--pattern-structural)):

```osprey-ml
title page =
    match page
        { heading, .. } => heading
        _ => "untitled"
```

`[FLAVOR-ML-CTOR-POSITIONAL]` A positionally-declared variant is constructed
and matched by juxtaposition:

```osprey-ml
tree = Node Leaf Leaf
depth : Tree -> Result<int, MathError>
depth Leaf = Success(value = 0)
depth (Node left right) = 1 + depth left + depth right
```

Constructors must be saturated; they do not curry. Positional patterns apply
only to positionally-declared payloads.

## Fibers and Channels

`[FLAVOR-ML-SPAWN]` `spawn expression` or `spawn` followed by an indented block
lowers to `Expr::Spawn`.

`[FLAVOR-ML-CONCURRENCY]` The remaining forms are `await fiber`, bare or
valued `yield`, `send channel value`, and `recv channel`. Compound operands are
parenthesised. They lower to the corresponding shared AST nodes.

```osprey-ml
fiber = spawn work 1
result = await fiber
send channel result
next = recv channel
yield next
```

## Blocks

`[FLAVOR-ML-BLOCK]` A function body, match arm, handler arm, or spawned body may
be an indented sequence. Its final expression is the block value; preceding
lines are statements. It lowers to `Expr::Block { statements, value }`.

## Canonical Lowering Table

| ML surface | Canonical AST |
| --- | --- |
| `x = e` / `mut x = e` / `x := e` | `Stmt::Let` / mutable `Stmt::Let` / `Stmt::Assignment` |
| `f x y = e` / `f (x, y) = e` | curried chain / tuple head, flat `Stmt::Function` |
| `f a b` / `f (a, b)` | nested calls / one flat call on a tuple |
| `[a, b]` / `[k => v]` / `xs[i]` | `Expr::List` / `Expr::Map` / `Expr::Index` |
| `{ x = e }` / `{ x, .. }` | `Expr::Record` / structural pattern |
| `namespace`, `module`, `state`, `signature`, `import` | shared project AST nodes |
| `extern f (x : T) -> U` | `Stmt::Extern` |
| `type`, inline unions | `Stmt::Type` and `TypeVariant` |
| `match` and equational clauses | `Expr::Match` |
| uppercase record head / lowercase update head | `Expr::TypeConstructor` / `Expr::Update` |
| `effect`, `perform`, lexical `handle`, `resume` | shared effect AST nodes |
| `spawn`, `await`, `yield`, `send`, `recv` | shared concurrency AST nodes |
