# Syntax

This chapter defines the syntactic forms that make up an Osprey program. Semantics for individual constructs are in their dedicated chapters; cross-references are noted inline.

## Flavors

Osprey is one language with more than one way to write it. A **flavor** is a
surface syntax — a spelling of the language, not a dialect of it. Two flavors
exist today, and both are permanent and first-class:

- **Default** (`.osp`) — C-style: braces, parenthesised calls, named arguments.
- **ML** (`.ospml`) — ML-style: offside-rule layout, whitespace application,
  currying by default.

All syntax can be expressed in every flavor: anything you can write in one,
you can write in the others. The same function and call look like this in
each:

```osprey
fn add(x, y) = x + y
let sum = add(x: 2, y: 3)
```

```osprey-ml
add (x, y) = x + y
sum = add (2, 3)
```

And the same union type:

```osprey
type Shape = Circle    { radius: int }
           | Rectangle { width: int, height: int }
```

```osprey-ml
type Shape =
    Circle
        radius : int
    Rectangle
        width : int
        height : int
```

Those two spellings declare one type. This is the shape both of them mean:

```typediagram
union Shape {
  Circle { radius: Int }
  Rectangle { width: Int, height: Int }
}
```

Every flavor parses to the same canonical AST (`osprey_ast::Program`) before
any semantic analysis runs, so type checking, effects, and code generation are
identical whichever flavor a file is written in, and files in different
flavors mix freely in one project. These two need not be the last: a flavor is
a parser frontend over the shared core, so more may be added over time —
including flavors defined outside this specification. The
[FLAVOR-BOUNDARY](0023-LanguageFlavors.md) law is the contract every flavor,
present or future, must satisfy.

The rest of this chapter defines each construct in the Default spelling with
the ML spelling alongside wherever the two differ, as above. The ML flavor is
fully specified in [ML Flavor Syntax](0024-MLFlavorSyntax.md).

> Canonical mappings: `let`/`mut` → `Stmt::Let{mutable}`; `fn` →
> `Stmt::Function`; `extern` → `Stmt::Extern`; `type` → `Stmt::Type` +
> `TypeVariant`; `import` → `Stmt::Import`; `module` →
> `Stmt::Module{name, body}`; calls →
> `Expr::Call{function, arguments, named_arguments}`; `match` → `Expr::Match` +
> `MatchArm`; blocks → `Expr::Block{statements, value}`; field access →
> `Expr::FieldAccess`; indexing → `Expr::Index`; patterns → `Pattern::*`.
> Positional variants use `TypeVariant` whose `TypeField` names are decimal
> slot indices (`osprey_ast::positional_field_name`)
> ([TYPE-UNION-POSITIONAL](#type-declarations)); `e ?: d` uses the ternary
> `Expr::Match` with boolean-literal arms.

- [Flavors](#flavors)
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

```osprey-ml
module Geometry
    pi = 3.14159
    area r = pi * r * r
```

Module semantics for multi-file projects, exports, signatures, state modules,
and path-independent namespaces are defined in
[Modules and Namespaces](0025-ModulesAndNamespaces.md); the ML layout forms
(`module`, `namespace`) are in
[ML Flavor Syntax](0024-MLFlavorSyntax.md#modules-and-namespaces).

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
wildcard policy are defined in [Modules and Namespaces](0025-ModulesAndNamespaces.md#imports-modules-import).

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
param     ::= (ID | "_") (":" type)?      (* `_` — see [PARAM-WILDCARD] *)
effectSet ::= "!" effectRef | "!" "[" effectRef ("," effectRef)* "]"
effectRef ::= ID ("<" typeList ">")?
```

The optional `<typeParamList>` declares the function's type parameters
([TYPE-GENERICS-FN](0004-TypeSystem.md#generics-and-variance)); variance
markers are not permitted there. `effectSet` is the declared effect row —
each reference may apply type arguments to a generic effect
([EFFECTS-GENERIC-ROWS](0017-AlgebraicEffects.md#generic-effects)).

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

Effect sets (`!E`) are described in [Algebraic Effects](0017-AlgebraicEffects.md). Functions of two or more parameters require named arguments at call sites; see [Function Calls](0005-FunctionCalls.md).

A Default multi-parameter function such as `fn add(x, y) = x + y` lowers to a **single flat multi-parameter** `Stmt::Function`; it does **not** curry. (The ML flavor curries by default — `add x y` is nested single-parameter functions — and spells this flat form as `add (x, y)`; the two Default forms and their ML twins are defined in [Currying canonicalisation](0023-LanguageFlavors.md#currying-canonicalisation).)

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
variant           ::= ID ("{" fieldDeclarations "}" | "(" type ("," type)* ")")?
fieldDeclarations ::= fieldDeclaration ("," fieldDeclaration)*
fieldDeclaration  ::= ID ":" type constraint?
constraint        ::= "where" function_name
```

`typeParam`'s optional `in`/`out` marker declares the parameter's variance
([TYPE-VARIANCE-DECL](0004-TypeSystem.md#generics-and-variance)); `in` and
`out` are contextual keywords reserved only inside `<…>`
([Lexical Structure](0002-LexicalStructure.md#keywords)). A variant's payload is
either named fields or a parenthesised positional type list
([TYPE-UNION-POSITIONAL]); positional payloads are shared core, not ML sugar, so
the Default spelling is `Node(Tree, Tree)` and never synthesized `_0`/`_1`
fields. Positional constructors do **not** curry — `Node Leaf` is an arity
error, a deliberate exception to curry-by-default, since
`Expr::TypeConstructor` has no partial form — and diagnostics print them as
`Node/2`. A positionally-declared variant is **constructed** with that same
parenthesised, slot-ordered spelling — `Node(left, right)` — and destructured by
the matching pattern form ([Match Expressions](#match-expressions)). It is the
one call-shaped expression exempt from the named-argument rule for two or more
arguments ([Function Calls](0005-FunctionCalls.md#rules)), because a positional
payload has no field names to supply. A slot is recorded as a field whose
declared name is its decimal index (`0`, `1`) — not a spellable `_0` — so no
source can reach a slot by name and no generated slot name can collide with a
user-written field.

```osprey
type Color = Red | Green | Blue

type Shape = Circle    { radius: int }
           | Rectangle { width: int, height: int }

type Tree = Leaf | Node(Tree, Tree)
let tree  = Node(Node(Leaf, Leaf), Leaf)   // construction is positional too

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

Validation, non-destructive update (`record { field: value }`), and full field-access semantics are in [Type System](0004-TypeSystem.md).

## Expressions

```ebnf
expression          ::= ternaryExpression
ternaryExpression   ::= logicalOrExpression "{" pattern "}" "?" expression ":" ternaryExpression
                      | logicalOrExpression "?:" ternaryExpression
                      | logicalOrExpression                             (* right-assoc *)
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
lambdaExpression    ::= "|" paramList "|" "=>" expression        (* one or more params *)
                      | "fn" "(" paramList? ")" ("->" type)? "=>" expression

argumentList        ::= namedArgument ("," namedArgument)+
                      | expression ("," expression)*
namedArgument       ::= ID ":" expression
```

The two lambda spellings are interchangeable and lower to the same
`Expr::Lambda`; the ML flavor writes it `\x y => e`
([FLAVOR-ML-CURRY](0024-MLFlavorSyntax.md#functions-and-currying)). The pipe form
takes **at least one** parameter: `||` lexes as logical-or under maximal munch
([Lexical Structure](0002-LexicalStructure.md#operators)), so a zero-argument
lambda is written `fn() => e`.

`[PARAM-WILDCARD]` A parameter may be written `_` to declare an argument the
body ignores. It lowers to a `Parameter` carrying a generated, unspellable
name, so repeated `_`s never collide and none is referenceable. This is shared
core, not ML sugar — without it ML's `\(acc, _) => …` has no Default twin and the
pair breaks [FLAVOR-IR-EQUIV](0023-LanguageFlavors.md#cross-flavor-equivalence-tests).

```osprey
let total = fold(xs, 0, |acc, _| => acc + 1)
```

```osprey-ml
total = fold xs 0 (\(acc, _) => acc + 1)
```

Because a `_` parameter has no name to supply at the call site, it is usable in
a `fn` head only where the call is positional — arity one, or a lambda invoked
by a built-in ([Function Calls](0005-FunctionCalls.md#rules)).

Precedence, highest to lowest:

1. Unary `!`, `-`, `+`
2. Multiplicative `*`, `/`, `%`
3. Additive `+`, `-`
4. Comparison `==`, `!=`, `<`, `>`, `<=`, `>=`
5. Logical AND `&&`
6. Logical OR `||`
7. Ternary match `{ … } ? … : …` and Result default `?:` (right-associative)

Both ternary forms bind below every operator above, so `a + b ?: 0` parses as
`(a + b) ?: 0`. Their semantics and desugaring are in
[Ternary Match](0007-PatternMatching.md#ternary-match-syntactic-sugar).

Block expressions and their scoping are defined in [Block Expressions](0008-BlockExpressions.md). Pattern-matching for booleans (the only conditional construct) is in [Boolean Operations](0009-BooleanOperations.md).

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

Field access on `any`, `Result`, or any union type requires a `match` to narrow the value first. See [Type System](0004-TypeSystem.md) for the full rules.

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
              | ID "(" binder ("," binder)* ")"         (* positional constructor *)
              | ID ":" type                            (* type annotation *)
              | ID ":" "{" fieldPattern "}"            (* named structural *)
              | "{" fieldPattern "}"                   (* anonymous structural *)
              | "_"                                    (* wildcard *)
fieldPattern::= ID ("," ID)*
binder      ::= ID | "_"
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
are in [Pattern Matching](0007-PatternMatching.md). There is no grouped pattern
`(P)` in this flavor: braces already delimit every arm, so grouping would be
redundant. It exists only in the ML flavor, where whitespace application makes
`check Node l r` ambiguous
([FLAVOR-ML-PATTERN-GROUP](0024-MLFlavorSyntax.md#match)), and it erases at
parse time, so no AST node distinguishes the flavors. A positional
constructor pattern is legal only against a positionally-declared variant — a
named-field variant keeps its by-name binding, which is load-bearing because
codegen resolves each binder against the layout by name — and its arguments are
binders or `_` only: nested constructor patterns such as `Node (Node a b) c` are
rejected with a diagnostic.

## Variable Binding

- `let` creates an immutable binding; `mut` creates a mutable one.
- Every binding is initialised at declaration.
- Inner scopes may shadow outer bindings.
- Function arguments evaluate left to right before the call. `&&` and `||` short-circuit.
