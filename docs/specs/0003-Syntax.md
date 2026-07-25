# Syntax

This chapter uses Default spelling unless it shows an ML equivalent. Detailed
semantics live in the linked chapters.

## Flavors

Default (`.osp`) uses braces, `fn`, and parenthesized calls. ML (`.ospml`) uses
layout, whitespace application, and currying by default. Each frontend lowers
to the same `osprey_ast::Program` before semantic analysis.

Shared AST does not mean every surface form exists in both flavors. Default has
`if` and structural ternary spellings that ML omits. The
surface/core contract is [FLAVOR-BOUNDARY](0023-LanguageFlavors.md); complete ML
grammar is in [ML Flavor Syntax](0024-MLFlavorSyntax.md). Multi-file and
cross-flavor project limits are specified in
[Modules and Namespaces](0025-ModulesAndNamespaces.md).

## Program Structure

A file contains declarations and expression statements. Declarations include
bindings, functions, externs, types, effects, imports,
namespaces, modules, and module signatures. Module and import forms are
specified in [Modules and Namespaces](0025-ModulesAndNamespaces.md); effect
forms are specified in [Algebraic Effects](0017-AlgebraicEffects.md).

## Bindings

Default uses `let` for an immutable binding and `mut` for a rebindable binding.
Reassignment uses `=` and is rejected for an immutable or undeclared name.

```osprey
let name = "Alice"
mut count = 0
count = count + 1
```

ML omits `let`; its reassignment operator is `:=`.

```osprey-ml
name = "Alice"
mut count = 0
count := count + 1
```

A binding may include a type annotation after `:`. The annotation constrains
inference; it is not required when inference already fixes the type.

## Function Declarations

```ebnf
function ::= "fn" identifier typeParameters? "(" parameters? ")"
             ("->" type)? effectSet? ("=" expression | block)
parameter ::= (identifier | "_") (":" type)?
```

Default functions have flat parameter lists. Calls are described in
[Function Calls](0005-FunctionCalls.md).

```osprey
fn double(x) = x * 2
fn add(x, y) = x + y
fn getValue() = 42
```

### Ignored parameters [PARAM-WILDCARD]

`_` declares a parameter the body cannot reference. Lowering gives each `_` a
distinct unspellable internal name, so repeated ignored parameters do not
collide.

```osprey
let count = range(0, 10) |> fold(0, |acc, _| => acc + 1)
```

A named function can use `_` only where its caller supplies arguments
positionally; there is no source name for a named argument to target.

## Extern Declarations

An extern declares a C-ABI symbol and has no body. Parameter types are required;
an omitted return type means `Unit`.

```ebnf
extern ::= "extern" "fn" identifier "(" externParameters? ")" ("->" type)?
```

```osprey
extern fn sqlite3_open(filename: string, ppDb: Ptr) -> int
```

Supported ABI types, callbacks, linking, and the memory-safety boundary are in
[Foreign Function Interface](0019-ForeignFunctionInterface.md).

## Type Declarations

A type declaration defines a record, a union, or a type alias. Unions may have
nullary variants, named-field payloads, or positional payloads.

```osprey
type Point = { x: int, y: int }
type Shape = Circle { radius: int } | Rectangle { width: int, height: int }
type Color = Red | Green | Blue
```

Type parameters and variance are defined in
[Type System](0004-TypeSystem.md#generics-and-variance).

### Positional variants [TYPE-UNION-POSITIONAL]

A positional payload is declared, constructed, and matched in slot order:

```osprey
type Tree = Leaf | Node(Tree, Tree)
let tree = Node(Node(Leaf, Leaf), Leaf)

fn size(tree) = match tree {
    Leaf          => 1
    Node(left, _) => 1 + size(left)
}
```

Positional constructors require every slot and do not partially apply. Their
slots have no source field names, so named arguments and field access cannot
target them. Nested constructor patterns are not implemented; each positional
pattern slot is a binding or `_`.

## Records

Construction supplies every declared field by name. Construction-site field
order does not change which value is assigned to each field.

```osprey
type Person = { name: string, age: int }
let person = Person { age: 25, name: "Alice" }
let older = person { age: 26 }
```

Records are immutable; `record { field: value }` creates a modified copy.
Field typing is specified in [Type System](0004-TypeSystem.md). The parsed
`where` validation suffix is rejected during type checking.

## Expressions

Default supports literals, names, calls, lambdas, blocks, field access,
indexing, pipes, arithmetic and boolean operators, matches, ternaries, effects,
and concurrency expressions.

The relevant precedence, highest to lowest, is:

1. Postfix call, field access, and indexing
2. Pipe `|>`
3. Unary `!`, `-`, `+`
4. Multiplicative `*`, `/`, `%`
5. Additive `+`, `-`
6. Comparison `==`, `!=`, `<`, `>`, `<=`, `>=`
7. Logical AND `&&`
8. Logical OR `||`
9. Ternary `? :` and Result default `?:`, both right-associative

`x |> f(a)` lowers to `f(x, a)`. The two Default lambda spellings lower to
`Expr::Lambda`:

```osprey
let one = |x| => x + 1
let zero = fn() => 0
```

The pipe-delimited form requires at least one parameter because `||` is the
logical-OR token.

## Indexing

Postfix indexing is available on lists, maps, and strings. It returns
`Result<T, Error>` (or `Result<string, Error>` for a string), so callers can
handle an invalid index or absent key.

```osprey
match values[0] {
    Success { value }   => print(value)
    Error { message }   => print(message)
}
```

## Field Access

`value.field` reads a record field. A union payload must first be narrowed by a
constructor pattern. Record update uses `value { field: replacement }`.

## Match Expressions

```ebnf
match ::= "match" expression "{" arm+ "}"
arm   ::= pattern "=>" expression
```

Patterns are scalar literals, `_`, a lower-case binding, union
constructors with named or positional payloads, and list patterns. Pattern
semantics and exhaustive matching are defined in
[Pattern Matching](0007-PatternMatching.md).

`name: Type` is accepted as a typed binding. The backend treats it as
a catch-all binding rather than a runtime type test, so it is valid only when
the scrutinee already has that static type. Standalone structural record
patterns are not implemented; the Default structural ternary is a separate
lowering.

## Evaluation order

Statements and positional call arguments evaluate left to right. `&&` and `||`
short-circuit. A named call is reordered to parameter declaration order before
its argument expressions are lowered.
