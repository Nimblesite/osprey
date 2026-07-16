---
layout: page
title: "Syntax"
description: "Osprey Language Specification: Syntax"
date: 2026-07-15
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0003-syntax/"
---

# Syntax

This chapter defines the syntactic forms that make up an Osprey program. Semantics for individual constructs are in their dedicated chapters; cross-references are noted inline.

> **Flavor layer — surface (CST).**  The syntactic forms here are surface spellings only; the **semantics and lowering are shared-core and flavor-blind** — every spelling collapses into the one canonical AST (`osprey_ast::Program`), the single tree every later phase consumes ([FLAVOR-BOUNDARY]). This chapter shows **both flavors**: the Default (`.osp`) spelling — C-style braces, `fn`, and `f(x: a, y: b)` named-argument calls — and, wherever the surface actually differs, the **ML (`.ospml`) twin shown inline alongside it** in `osprey-ml` blocks (offside layout, `\x => e`, whitespace application). Both spellings lower to the *same* AST nodes; see [ML Flavor Syntax](/spec/0024-mlflavorsyntax/) for the full ML counterpart of each form, and [Language Flavors](/spec/0023-languageflavors/) for the one-AST-many-CSTs model and the [FLAVOR-BOUNDARY] law.
>
> The major forms below map to canonical AST nodes as follows: `let`/`mut` → `Stmt::Let{mutable}`; `fn` → `Stmt::Function`; `extern` → `Stmt::Extern`; `type` → `Stmt::Type` + `TypeVariant`; `import` → `Stmt::Import`; `module` → `Stmt::Module{name, body}`; calls → `Expr::Call{function, arguments, named_arguments}`; `match` → `Expr::Match` + `MatchArm`; `{ … }` blocks → `Expr::Block{statements, value}`; field access → `Expr::FieldAccess`; indexing → `Expr::Index`; and the pattern forms → `Pattern::*` (`Wildcard`, `Literal`, `Constructor`, `TypeAnnotated`, `Structural`, `Binding`). Names and shapes are flavor-blind from the AST upward.

- [Program Structure](#program-structure)
- [Imports](#imports)
- [Let Declarations](#let-declarations)
- [Function Declarations](#function-declarations)
- [Extern Declarations](#extern-declarations)
- [Type Declarations](#type-declarations)
- [Records](#records)
- [Expressions](#expressions)
- [Field Access](#field-access)
- [Match Expressions](#match-expressions)
- [Variable Binding](#variable-binding)

## Program Structure

```ebnf
program   ::= statement* EOF
statement ::= importStmt
            | letDecl
            | fnDecl
            | externDecl
            | typeDecl
            | moduleDecl
            | exprStmt

moduleDecl ::= "module" ID "{" statement* "}"
```

A `moduleDecl` groups declarations under a namespace and lowers to
`Stmt::Module { name, body }`:

```osprey
module Geometry {
    let pi   = 3.14159
    fn area(r) = pi * r * r
}
```

> `module` has no ML-flavor surface syntax; see
> [Modules and Namespaces](/spec/0025-modulesandnamespaces/).

Module semantics for multi-file projects, exports, signatures, state modules,
and path-independent namespaces are defined in
[Modules and Namespaces](/spec/0025-modulesandnamespaces/).

## Imports

```ebnf
importStmt ::= "import" ID ("." ID)*
```

```osprey
import std
import std.io
import graphics.canvas
```

Import semantics for multi-file projects, aliases, explicit member imports, and
wildcard policy are defined in [Modules and Namespaces](/spec/0025-modulesandnamespaces/#imports).

## Let Declarations

```ebnf
letDecl ::= ("let" | "mut") ID (":" type)? "=" expr
```

```osprey
let x       = 42
let name    = "Alice"
mut counter = 0
let result  = calculateValue(input: data)
```

```osprey-ml
x       = 42
name    = "Alice"
mut counter = 0
result  = calculateValue data
```

`let` binds immutably; `mut` binds mutably. Type annotations are optional.

## Function Declarations

```ebnf
fnDecl    ::= docComment? "fn" ID ("<" typeParamList ">")? "(" paramList? ")"
              ("->" type)? effectSet?
              ("=" expr | "{" blockBody "}")
paramList ::= param ("," param)*
param     ::= ID (":" type)?
effectSet ::= "!" effectRef | "!" "[" effectRef ("," effectRef)* "]"
effectRef ::= ID ("<" typeList ">")?
```

The optional `<typeParamList>` declares the function's type parameters
([TYPE-GENERICS-FN](/spec/0004-typesystem/#generics-and-variance)); variance
markers are not permitted there. `effectSet` is the declared effect row —
each reference may apply type arguments to a generic effect
([EFFECTS-GENERIC-ROWS](/spec/0017-algebraiceffects/#generic-effects)).

```osprey
fn double(x)   = x * 2
fn add(x, y)   = x + y
fn greet(name) = "Hello " + name
fn getValue()  = 42
```

```osprey-ml
double x   = x * 2
add (x, y) = x + y
greet name = "Hello " + name
getValue () = 42
```

Effect sets (`!E`) are described in [Algebraic Effects](/spec/0017-algebraiceffects/). Functions of two or more parameters require named arguments at call sites; see [Function Calls](/spec/0005-functioncalls/).

A Default multi-parameter function such as `fn add(x, y) = x + y` lowers to a **single flat multi-parameter** `Stmt::Function`; it does **not** curry. (The ML flavor curries by default — `add x y` is nested single-parameter functions — and spells this flat form as `add (x, y)`; the two Default forms and their ML twins are defined in [Currying canonicalisation](/spec/0023-languageflavors/#currying-canonicalisation).)

## Extern Declarations

`extern` declares an interface to a foreign function (Rust, C, or any C-ABI library). It has no body.

```ebnf
externDecl      ::= docComment? "extern" "fn" ID "(" externParamList? ")" ("->" type)?
externParamList ::= externParam ("," externParam)*
externParam     ::= ID ":" type
```

Parameter types are required. Calls use named arguments (single-parameter functions may use positional).

```osprey
extern fn rust_add(a: int, b: int) -> int
extern fn rust_is_prime(n: int) -> bool

let sum     = rust_add(a: 15, b: 25)
let isPrime = rust_is_prime(17)
```

```osprey-ml
extern rust_add (a : int) (b : int) -> int
extern rust_is_prime (n : int) -> bool

sum     = rust_add 15 25
isPrime = rust_is_prime 17
```

ABI mapping:

| Osprey   | Rust                | C            |
| -------- | ------------------- | ------------ |
| `int`    | `i64`               | `int64_t`    |
| `bool`   | `bool`              | `bool`       |
| `string` | `*const c_char`     | `char *`     |

The foreign function must use the C ABI (`extern "C"` and `#[no_mangle]` in Rust) and be linked at compile time.

## Type Declarations

```ebnf
typeDecl          ::= docComment? "type" ID ("<" typeParamList ">")? "=" (unionType | recordType)
typeParamList     ::= typeParam ("," typeParam)*
typeParam         ::= ("in" | "out")? ID
unionType         ::= variant ("|" variant)*
recordType        ::= "{" fieldDeclarations "}"
variant           ::= ID ("{" fieldDeclarations "}")?
fieldDeclarations ::= fieldDeclaration ("," fieldDeclaration)*
fieldDeclaration  ::= ID ":" type constraint?
constraint        ::= "where" function_name
```

`typeParam`'s optional `in`/`out` marker declares the parameter's variance
([TYPE-VARIANCE-DECL](/spec/0004-typesystem/#generics-and-variance)); `in` and
`out` are contextual keywords reserved only inside `<…>`
([Lexical Structure](/spec/0002-lexicalstructure/#keywords)).

```osprey
type Color = Red | Green | Blue

type Shape = Circle    { radius: int }
           | Rectangle { width: int, height: int }

type Pair<T, U>  = Pair { first: T, second: U }
type Feed<out T> = Feed { supply: T } | Dry
type Gate<in T>  = Gate { admit: (T) -> bool } | Open
```

```osprey-ml
type Color =
    Red
    Green
    Blue

type Shape =
    Circle
        radius : int
    Rectangle
        width : int
        height : int
```

## Records

A record type names a fixed set of fields. Construction uses `TypeName { field: value, ... }`; field order at the call site is irrelevant.

```osprey
type Point  = { x: int, y: int }
type Person = { name: string, age: int } where validatePerson

let point  = Point { x: 10, y: 20 }
let person = Person { name: "Alice", age: 25 }
```

```osprey-ml
type Point =
    x : int
    y : int

type Person =
    name : string
    age : int

point  = Point(x = 10, y = 20)
person = Person(name = "Alice", age = 25)
```

Validation, non-destructive update (`record { field: value }`), and full field-access semantics are in [Type System](/spec/0004-typesystem/).

## Expressions

```ebnf
expression          ::= logicalOrExpression
logicalOrExpression ::= logicalAndExpression ("||" logicalAndExpression)*
logicalAndExpression::= comparisonExpression ("&&" comparisonExpression)*
comparisonExpression::= additiveExpression (("==" | "!=" | "<" | ">" | "<=" | ">=") additiveExpression)*
additiveExpression  ::= multiplicativeExpression (("+" | "-") multiplicativeExpression)*
multiplicativeExpression ::= unaryExpression (("*" | "/" | "%") unaryExpression)*
unaryExpression     ::= ("+" | "-" | "!")? pipeExpression
pipeExpression      ::= callExpression ("|>" callExpression)*
callExpression      ::= primaryExpression (
                          "." ID "(" argumentList? ")"
                        | "(" argumentList? ")"
                        | "[" expression "]"
                        | "." ID
                      )*
primaryExpression   ::= literal | ID | "(" expression ")"
                      | lambdaExpression | blockExpression | matchExpression

argumentList        ::= namedArgument ("," namedArgument)+
                      | expression ("," expression)*
namedArgument       ::= ID ":" expression
```

Precedence, highest to lowest:

1. Unary `!`, `-`, `+`
2. Multiplicative `*`, `/`, `%`
3. Additive `+`, `-`
4. Comparison `==`, `!=`, `<`, `>`, `<=`, `>=`
5. Logical AND `&&`
6. Logical OR `||`

Block expressions and their scoping are defined in [Block Expressions](/spec/0008-blockexpressions/). Pattern-matching for booleans (the only conditional construct) is in [Boolean Operations](/spec/0009-booleanoperations/).

## List Access

```ebnf
listAccess ::= expression "[" expression "]"
```

Indexing returns `Result<T, IndexError>`:

```osprey
let numbers = [1, 2, 3, 4]

match numbers[0] {
    Success { value }   => print("first: ${value}")
    Error   { message } => print("index error: ${message}")
}
```

```osprey-ml
numbers = [1, 2, 3, 4]

match numbers[0]
    Success value   => print "first: ${value}"
    Error   message => print "index error: ${message}"
```

## Field Access

```ebnf
fieldAccess ::= expression "." ID
```

Fields are accessible directly only on record values:

```osprey
type User = { id: int, name: string }
let user  = User { id: 1, name: "Alice" }
let n     = user.name
```

```osprey-ml
type User =
    id : int
    name : string

user  = User(id = 1, name = "Alice")
n     = user.name
```

Field access on `any`, `Result`, or any union type requires a `match` to narrow the value first. See [Type System](/spec/0004-typesystem/) for the full rules.

Records are immutable. Use the non-destructive update form to produce a modified copy:

```osprey
let p2 = point { x: 15 }   // y carried over
```

```osprey-ml
p2 = point(x = 15)   // y carried over
```

## Match Expressions

```ebnf
matchExpr   ::= "match" expr "{" matchArm+ "}"
matchArm    ::= pattern "=>" expr
pattern     ::= unaryExpr                              (* literals incl. -1, +42 *)
              | ID ("{" fieldPattern "}")?             (* constructor / destructure *)
              | ID "(" pattern ("," pattern)* ")"      (* positional constructor *)
              | ID ":" type                            (* type annotation *)
              | ID ":" "{" fieldPattern "}"            (* named structural *)
              | "{" fieldPattern "}"                   (* anonymous structural *)
              | "_"                                    (* wildcard *)
fieldPattern::= ID ("," ID)*
```

```osprey
type Status = Ready | Running | Done { code: int }

let label = match status {
    Ready          => "ready"
    Running        => "running"
    Done { code }  => "done (${code})"
}
```

```osprey-ml
type Status =
    Ready
    Running
    Done
        code : int

label =
    match status
        Ready       => "ready"
        Running     => "running"
        Done code   => "done (${code})"
```

Pattern semantics, exhaustiveness, and the two-arm shorthands — the ternary
and the Default-flavor `if`/`else if`/`else` expression ([GRAMMAR-IF-ELSE]) —
are in [Pattern Matching](/spec/0007-patternmatching/).

## Variable Binding

- `let` creates an immutable binding; `mut` creates a mutable one.
- Every binding is initialised at declaration.
- Inner scopes may shadow outer bindings.
- Function arguments evaluate left to right before the call. `&&` and `||` short-circuit.