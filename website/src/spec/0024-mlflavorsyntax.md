---
layout: page
title: "ML Flavor Syntax"
description: "Osprey Language Specification: ML Flavor Syntax"
date: 2026-07-15
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0024-mlflavorsyntax/"
---

# ML Flavor Syntax

The **ML flavor** is a layout-based source surface for Osprey: indentation
delimits blocks, functions **curry by default** (whitespace application reads as
curried and lowers to the Default flavor's explicit-curry nested-lambda shape),
and effect handlers are first-class values. It is one of Osprey's [language flavors](/spec/0023-languageflavors/) — a
parsing-and-lowering profile, not a separate language. Every construct here
lowers to the same `osprey_ast::Program` the Default (brace) flavor produces,
and from there shares one type checker, effect checker, and backend.

This chapter is the **surface reference**. The boundary rules, the lowering
contract, currying canonicalisation, and the shared-core handler-value feature
are normative in [Language Flavors](/spec/0023-languageflavors/); this chapter is
subordinate to that contract. Implementation is tracked in
[plan 0013](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0013-ml-flavor-frontend.md).

- [Status](#status)
- [Layout Model](#layout-model)
- [Bindings and Mutation](#bindings-and-mutation)
- [Functions and Currying](#functions-and-currying)
- [Function Calls](#function-calls)
- [Modules and Namespaces](#modules-and-namespaces)
- [Effects](#effects)
- [Handlers](#handlers)
- [Match](#match)
- [Records](#records)
- [Blocks](#blocks)
- [Canonical Lowering Table](#canonical-lowering-table)
- [Worked Example](#worked-example)
- [Resolved Syntax Questions](#resolved-syntax-questions)
- [References](#references)

## Status

**Partially implemented; in active development.** The Default flavor (specs
`0001`–`0022`) remains the primary frontend. Select the ML surface with
`--flavor ml`, the `.ospml` extension, or a `// osprey: flavor=ml` marker (see
[Flavor Selection](/spec/0023-languageflavors/#flavor-selection)).

- **Phase 1 — flavor frontend seam: implemented and green.** The `Flavor`
  enum, `Parsed.flavor`, and `parse_program_with_flavor` are live, with
  `parse_program` kept as the `Flavor::Default` specialisation.
- **Phase 4 — flavor selection: implemented and green.** The CLI
  `--flavor default|ml` flag, the `.ospml` extension, and the
  `// osprey: flavor=ml` marker are resolved by the precedence
  flag > marker > extension > Default, with a hard error when extension and
  marker disagree. The differential harness
  ([`crates/diff_examples.sh`](https://github.com/Nimblesite/osprey/blob/main/crates/diff_examples.sh)) discovers
  `.ospml` fixtures **additively**, leaving every existing `.osp` example
  untouched.
- **Phases 2–3 — ML lexer/parser/lowerer: in active development.** The frontend
  is a hand-written Rust layout lexer + recursive-descent (Pratt /
  precedence-climbing) parser in
  [`crates/osprey-syntax/src/ml/`](https://github.com/Nimblesite/osprey/blob/main/crates/osprey-syntax/src/ml/) (see
  [`[FLAVOR-ML-LAYOUT]`](#layout-model)).
- **Modules and namespaces: implemented.** File/block namespaces, layout
  modules and signatures, concise state modules, explicit exports, layout
  imports, and `::` symbol paths lower to the shared module AST specified by
  [Modules and Namespaces](/spec/0025-modulesandnamespaces/).
- **First-class handler values: deferred.** ML effect declarations,
  `perform`, and the existing fused `handle ... in ...` form are implemented;
  only reusable handler values/installers await the shared-core feature.

The parsing techniques and the offside rule are cited in the
[References](#references) section.

## Layout Model

`[FLAVOR-ML-LAYOUT]` The ML flavor uses the **offside rule**. A block is
introduced by a header line and continued by the lines indented under it; a line
indented less than the block's column closes it. Blocks nest by indentation.

```ebnf
INDENT  ::= (* start of a more-indented region *)
DEDENT  ::= (* return to a less-indented region *)
NEWLINE ::= (* significant end-of-line within a layout region *)
```

**Implementation decision — hand-written Rust layout lexer.** These tokens are
produced by a **hand-written Rust layout lexer** in
[`crates/osprey-syntax/src/ml/lexer.rs`](https://github.com/Nimblesite/osprey/blob/main/crates/osprey-syntax/src/ml/lexer.rs)
(with `token.rs`, `parser.rs`, `mod.rs` alongside). The lexer derives the layout
markers (`Indent`/`Dedent`/`Newline`) from the **offside rule** (Landin 1966)
via an **explicit indentation stack**, with **bracket depth suppressing layout
inside parentheses**; it ignores blank lines and comment-only lines, and
preserves source positions (row/column) on every token so diagnostics and the
LSP keep working
([FLAVOR-LOWER-CONTRACT](/spec/0023-languageflavors/#the-lowering-contract)). This is
now wired end-to-end in the editor: the language server selects the ML frontend
for a `.ospml` document through `osprey_syntax::parse_program_for_path`, so a
layout-flavor file is analysed by its own parser instead of being flagged as
broken Default syntax — see
[FLAVOR-SELECT](/spec/0023-languageflavors/#flavor-selection). The
parser above it is a **recursive-descent (Pratt / precedence-climbing)** parser
that produces an ML **concrete syntax tree (CST)**; a separate lowerer
(`lower.rs`) then converts that CST to canonical `osprey_ast::Program`, keeping a
clean **CST→AST separation**.

This **supersedes** the earlier plan of a `tree-sitter-osprey-ml` grammar with
an external C scanner. Rationale: the offside rule is naturally expressed with
an explicit indent stack in safe Rust; the frontend stays panic-free /
`Result`-returning and unit-testable (project rules), with no `unsafe` C and no
codegen-tool build dependency. Per
[`[FLAVOR-BOUNDARY]`](/spec/0023-languageflavors/#the-one-law) the parser
**mechanism** is a below-the-AST, flavor-internal concern, so this swap does not
change the architecture (many CSTs, one AST). The parsing techniques are cited
in the [References](#references) section.

> **Escape hatch (documented fallback, not the primary path).** If the
> hand-written layout frontend becomes onerous or accrues parsing bugs we cannot
> tame, we fall back to a `tree-sitter-osprey-ml` grammar with an external
> `INDENT`/`DEDENT`/`NEWLINE` `scanner.c`. The boundary law
> ([`[FLAVOR-BOUNDARY]`](/spec/0023-languageflavors/#the-one-law)) makes the parser
> mechanism a flavor-internal swap that leaves the AST and everything above it
> untouched. (The tree-sitter brace grammar has none today —
> `tree-sitter-osprey/` ships no `scanner.c` — so the fallback scanner would be
> new work.)

String interpolation keeps `${…}`. Parentheses remain available for grouping and
precedence; they are not mandatory call punctuation.

## Comments

`[FLAVOR-ML-COMMENTS]` The ML flavor has two ordinary comment forms and one
documentation form:

- **`// …`** — a line comment to end of line.
- **`(* … *)`** — a block comment in the ML-family (SML/OCaml/F#) convention.
  It **nests**, so a commented-out region containing another `(* *)` closes
  correctly; an unterminated block comment is a lexical error. Layout ignores
  comment content entirely (it is trivia the layout lexer skips).
- **`(** … *)`** — a **documentation comment** (the odoc double-star
  convention), specified in
  [Documentation Comments](/spec/0026-documentationcomments/) `[DOC-SIGIL-ML]`. It
  attaches to the following declaration and lowers to the same `DocComment` the
  Default flavor's `///` produces. An empty `(**)` or an all-star banner
  `(*****)` is an ordinary comment, not a doc.

The Default (brace) flavor's ordinary line comment is `//` and its doc comment
is `///` ([0026](/spec/0026-documentationcomments/) `[DOC-SIGIL-DEFAULT]`); the ML
block forms are the layout-flavor idiom for the same roles.

## Bindings and Mutation

`[FLAVOR-ML-BIND]` `=` **binds**, `:=` **mutates**. There is no `let`: a bare
`name = expr` introduces an immutable binding in the current layout block. `mut`
marks a mutable binding, and every write to it uses `:=`, so mutation is visible
without scanning back to the declaration.

```ebnf
binding   ::= "mut"? bindingHead "=" expr
bindingHead ::= ID paramPattern*          (* zero patterns ⇒ value; one+ ⇒ function *)
mutation  ::= ID ":=" expr
```

```osprey-ml
answer = 42
mut requests = 0
requests := requests + 1
```

Same-scope rebinding with `=` is rejected; the diagnostic suggests `:=` if
mutation was meant. Shadowing in a nested block or pattern is allowed.

Lowering: `name = e` → `Stmt::Let { mutable: false }`; `mut name = e` →
`Stmt::Let { mutable: true }`; `name := e` → `Stmt::Assignment`. These are the
*same* canonical nodes the Default flavor emits for `let`, `mut`, and `=`
reassignment respectively — only the spelling differs.

## Functions and Currying

`[FLAVOR-ML-FN]` A function definition is a binding whose head has one or more
parameter patterns. The optional signature line above it uses ML arrows.

```ebnf
signature ::= ID ":" type
funDef    ::= ID paramPattern+ "=" blockOrExpr                 (* curried: one arg per pattern *)
            | ID "(" param ("," param)* ")" "=" blockOrExpr    (* uncurried: one flat arg list *)
type      ::= type "->" type            (* right-associative: a -> b -> c = a -> (b -> c) *)
            | "(" type ("," type)* ")" "->" type   (* uncurried multi-argument *)
            | typeAtom
```

```osprey-ml
inc : int -> int
inc x = x + 1

add : int -> int -> int
add x y = x + y
```

`[FLAVOR-ML-CURRY]` ML **curries by default**. A multi-parameter binding
`add x y = body` reads as curried: it lowers to the **nested-lambda shape** — a
one-parameter `Stmt::Function` whose body is a one-parameter `Expr::Lambda` —
byte-identical to the Default flavor's *explicit-curry*
`fn add(x) = fn(y) => body`, **not** to the Default *multi-parameter*
`fn add(x, y)` (a deliberately different value, normative in
[FLAVOR-CURRY](/spec/0023-languageflavors/#currying-canonicalisation)). An ML program
and its Default explicit-curry twin emit byte-identical
IR ([FLAVOR-IR-EQUIV](/spec/0023-languageflavors/#cross-flavor-equivalence-tests)).

Application is curried and left-associative: `add 1 2` is `((add) 1) 2`, lowering
to nested single-argument calls `Call(Call(add, [1]), [2])`; a function-typed
signature's arrows are right-associative (`int -> int -> int` is
`int -> (int -> int)`), mirroring the application. **Partial application just
works**: `add 1` is the inner saturated call returning a function value — the
idiom ML reaches for (`rose = c256 "213"` makes a one-argument colouriser from
the two-argument `c256`).

**ML also has an uncurried, multi-argument form** — for a binding that should
*not* curry — written with parenthesised, comma-separated parameters:

```osprey-ml
add : (int, int) -> int
add (x, y) = x + y

sum = add (10, 20)
```

`add (x, y) = body` lowers to a **flat two-parameter `Stmt::Function`** — the
*same* canonical node as the Default *multi-parameter* `fn add(x, y) = body` —
and the saturated call `add (10, 20)` lowers to a single multi-argument
`Call(add, [10, 20])`, matching Default's `add(x: 10, y: 20)`. It does **not**
partially apply; the parenthesised comma-list is an argument grouping, not a
tuple value (Osprey has no tuple type). It is the deliberate not-equivalent of
the curried `add x y`.

ML therefore has **two** function forms, and they twin the two Default forms
exactly:

| ML form | lowers to | Default twin |
| --- | --- | --- |
| curried `add x y = e` | one-param `Function` → `Lambda` chain | explicit-curry `fn add(x) = fn(y) => e` |
| uncurried `add (x, y) = e` | flat two-param `Function` | multi-param `fn add(x, y) = e` |

This is what keeps cross-flavor IR **byte-identical**
([FLAVOR-IR-EQUIV](/spec/0023-languageflavors/#cross-flavor-equivalence-tests)) with
**no** backend currying magic: a twin's author picks the ML form matching its
Default original's currying — curried Default ↔ ML whitespace, uncurried Default
↔ ML parens — so both sides lower to the same AST and emit the same IR.

Lowering (normative in
[FLAVOR-CURRY](/spec/0023-languageflavors/#currying-canonicalisation)): curried
`add x y = body` → a one-parameter `Stmt::Function` returning a one-parameter
`Expr::Lambda`; `\x y => body` → the same curried `Expr::Lambda` chain; `add 1 2`
→ nested one-argument `Expr::Call`s — each byte-identical to Default
*explicit-curry* `fn add(x) = fn(y) => body` and `add(1)(2)`. Uncurried
`add (x, y) = body` → a flat multi-parameter `Stmt::Function`, and `add (1, 2)` →
a single `Call(add, [1, 2])` — byte-identical to Default `fn add(x, y)` and
`add(x: 1, y: 2)`. No flavor-only node shape survives lowering; ML reuses
Default's value vocabulary. (The backend *may* still fold a saturated curried
call into a direct multi-argument call as an independent optimisation, but the
lowered AST of `add x y` stays the curried nested form.)

API guidance: put stable, configuration-like arguments first and the data
argument last, so partial application is useful (`replace " " ""` ⇒ a
space-remover).

## Function Calls

`[FLAVOR-ML-CALL]` Calls use whitespace application; parentheses group.

```ebnf
application ::= app atom
             | atom
atom        ::= ID | literal | "(" expr ")"
```

```osprey-ml
length snap
textResp 201 "created\n"
c256 "213" (blocks 0 (mn n 28))
```

Lowering: whitespace application `f a b` → nested `Expr::Call`, one argument each
(`Call(Call(f,[a]),[b])`) — curried. A parenthesised comma-list `f (a, b)` is the
**uncurried** saturated call → a single `Call(f, [a, b])` (matching Default's
`f(x: a, y: b)`); a single parenthesised expression `f (a)` is just grouping and
lowers to `Call(f, [a])`.

## Modules and Namespaces

`[FLAVOR-ML-MODULES]` Module semantics are defined by
[Modules and Namespaces](/spec/0025-modulesandnamespaces/). This section defines
only the ML projection: layout supplies every body boundary, `::` qualifies
logical symbols, and visibility is written exactly once.

```ebnf
namespaceDecl ::= "namespace" namespaceName (INDENT item+ DEDENT)?
namespaceName ::= ID | STRING
moduleDecl    ::= "module" symbolPath (":" symbolPath)? INDENT item+ DEDENT
stateDecl     ::= "state" symbolPath (":" symbolPath)? INDENT item+ DEDENT
signatureDecl ::= "signature" ID INDENT signatureItem+ DEDENT
symbolPath    ::= ID ("::" ID)*
```

A namespace header without an indented body is file-scoped. An indented body is
one block contribution to the open namespace:

```osprey-ml
namespace billing

module Tax
    ...
```

```osprey-ml
namespace billing
    module Tax
        ...
```

A named signature is the whole public contract of an ascribed module. Signature
items are public by definition, and implementation declarations do not repeat
`export`:

```osprey-ml
signature TaxApi
    type Money
    type Rate = int
    addTax : Money -> Money

module Tax : TaxApi
    type Money = int
    type Rate = int

    addTax cents = cents
```

In a signature, bare `type T` is abstract; `type T = R` is manifest. Writing
`opaque type T` there is redundant and rejected. An ascribed module exports
exactly its signature, so any explicit `export` inside it is also rejected.

An un-ascribed module marks each public declaration group exactly once. The
inference-first form exports the definition directly; when a type contract is
genuinely load-bearing, an exported value signature transfers visibility to
the immediately following same-name bare definition:

```osprey-ml
module Tax
    defaultRate = 10

    export addTax cents =
        cents + cents * defaultRate / 100

    export zero cents = cents
    export opaque type UserId = int
```

Prefixing both a signature and its definition with `export` is an error, as is
an orphan signature. `export mut` is always an error: module-owned cells are
private.

The state-owning form is deliberately `state Name`, never the redundant
`state module Name`:

```osprey-ml
state Counter
    mut count = 0

    export effect CounterFx
        read : Unit => int

    export run action =
        handle CounterFx
            read => count
        in
            action ()
```

State never leaks through an ordinary accessor. `run` installs the capability;
the private cell is read only inside its handler arm. First-class exported
handler values will provide the still-cleaner factory form described in
[Handlers](#handlers) once that shared-core feature lands.

Imports use the same logical targets as the shared model, but explicit member
selection is a layout list rather than Default's brace list:

```ebnf
importDecl   ::= "import" importTarget ("as" ID)?
               | "import" importTarget INDENT importMember+ DEDENT
               | "import" importTarget INDENT "*" DEDENT
importMember ::= ID ("as" ID)?
```

```osprey-ml
import billing::Tax
import billing::Tax as T
import billing::Tax
    addTax
    zero as noTax
import "billing/api" as api

gross = T::addTax 100
```

`::` is namespace/module/member qualification; `.` remains value field access.
Qualified whitespace application is still curried (`Tax::add 1 2`). Calling a
flat multi-parameter API uses the explicit uncurried form
`Tax::add (1, 2)`, matching Default `Tax::add(1, 2)`.

## Effects

`[FLAVOR-ML-EFFECT]` An effect declaration is a layout block of operation
signatures. Operations use `=>` so that `->` keeps its one meaning — function
and currying type. An operation is a request with a **payload** and a **result**,
not a curried function.

```ebnf
effectDecl ::= "effect" ID typeParam* INDENT opSig+ DEDENT
opSig      ::= ID ":" type "=>" type
typeParam  ::= ("in" | "out")? ID
```

```osprey-ml
effect Db
    add : string => int
    list : Unit => string
    count : Unit => int

effect Log
    info : string => Unit
```

Zero-payload operations take `Unit`. Multi-field requests use a record payload,
not a fake multi-argument operation:

```osprey-ml
type AddTask =
    body : string
    priority : int

effect Db
    add : AddTask => int
```

Lowering: `effect E` + arms → `Stmt::Effect { operations }`, where each
`op : P => R` becomes `EffectOperation { name, parameters: [P], return_type: R }`
— the same canonical node the Default `op : fn(P) -> R` produces.
`perform E.op a` → `Expr::Perform`.

> `->` belongs to functions and currying. `=>` belongs to clauses and requests
> that yield a result: it appears in `effect` operations, `handler` arms, and
> `match` arms, always meaning "the left yields the right."

## Generics ([FLAVOR-ML-GENERICS])

`[FLAVOR-ML-GENERICS]` The ML flavor spells every generic binder by
juxtaposition on declarations and by an angle-bracket binder on signatures —
all lowering to the same variance-carrying `TypeParam` nodes the Default
flavor produces ([TYPE-GENERICS-DECL], [TYPE-VARIANCE-DECL] in
[Type System](/spec/0004-typesystem/#generics-and-variance)):

- **Type declarations** take whitespace parameters with optional variance
  markers: `type Box T =`, `type Feed out T =`, `type Gate in T =` — twinning
  Default `type Box<T>`, `type Feed<out T>`, `type Gate<in T>`.
- **Effect declarations** likewise: `effect Stash T` twins
  `effect Stash<T>` ([EFFECTS-GENERIC-DECL](/spec/0017-algebraiceffects/#generic-effects)).
- **Function type parameters** bind on the signature line:
  `pick<T> : (T, T) -> T` twins `fn pick<T>(first: T, second: T)`. A binding
  without a signature cannot declare type parameters. Variance markers are
  rejected on function binders.
- **Effect rows** apply type arguments with angle brackets:
  `bumped : Unit -> int ! Stash<int>` twins `fn bumped() -> int !Stash<int>`
  ([EFFECTS-GENERIC-ROWS](/spec/0017-algebraiceffects/#generic-effects)).
- **Construction sites** apply explicit type arguments on the inline record
  form: `Box<int>(item = 7)` twins `Box<int> { item: 7 }`
  ([TYPE-GENERICS-DECL](/spec/0004-typesystem/#generics-and-variance)). The
  layout (indented) record form takes no type arguments — use the inline
  form when the fields alone cannot pin the instantiation.

```osprey-ml
type Feed out T =
    Feed
        supply : T
    Dry

effect Stash T
    put : T => Unit
    take : Unit => T

pick<T> : (T, T) -> T
pick (first, second) = first
```

`out` stays an ordinary identifier outside type-parameter position; `in` (the
hard keyword of `handle … in`) is accepted contextually inside a parameter
list. A generic signature is distinguished from a `name < expr` comparison by
requiring the whole `name<params…> :` shape before committing.

## Handlers

`[FLAVOR-ML-HANDLER]` Handlers are **first-class values**. `handler E` followed
by indented arms evaluates to a value of type `Handler E`. `handle` installs one
or more such values around a computation, with `do` marking the handled body.

```ebnf
handlerValue ::= "handler" ID INDENT handlerArm+ DEDENT
handlerArm   ::= ID param* "=>" blockOrExpr
install      ::= "handle" expr+ "do" blockOrExpr
```

```osprey-ml
memoryDb : Unit -> Handler Db
memoryDb () =
    mut tasks = ""
    mut taskCount = 0

    handler Db
        add t =>
            taskCount := taskCount + 1
            tasks := "${tasks}#${toString taskCount} ${t}\n"
            taskCount

        list =>
            tasks

        count =>
            taskCount
```

Installing several at once replaces the Default flavor's repeated nesting:

```osprey-ml
db = memoryDb ()
log = silentLog ()

handle db log
do
    createTask "buy milk"
```

The mutable cells belong to the handler value: a fresh `handler` makes fresh
state; passing the same value around shares it. Parameterised handlers compose
with currying (`filePersist path = … handler Persist …`).

First-class handler values, the `Handler E` type, and multi-install are a
**shared-core feature**, not ML-only sugar — see
[FLAVOR-HANDLER-VALUE](/spec/0023-languageflavors/#shared-core-additions). They
lower to `Expr::HandlerValue` and `Expr::Install`; `handle a b c do body`
desugars to nested installs. The Default flavor gains the same feature in brace
spelling.

## Match

`[FLAVOR-ML-MATCH]` `match` uses the same clause style as handlers: the
scrutinee follows `match`, and each indented arm is `Pattern => body`. A
one-payload constructor binds its payload directly — `Success value`, not
`Success { value }`.

```ebnf
matchExpr ::= "match" expr INDENT matchArm+ DEDENT
matchArm  ::= pattern "=>" blockOrExpr
```

```osprey-ml
diskBytes =
    match saved
        Success value => length snap
        Error message => -1
```

Lowering: `Expr::Match` + `MatchArm`; `Success value` →
`Pattern::Constructor { name: "Success", fields: ["value"] }` — the same node the
Default `Success { value }` produces. Wildcard `_` → `Pattern::Wildcard`.

## Records

`[FLAVOR-ML-RECORD]` Record construction is a layout block headed by the
constructor name, with `field = value` lines. Inside a record literal the left
of `=` is a field name, not a new binding; the indentation under a constructor
makes that unambiguous.

```ebnf
recordExpr ::= ID INDENT fieldInit+ DEDENT
fieldInit  ::= ID "=" expr
```

```osprey-ml
textResp status bodyText =
    HttpResponse
        status = status
        headers = "Content-Type: text/plain"
        contentType = "text/plain"
        streamFd = -1
        isComplete = true
        partialBody = bodyText
```

Lowering: `Expr::TypeConstructor { name, fields }`; record update lowers to
`Expr::Update`.

## Blocks

`[FLAVOR-ML-BLOCK]` A function body, match arm, handler arm, or `do` body is an
ordinary layout region containing bindings, mutations, performs, and a final
expression. The final expression is the block's value. There is no separate
`{ … }` expression form in this flavor.

```osprey-ml
onPost body =
    id = perform Db.add body
    snap = perform Db.list
    written = perform Persist.flush snap
    perform Log.info "created"
    textResp 201 "created\n"
```

Lowering: `Expr::Block { statements, value }`, where `value` is the trailing
expression — the same node the Default `{ … }` block produces.

## Canonical Lowering Table

Every ML form on the left lowers to the canonical node on the right
(`crates/osprey-ast/src/lib.rs`). The Default-flavor spelling of the same node
is in [FLAVOR-LAYER](/spec/0023-languageflavors/#flavor-concern-vs-shared-core-concern).

| ML surface | Canonical AST node |
| --- | --- |
| `x = e` | `Stmt::Let { mutable: false }` |
| `mut x = e` | `Stmt::Let { mutable: true }` |
| `x := e` | `Stmt::Assignment` |
| `f x y = e` (curried) | one-param `Stmt::Function` returning a `Lambda` chain |
| `f (x, y) = e` (uncurried) | flat multi-param `Stmt::Function` |
| `\x y => e` | curried `Expr::Lambda` chain |
| `f a b` | nested one-arg `Expr::Call` — `Call(Call(f,[a]),[b])` |
| `f (a, b)` (saturated) | single multi-arg `Expr::Call` — `Call(f, [a, b])` |
| `namespace n` + following declarations | file-scoped `Stmt::Namespace` |
| `module M : S` + layout body | `Stmt::Module { kind: Plain, signature: S }` |
| `state M` + layout body | `Stmt::Module { kind: State }` |
| `signature S` + layout items | `Stmt::Signature` |
| layout member import | `Stmt::Import` + explicit `ImportSelection` |
| `A::B::value` | `Expr::Path(SymbolPath)` |
| `type T =` + variant/field layout | `Stmt::Type` + `TypeVariant` |
| `[a, b, c]` / `xs[i]` | `Expr::List` / `Expr::Index` |
| layout block | `Expr::Block` |
| `match v` + arms | `Expr::Match` + `MatchArm` |
| `Success value` | `Pattern::Constructor { fields: ["value"] }` |
| `T` + `f = v` lines | `Expr::TypeConstructor` |
| `effect E` + `op : P => R` | `Stmt::Effect` + `EffectOperation` |
| `perform E.op a` | `Expr::Perform` |
| `handler E` + arms | `Expr::HandlerValue` *(shared-core addition)* |
| `handle a b do body` | `Expr::Install` *(shared-core addition)* |

## Worked Example

The same program a Default-flavor author would write with braces, `fn`, named
arguments, and nested `handle … in`. It exercises curried definitions, partial
application (`textResp 201`, `c256 "213"`), `=>` effect operations, first-class
handler values with owned `mut` state, and one grouped `handle … do`.

```osprey-ml
effect Db
    add : string => int
    list : Unit => string
    count : Unit => int

effect Log
    info : string => Unit

c256 : string -> string -> string
c256 n s =
    "\e[38;5;${n}m${s}\e[0m"

rose : string -> string
rose = c256 "213"

textResp : int -> string -> HttpResponse
textResp status bodyText =
    HttpResponse
        status = status
        headers = "Content-Type: text/plain"
        contentType = "text/plain"
        streamFd = -1
        isComplete = true
        partialBody = bodyText

memoryDb : Unit -> Handler Db
memoryDb () =
    mut tasks = ""
    mut taskCount = 0

    handler Db
        add t =>
            taskCount := taskCount + 1
            tasks := "${tasks}#${toString taskCount} ${t}\n"
            taskCount

        list => tasks
        count => taskCount

silentLog : Unit -> Handler Log
silentLog () =
    handler Log
        info m => ()

createTask : string -> HttpResponse
createTask body =
    id = perform Db.add body
    snap = perform Db.list
    perform Log.info "created #${toString id} ${snap}"
    textResp 201 "created task #${toString id}\n"

db = memoryDb ()
log = silentLog ()

handle db log
do
    response = createTask "buy milk"
    print (httpResponseBody response)
```

The first-class handlers make test doubles trivial — a test installs spy or
stub handlers that close over the test's own `mut` cells around the call under
test, with no `Db`/`Log` parameters polluting the production signature:

```osprey-ml
test "createTask stores the task and logs" =
    mut stored = ""
    mut logLine = ""

    db =
        handler Db
            add task =>
                stored := task
                1
            list => "#1 ${stored}\n"
            count => 1

    log =
        handler Log
            info message => logLine := message

    response =
        handle db log
        do
            createTask "buy milk"

    expectEqual 201 (httpResponseStatus response)
    expectEqual "buy milk" stored
    expectEqual "created #1 #1 buy milk\n" logLine
```

## Resolved Syntax Questions

- **Zero-argument functions:** a parameterless `name = expr` is a value binding;
  a `name () = expr` is a `Unit -> T` function. Pure constants are values
  (`banner`); `()` is used where recursion or effects make the call boundary
  meaningful (`serveForever ()`).
- **Lambdas:** anonymous functions are written `\param* => body` (lowering to
  `Expr::Lambda`), keeping `=>` as the clause/yield arrow and `->` as the type
  arrow.
- **Effect annotations on signatures:** the effect row follows the result type,
  as in the Default flavor (`saveTask : string -> int ![Store, Log]`).

## References

These are the verified sources behind the hand-written ML frontend
([`[FLAVOR-ML-LAYOUT]`](#layout-model)): the recursive-descent / predictive
parser, its Pratt (precedence-climbing) expression layer, and the offside-rule
layout lexer.

### 1. Recursive-descent / predictive parsing foundations

- **Compilers: Principles, Techniques, and Tools** (the "Dragon Book"), 2nd ed. — Alfred V. Aho, Monica S. Lam, Ravi Sethi, Jeffrey D. Ullman. 2006. Pearson. ISBN 9780321486813. <https://www.pearson.com/en-us/subject-catalog/p/compilers-principles-techniques-and-tools/P200000003472/9780321486813> — Authorizes the canonical predictive recursive-descent / LL(1) construction (FIRST/FOLLOW-driven procedure-per-nonterminal parsing) the hand-written parser implements.
- **Compiler Construction** — Niklaus Wirth. 1996 (Addison-Wesley; author's free PDF). ETH Zürich. Landing page: <https://people.inf.ethz.ch/wirth/CompilerConstruction/index.html> · PDF: <https://people.inf.ethz.ch/wirth/CompilerConstruction/CompilerConstruction1.pdf> — Authorizes the single-symbol-lookahead, single-pass recursive-descent strategy of deriving one recursive procedure per grammar production directly from an EBNF grammar.
- **Crafting Interpreters** (ch. 6 "Parsing Expressions", ch. 17 "Compiling Expressions") — Robert Nystrom. 2021. Genever Benning (freely readable online). <https://craftinginterpreters.com/compiling-expressions.html> — Practitioner reference authorizing the by-hand, no-generator recursive-descent parser and its Pratt-based expression layer for a real language.

### 2. Operator-precedence / Pratt parsing

- **Top Down Operator Precedence** — Vaughan R. Pratt. 1973. Proc. 1st ACM SIGACT-SIGPLAN Symposium on Principles of Programming Languages (POPL '73), pp. 41–51. DOI: <https://doi.org/10.1145/512927.512931> — The primary source authorizing the Pratt (top-down operator-precedence) expression parser: per-token prefix/infix handlers driven by binding powers.
- **Parsing Expressions by Precedence Climbing** — Eli Bendersky. 2 Aug 2012. <https://eli.thegreenplace.net/2012/08/02/parsing-expressions-by-precedence-climbing> — Authorizes the precedence-climbing formulation of operator-precedence parsing (the loop-based, min-precedence variant used in production front-ends such as Clang).
- **Parsing Expressions by Recursive Descent: From Precedence Climbing to Pratt Parsing** — Theodore S. Norvell, Memorial University of Newfoundland. <https://www.engr.mun.ca/~theo/Misc/pratt_parsing.htm> — Authorizes treating precedence climbing and Pratt parsing as the same algorithm, justifying a single binding-power table for prefix/infix/postfix/ternary operators.

### 3. The offside rule / layout-sensitive (indentation) syntax

- **The Next 700 Programming Languages** — Peter J. Landin. 1966. Communications of the ACM 9(3), pp. 157–166. DOI: <https://doi.org/10.1145/365230.365257> — The origin of the "offside rule"; the primary source authorizing indentation-as-structure (a token left of the line's first significant token starts a new construct).
- **Principled Parsing for Indentation-Sensitive Languages: Revisiting Landin's Offside Rule** — Michael D. Adams. 2013. Proc. 40th ACM SIGPLAN-SIGACT Symposium on Principles of Programming Languages (POPL '13), pp. 511–522. DOI: <https://doi.org/10.1145/2429069.2429129> · author PDF: <https://michaeldadams.org/papers/layout_parsing/LayoutParsing.pdf> — Authorizes a grammar-integrated, principled treatment of indentation sensitivity rather than an ad-hoc lexer hack.
- **Haskell 2010 Language Report** — §2.7 "Layout" (informal) and §10.3 (formal layout algorithm) — Simon Marlow (ed.). 2010. haskell.org. Lexical chapter: <https://www.haskell.org/onlinereport/haskell2010/haskellch2.html> · Syntax-reference chapter: <https://www.haskell.org/onlinereport/haskell2010/haskellch10.html> — Secondary/reference source authorizing a concrete, fully specified offside layout algorithm (brace/semicolon insertion from indentation) suitable for a hand-written lexer/parser.

### 4. Error recovery in recursive-descent (panic-mode / synchronization)

- **Compilers: Principles, Techniques, and Tools** (the "Dragon Book"), 2nd ed., §4.1.3–4.1.4 (Error-Recovery Strategies; panic-mode and phrase-level recovery) — Aho, Lam, Sethi, Ullman. 2006. Pearson. ISBN 9780321486813. <https://www.pearson.com/en-us/subject-catalog/p/compilers-principles-techniques-and-tools/P200000003472/9780321486813> — The foundational reference authorizing panic-mode error recovery: on a syntax error, discard input tokens until a synchronizing token (e.g. statement terminators / FOLLOW sets) is reached, then resume.

> Verification (research subagent, 2026): the three DOIs (Pratt 10.1145/512927.512931, Landin 10.1145/365230.365257, Adams 10.1145/2429069.2429129) resolve through doi.org to the correct ACM DL records (ACM landing pages 403 to automated fetches; corroborated via doi.org redirect + dblp). Wirth ETH page + PDF, Adams author PDF, Nystrom, Bendersky, Norvell, and both Haskell 2010 chapters were each fetched and matched. Dragon Book §4.1.3–4.1.4 are the standard 2nd-ed. TOC section numbers (book/publisher confirmed; exact section numbers not page-verified).

## Cross-references

- [Language Flavors](/spec/0023-languageflavors/) — the normative boundary,
  contract, currying canonicalisation, and shared-core handler-value feature.
- [Algebraic Effects](/spec/0017-algebraiceffects/) — effect semantics shared by both
  flavors.
- [Modules and Namespaces](/spec/0025-modulesandnamespaces/) — shared namespace,
  module, signature, import, export, and state-ownership semantics.
- [Plan 0013 — ML Flavor Frontend](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0013-ml-flavor-frontend.md).