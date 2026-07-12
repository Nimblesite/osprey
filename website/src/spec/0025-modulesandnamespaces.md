---
layout: page
title: "Modules and Namespaces"
description: "Osprey Language Specification: Modules and Namespaces"
date: 2026-07-12
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0025-modulesandnamespaces/"
---

# Modules and Namespaces

Osprey multi-file programs are built from **logical namespaces** and
**explicit modules**, not from file paths. A source file's path decides whether
it belongs to a project; it does **not** decide the names it exports.

> **Flavor layer - shared core (AST and above).** Namespace/import resolution,
> module signatures, exports, state ownership, separate compilation, and project
> assembly are shared-core semantics. The Default flavor and ML flavor may spell
> declarations differently, but both lower to the same canonical project model:
> `NamespaceDecl`, `ModuleDecl`, `SignatureDecl`, `Import`, and symbol paths.
> No type checker, effect checker, code generator, runtime, or LSP feature may
> infer semantics from `.osp` vs `.ospml` once lowering has happened. See
> [Language Flavors](/spec/0023-languageflavors/).

## Status `[MODULES-STATUS]`

The initial module system is implemented end to end in both flavors. The shared
AST, project loader, namespace graph, import/privacy/signature checks, resolved
linkage names, state-owner validation, handler-local state materialization,
type checking, native/LLVM codegen, project CLI, formatter, and same-document
editor features are active. Mixed `.osp`/`.ospml` projects build and run through
the same canonical graph.

The implementation fails loudly at the remaining semantic boundaries. In
particular, opaque manifest aliases are rejected rather than flattened to their
private representation; owner-transparent/client-nominal alias checking needs a
non-flat interface-aware checker. Separate importer checking, parameterised and
recursive modules, ranked import suggestions, incremental cross-file LSP
indexing, module reference-page generation, and source-name restoration in
native debugger/stack frames remain planned and are unchecked in
[plan 0014](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0014-modules-and-namespaces.md). This chapter remains the
normative contract for those pieces and supersedes the fiber-isolated module
sketch in [Fibers and Concurrency](/spec/0011-lightweightfibersandconcurrency/#fiber-isolated-modules-planned).

## Research Basis `[MODULES-RESEARCH]`

The design combines .NET-style logical named groups with
ML-style abstraction boundaries and Osprey's algebraic effects. It deliberately
does **not** adopt the usual .NET `Company.Product.Feature` hierarchy as an
Osprey norm.

- Parnas set the bar for modularity: "The effectiveness of a \"modularization\" is
  dependent upon the criteria used" ([Parnas 1972](https://wstomv.win.tue.nl/edu/2ip30/references/criteria_for_modularization.pdf)).
- The .NET precedent Osprey keeps is the named logical group: "A namespace
  declaration assigns your types to a named group" ([Microsoft namespace guide](https://learn.microsoft.com/en-us/dotnet/csharp/fundamentals/program-structure/namespaces)).
- The .NET Framework Design Guidelines document the familiar hierarchy template
  `<Company>.(<Product>|<Technology>)[.<Feature>]`; Osprey records that as
  precedent, not a recommendation for app code ([Microsoft namespace guidelines](https://learn.microsoft.com/en-us/dotnet/standard/design-guidelines/names-of-namespaces)).
- F# separates namespaces from modules: a namespace attaches a name to related
  program elements, while a module groups F# constructs such as types, values,
  and functions ([F# namespaces](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/namespaces), [F# modules](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/modules)).
- Slash-style module names have precedent: Racket says a string module path uses
  Unix-style `/` as the separator ([Racket module paths](https://docs.racket-lang.org/guide/module-paths.html)), and Go import paths are string literals such as `"lib/math"` ([Go spec](https://go.dev/ref/spec#Import_declarations)).
- Rust gives the item-qualification precedent Osprey follows: "A path is a
  sequence of one or more path segments separated by `::` tokens" ([Rust Reference](https://doc.rust-lang.org/reference/paths.html)).
- OCaml's module system makes signatures the abstraction boundary: "A signature
  specifies which components of a structure are accessible" ([OCaml manual](https://ocaml.org/manual/5.0/moduleexamples.html)).
- Haskell modules are explicit about import/export control; the Report defines
  modules with import declarations and optional export lists ([Haskell 2010 Report](https://www.haskell.org/onlinereport/haskell2010/haskellch5.html)).
- Elm keeps module exposure visible at the top of the file through `exposing`
  lists ([Elm modules guide](https://guide.elm-lang.org/webapps/modules)).
- Clojure's namespace guide makes aliasing first-class because long names are
  rarely what readers want at every call site ([Clojure namespaces](https://clojure.org/guides/learn/namespaces)).
- Java's reverse-domain convention is about globally unique published packages;
  the JLS says it piggybacks on an existing unique-name registry, not source
  location ([JLS unique package names](https://docs.oracle.com/javase/specs/jls/se7/html/jls-6.html#jls-6.1)).
- Harper and Lillibridge identify the core problem as "the management of the
  flow of information between program units" ([POPL 1994](https://www.cs.cmu.edu/~rwh/papers/sharing/popl94.pdf)).
- Rossberg, Russo, and Dreyer summarize the ML lesson: "ML modules are a
  powerful language mechanism for decomposing programs" ([F-ing Modules](https://people.mpi-sws.org/~dreyer/courses/modules/f-ing.pdf)).
- Leroy's manifest-types work requires a "strict distinction between abstract
  types and manifest types" ([POPL 1994](https://caml.inria.fr/pub/papers/xleroy-manifest_types-popl94.pdf)).
- Backpack states the separate-compilation target: "explicit interfaces express
  assumptions about dependencies" ([Kilpatrick, Dreyer, Peyton Jones, Marlow 2014](https://plv.mpi-sws.org/backpack/)).
- Launchbury and Peyton Jones justify encapsulated mutable state: "Some
  algorithms make critical internal use of updatable state" ([Lazy Functional State Threads](https://www.microsoft.com/en-us/research/publication/lazy-functional-state-threads/)).
- Plotkin and Pretnar make state a handled effect: effects include "state, time,
  and their combinations" ([Handlers of Algebraic Effects](https://homepages.inf.ed.ac.uk/gdp/publications/Effect_Handlers.pdf)).
- Moseley and Marks give the architectural rule: "Separate" essential state from
  essential logic and accidental state/control ([Out of the Tar Pit](https://curtclifton.net/papers/MoseleyMarks06a.pdf)).
- Linear Haskell points at the resource-state horizon: "typestates ... are
  actually enforced by the type system" ([Bernardy et al. 2018](https://arxiv.org/pdf/1710.09756)).
- Modern lexical effect handlers aim at "local-reasoning principles"
  ([Ma, Ge, Lee, Zhang 2024](https://cs.uwaterloo.ca/~yizhou/papers/lexa-oopsla2024.pdf)).
- Redux captures the state-management operational rule: "single source of truth"
  ([Redux Three Principles](https://redux.js.org/understanding/thinking-in-redux/three-principles)).

These are not ornamental citations. They drive the rules below: names are
logical, interfaces are explicit, abstract state does not leak, and mutable state
has one owner.

## Comparative Practice `[MODULES-COMPARATIVE-PRACTICE]`

The survey above yields concrete rules:

- **Use namespaces for logical grouping, not architecture.** .NET/F# names are a
  useful precedent for path-independent grouping, but Osprey does not copy the
  deep enterprise naming convention as the default shape.
- **Use modules for boundaries.** OCaml/F#/ML practice puts abstraction,
  signatures, and implementation hiding at the module boundary; Osprey follows
  that instead of making namespaces carry privacy or state.
- **Make import surface area visible.** Haskell, Elm, and Clojure all make
  import/export choices visible in source. Osprey therefore supports explicit
  member imports and aliases, and treats wildcard imports as a script/test
  convenience.
- **Separate module paths from member access.** Rust's `::` keeps item paths
  visually distinct from record field access; Osprey uses `::` for namespace,
  module, and exported-member paths, leaving `.` for value/member operations.
- **Allow slash names only as labels.** Racket and Go show precedent for
  slash-like module/import paths, but in Osprey a quoted slash namespace is one
  opaque label. It does not imply folder mirroring, parent namespaces, or load
  order.
- **Reserve reverse-DNS/deep names for distribution.** Java's reverse-domain
  convention solves global package collision, not local application design.
  Osprey may use similar labels for published libraries later, but app code
  should usually stay flat.

## Design Goals `[MODULES-GOALS]`

The module system must make the good structure the easy
structure:

- **Path-independent names.** A namespace label comes from source text, not from
  `src/foo/bar.osp`.
- **Flat-first namespaces.** A good namespace is usually one short project or
  domain label, not a forced company/product/feature tower.
- **Separators are spelling, not architecture.** A quoted namespace may contain
  `/` when a project wants folder-like names, but `/` does not create parent
  namespaces, inheritance, visibility, or initialization order.
- **Open namespaces, closed modules.** Namespaces organize; modules encapsulate.
- **Explicit imports and exports.** Wildcard visibility is the escape hatch, not
  the default.
- **Separate compilation by interface.** A file can be checked against imported
  signatures without loading every implementation detail.
- **State has a declared owner.** Namespace and plain-module mutable state is
  forbidden. A state module may declare private cells, but only its algebraic-
  effect handler arms may read or write them.
- **Pure logic stays pure.** State crosses a module boundary only through effect
  operations. Ordinary functions may transform explicit values, but may not
  inspect or mutate module-owned cells.
- **Cross-flavor interop.** A `.osp` module and `.ospml` module import each other
  through canonical signatures.

## Canonical Project Model `[MODULES-MODEL]`

The module system is a project graph. Concrete syntax is only
how each flavor contributes nodes and edges to that graph.

The shared model contains:

- `SourceFile { path, flavor, contributions }` - a parsed file with one active
  flavor and one or more namespace contributions. Unscoped declarations
  contribute to the project's default namespace.
- `NamespaceContribution { namespace, items, span }` - one file-scoped,
  block-scoped, or project-defaulted contribution.
- `Namespace { label, contributions }` - an open logical grouping of
  declarations from any number of files.
- `Module { namespace, path, kind, exports, private_items, signature }` - a
  closed boundary inside a namespace. `kind` is `plain` or `state`.
- `Signature { name, items }` - an interface contract for a module.
- `ImportEdge { from_file, target, alias, imported_members }` - a dependency on
  a namespace/module/member surface, never on a physical file.
- `SymbolId { namespace, path }` - the stable identity for exported declarations.
- `StateOwner { module, cells, installers }` - private cell declarations plus
  the handler installers allowed to instantiate and access them.

Every later phase consumes this model, not surface syntax:

```text
source files (.osp/.ospml)
  -> flavor parsers
  -> canonical project graph
  -> import resolution
  -> signature and privacy checking
  -> type/effect checking
  -> codegen/runtime/LSP/docs
```

No semantic rule below depends on braces, layout, `fn`, whitespace application,
or named arguments. Those are flavor concerns described in
[Syntax](/spec/0003-syntax/), [Language Flavors](/spec/0023-languageflavors/), and
[ML Flavor Syntax](/spec/0024-mlflavorsyntax/).

## Surface Projection `[MODULES-FLAVOR-PROJECTION]`

Each flavor projects the same model into its own
surface. The examples in this chapter are illustrative; the model above is the
normative layer.

| Concept | Shared model | Default flavor | ML flavor |
| --- | --- | --- | --- |
| Namespace contribution | `Namespace { label }` | `namespace billing { ... }` or `namespace billing;` | file-scoped `namespace billing`, or an indented block |
| Module boundary | `Module { path, exports, private_items }` | `module Tax { ... }` | `module Tax` + indented body |
| State module | `Module { kind: state }` | `state module Store { ... }` | `state Store` + indented body |
| Import edge | `ImportEdge` | `import billing::Tax::{addTax}` | `import billing::Tax` + indented member names |
| Signature | `Signature { items }` | `signature StoreSig { ... }` | `signature StoreSig` + indented items |
| Export | exported item metadata | explicit `export fn f(...)` when unascribed | one `export` on a signature or inferred binding when unascribed; an ascribed module uses its signature |
| Symbol path | `SymbolId { namespace, path }` | `billing::Tax::addTax` | same path; application remains whitespace |

Default deliberately keeps braces, parentheses, explicit `fn`, named arguments,
and visible export markers: a C, C#, Java, Kotlin, or Dart programmer can read
the boundary without learning layout rules. ML deliberately removes structural
punctuation that layout and signatures already express. These are surface
choices only; both lower to the same graph.

## Namespaces `[MODULES-NAMESPACE]`

A `namespace` declaration contributes declarations to an
open logical namespace. Multiple files may contribute to the same namespace.
Namespace labels are opaque. `billing`, `"billing/api"`, and `"ui/forms"` are
three unrelated labels; no parent namespace is implied.

```ebnf
defaultNamespaceDecl ::= "namespace" namespaceName ("{" statement* "}" | ";")
mlNamespaceDecl ::= "namespace" namespaceName
                    (NEWLINE INDENT statement+ DEDENT | NEWLINE)
namespaceName ::= IDENT | STRING
symbolPath ::= IDENT ("::" IDENT)*
```

Default flavor:

```osprey
namespace billing {
    type Money = { cents: int, currency: string }
}

namespace billing {
    fn zero(currency: string) -> Money = Money { cents: 0, currency: currency }
}
```

ML flavor:

```osprey-ml
namespace billing

type Money =
    Money
        cents : int
        currency : string

zero : string -> Money
zero currency =
    Money
        cents = 0
        currency = currency
```

The two declarations above define one namespace, `billing`. The compiler
merges namespace bodies before semantic analysis. Duplicate exported names in the
same namespace are compile-time errors unless they are overloads explicitly
allowed by a later overload spec.

Quoted labels allow slash-style names without overloading `/` inside ordinary
expressions:

```osprey
namespace "billing/api";
```

The slash is part of the label. It does not create a `billing` parent namespace.

### File-scoped Namespaces `[MODULES-FILE-SCOPED-NAMESPACE]`

A file-scoped namespace declaration applies to
all declarations after it in the file:

Default flavor:

```osprey
namespace billing;

type Invoice = { id: string, total: int }
fn emptyInvoice(id: string) = Invoice { id: id, total: 0 }
```

ML flavor:

```osprey-ml
namespace billing

type Invoice =
    Invoice
        id : string
        total : int

emptyInvoice : string -> Invoice
emptyInvoice id =
    Invoice
        id = id
        total = 0
```

A file may contain either one file-scoped namespace declaration or any number of
block-scoped namespace declarations, not both. In ML, an indented body is a
block-scoped contribution; declarations at the namespace line's indentation make
the declaration file-scoped. Layout makes the distinction without braces or a
terminator.

### Path Independence `[MODULES-PATH-INDEPENDENCE]`

The physical file path is never part of the
namespace identity. A file `src/weird/place/x.osp` may declare `namespace billing;`
or `namespace "billing/api";`. The compiler may warn when path and namespace
drift from project convention, but it must not change symbol identity or import
resolution.

### Namespace Style `[MODULES-NAMESPACE-STYLE]`

Namespace style is flexible but flat-first:

- Prefer one short lowercase label for app namespaces: `app`, `billing`, `ui`,
  `worker`.
- Use quoted slash labels only when the slash is part of a meaningful external
  name, published package path, generated binding path, or project convention:
  `"billing/api"`, `"vendor/sqlite"`.
- Avoid reverse-domain and three-part product hierarchies in ordinary app code.
  They are accepted for interoperability and distribution, but examples and docs
  must not present them as the normal shape.
- Never mirror folders by default. If a team chooses folder-like slash labels,
  the label remains opaque and path-independent.

## Modules `[MODULES-MODULE]`

A `module` is a closed, stateless implementation boundary inside a namespace.
It may contain immutable values, functions, types, effects, and nested plain
modules. A direct `mut` declaration is forbidden; ordinary local `mut` bindings
inside its functions and blocks retain their normal lexical semantics. An
unascribed module exports only declarations marked `export`; an ascribed module's
signature is its complete public surface.

```ebnf
defaultModuleDecl ::= "module" symbolPath defaultSignatureAscription?
                      "{" defaultModuleItem* "}"
defaultStateDecl ::= "state" "module" symbolPath defaultSignatureAscription?
                     "{" defaultModuleItem* "}"
defaultModuleItem ::= "export"? statement
mlModuleDecl ::= "module" symbolPath mlSignatureAscription?
                 NEWLINE INDENT mlModuleItem+ DEDENT
mlStateDecl ::= "state" symbolPath mlSignatureAscription?
                NEWLINE INDENT mlModuleItem+ DEDENT
mlModuleItem ::= "export"? statement
defaultSignatureAscription ::= ":" symbolPath ("+" "extra")?
mlSignatureAscription ::= ":" symbolPath
```

Default flavor:

```osprey
namespace billing;

module Tax {
    let defaultRate = 10

    export fn addTax(cents) =
        cents + cents * defaultRate / 100
}
```

ML flavor:

```osprey-ml
namespace billing

module Tax
    defaultRate = 10

    export addTax cents =
        cents + cents * defaultRate / 100
```

`Tax::defaultRate` is private. `Tax::addTax` is exported. The `::` separator is
reserved for namespace/module paths; `.` remains value field/member access.

### Namespaces Versus Modules `[MODULES-NAMESPACE-VS-MODULE]`

Namespaces are open and stateless. Plain modules are closed and stateless. A
namespace cannot be used as a runtime value; a module is a named declaration
space. Only a state module declares an owner template for durable state, and that
state exists only inside an explicitly installed handler region.

## Imports `[MODULES-IMPORT]`

Imports name namespaces or modules, not files. Default uses a compact braced
member list; ML uses an indented list with no braces or commas.

```ebnf
defaultImportStmt ::= "import" importTarget defaultImportTail?
defaultImportTail ::= "as" IDENT
                    | "::" "{" importMember ("," importMember)* "}"
                    | "::" "*"
mlImportStmt ::= "import" importTarget
                 ("as" IDENT | "::" "*"
                 | NEWLINE INDENT importMember (NEWLINE importMember)* DEDENT)?
importTarget ::= namespaceName ("::" symbolPath)?
importMember ::= IDENT ("as" IDENT)?
```

Default flavor:

```osprey
import billing::Tax
import billing::Tax::{addTax}
import billing::Tax as Tax
import "billing/api" as billingApi

let gross = addTax(100)
let other = Tax::addTax(100)
```

ML flavor:

```osprey-ml
import billing::Tax
import billing::Tax
    addTax
import billing::Tax as Tax
import "billing/api" as billingApi

gross = addTax 100
other = Tax::addTax 100
```

### Name Resolution `[MODULES-RESOLUTION]`

Resolution is lexical and deterministic: local bindings and declarations win,
then the current module and its parents, imported aliases, imported members,
explicit namespace-qualified paths, and finally built-ins. An import never
silently replaces a nearer declaration.

Additional import rules:

- Identifier namespace labels can be used directly with `::`:
  `billing::Tax::addTax(100)`.
- Quoted namespace labels must be imported with an alias before member access:
  `import "billing/api" as billingApi`, then `billingApi::Tax::addTax(100)`.
- `import billing::Tax` brings the exported module `Tax` into the local scope as
  `Tax`.
- Default `import billing::Tax::{x, y}` and the equivalent ML indented member
  list bring only the listed exports into local scope.
- `import billing::Tax as Alias` brings `Alias` into local scope.
- `import billing::Tax::*` is allowed only in examples, scripts, and tests unless the
  project enables `allow_wildcard_imports = true`; it is forbidden for state
  modules.
- Ambiguous unqualified names are compile-time errors. The diagnostic must show
  every imported candidate and suggest qualification or aliasing.

Imports do not execute code, allocate module state, or load files by relative
path.

## Exports And Visibility `[MODULES-EXPORTS]`

Declarations are private by default inside modules and
public by default inside namespaces. A module controls its public surface through
`export` or a signature. In an ascribed module the signature already names every
export; ML therefore rejects redundant `export` markers there. Default may
repeat them as an explicit readability aid, but they cannot widen the signature.

Default flavor:

```osprey
module Parser {
    type Token = { text: string }       // private
    export type Ast = Expr | Stmt
    export fn parse(source: string) -> Result<Ast, Error> = ...
}
```

ML flavor:

```osprey-ml
module Parser
    type Token =
        Token
            text : string

    export type Ast =
        Expr | Stmt

    export parse : string -> Result<Ast, Error>
    parse source =
        ...
```

### Opaque Types `[MODULES-OPAQUE-TYPES]`

A module may export an opaque type, hiding its
representation:

Default flavor:

```osprey
module UserIds {
    export opaque type UserId = int

    export fn parseUserId(raw: string) -> Result<UserId, Error> = ...
    export fn showUserId(id: UserId) -> string = ...
}
```

ML flavor:

```osprey-ml
module UserIds
    export opaque type UserId = int

    export parseUserId : string -> Result<UserId, Error>
    parseUserId raw =
        ...

    export showUserId : UserId -> string
    showUserId id =
        ...
```

Outside `UserIds`, `UserId` is distinct from `int`. Inside `UserIds`, the
manifest representation is available. This is the Osprey form of ML abstract
types and Leroy-style manifest types.

## Signatures `[MODULES-SIGNATURE]`

A `signature` is an explicit interface for a module. It
lists the names, types, effects, and opacity visible to clients.

```ebnf
defaultSignatureDecl ::= "signature" IDENT "{" signatureItem* "}"
mlSignatureDecl ::= "signature" IDENT NEWLINE
                    INDENT signatureItem+ DEDENT
signatureItem ::= abstractTypeSpec | manifestTypeSpec | valueSpec
                | effectSpec | moduleSpec
abstractTypeSpec ::= "opaque" "type" IDENT       /* Default */
                   | "type" IDENT                 /* ML */
manifestTypeSpec ::= "type" IDENT "=" typeExpr
valueSpec ::= "fn" IDENT typeParams? "(" signatureParams? ")"
              ("->" typeExpr)? effectRow?         /* Default */
            | IDENT ":" mlType effectRow?         /* ML */
effectSpec ::= "effect" IDENT effectSignatureBody
moduleSpec ::= "module" IDENT ":" symbolPath
signatureParams ::= signatureParam ("," signatureParam)*
signatureParam ::= IDENT ":" typeExpr
effectSignatureBody ::= "{" effectOperationSpec* "}"
                      | NEWLINE INDENT effectOperationSpec+ DEDENT
effectOperationSpec ::= IDENT ":" typeExpr         /* Default fn type */
                      | IDENT ":" mlType "=>" mlType
defaultSignatureAscription ::= ":" symbolPath ("+" "extra")?
mlSignatureAscription ::= ":" symbolPath
```

Default flavor:

```osprey
signature StoreSig {
    opaque type Store
    effect StoreFx {
        load : fn() -> Store
        save : fn(Store) -> Unit
    }
    fn empty() -> Store
}

module MemoryStore : StoreSig {
    export type Store = { values: [string] }
    export effect StoreFx {
        load : fn() -> Store
        save : fn(Store) -> Unit
    }
    export fn empty() = Store { values: [] }
}
```

ML flavor:

```osprey-ml
signature StoreSig
    type Store
    effect StoreFx
        load : Unit => Store
        save : Store => Unit
    empty : Unit -> Store

module MemoryStore : StoreSig
    type Store =
        Store
            values : [string]

    effect StoreFx
        load : Unit => Store
        save : Store => Unit

    empty () =
        Store
            values = []
```

In an ML signature, bare `type Store` is abstract; repeating `opaque` would add
no information. The implementation supplies its representation, but clients see
only the abstract identity. The ascription already supplies the export list, so
the ML implementation repeats neither `export` nor signature punctuation.

Signature conformance is checked structurally:

- Every signature item must have a matching implementation declaration; the
  ascription exports it.
- Types must match after applying opacity rules.
- Effect operations must match names, parameter types, return types, and effect
  rows.
- Extra private declarations are allowed.
- Other implementation declarations stay private. In Default,
  `: StoreSig + extra` opts into additionally exporting declarations explicitly
  marked `export`; without `+ extra`, such extra exports are rejected. ML keeps
  ascription exact: extend the named signature or use an unascribed module
  instead of adding a redundant `+ extra`/`export` side channel.

### Separate Checking `[MODULES-SEPARATE-CHECKING]`

A compiler may type-check an importing file using
only the imported module's signature. The implementation body is needed only
when compiling that module or linking the final project.

## Parameterised Modules `[MODULES-FUNCTOR]`

A parameterised module is a module-level function from
signatures to modules. This is planned after basic signatures.

```osprey
module MakeRepo(Db: DatabaseSig, Clock: ClockSig) : RepoSig {
    export fn save(item: Item) -> Unit !Db.Database =
        Db.insert(table: "items", value: encode(item, Clock.now()))
}
```

Parameterised modules are the dependency-injection mechanism for reusable
libraries. They are preferred over ambient globals.

## State Ownership `[MODULES-STATE]`

Local mutation is unchanged: `mut` inside an ordinary function or block is a
lexical local and may be read or written by that scope. Durable module-owned
state is different. It may be declared only in a state module and may be
observed or changed only by that module's owning algebraic-effect handler arms
([EFFECTS-HANDLER-STATE](/spec/0017-algebraiceffects/#handler-owned-state)).

### Forbidden Top-level State `[MODULES-STATE-TOPLEVEL]`

A direct `mut` in a namespace or plain module is a compile-time error:

```osprey
namespace badState;

mut count = 0
// error: mutable state must be local or owned by a state-module handler
```

### State Modules `[MODULES-STATE-MODULE]`

A state module declares private cell templates, exported effects, and one or
more handler installer functions. A cell declaration establishes its initial
value; every subsequent read or write of that cell must occur lexically inside
an operation arm of a `handle Effect ... in body` expression defined by the same
state module. The installer function itself may install that handler and invoke
the supplied body, but its ordinary code before and after the arms cannot touch
the cells.

Default flavor keeps the complete boundary explicit:

```osprey
signature CounterSig {
    effect CounterFx {
        next : fn() -> int
        read : fn() -> int
    }
    fn run(action: fn() -> Unit) -> Unit
}

state module Counter : CounterSig {
    mut count = 0

    export effect CounterFx {
        next : fn() -> int
        read : fn() -> int
    }

    export fn run(action: fn() -> Unit) -> Unit =
        handle CounterFx
            next => {
                count = count + 1
                count
            }
            read => count
        in action()
}
```

The ML projection removes the redundant module keyword, braces, and exports
already supplied by the signature:

```osprey-ml
signature CounterSig
    effect CounterFx
        next : Unit => int
        read : Unit => int
    run : (Unit -> Unit) -> Unit

state Counter : CounterSig
    mut count = 0

    effect CounterFx
        next : Unit => int
        read : Unit => int

    run action =
        handle CounterFx
            next () =>
                count := count + 1
                count
            read () => count
        in action ()
```

Clients import declarations, then explicitly invoke the installer around code
that performs the effect:

```osprey
import app::Counter::{CounterFx, run}

fn allocate() -> int !CounterFx = perform CounterFx.next()

run(action: fn() => {
    print(toString(allocate()))
})
```

Each call to `run` instantiates a fresh `count` cell for that handler region.
Merely importing `Counter`, `CounterFx`, or `run` allocates nothing. First-class
handler values and reusable `install` syntax are a later ergonomic layer (plan
0016); the initial module implementation uses the existing fused
`handle Effect ... in body` form.

Rules:

- State-module cells are private by construction. `export mut` is always an
  error, as is exporting a pointer, reference, or closure that exposes a cell.
- An ordinary function, exported or private, cannot read or write a module cell.
  Pure helpers remain legal when all state arrives through explicit parameters.
- A state module must export its effect surface and at least one installer that
  establishes the owning handler region.
- A namespace may contain at most one state module in the initial module
  system. There is no annotation, ordinary-function, or ambient-state escape
  hatch; additional owners are a compile-time error.
- Cached derived cells are forbidden in Phase 1. Derived values are computed in
  handler arms or by pure helpers over explicit values.

### Single Source of Truth `[MODULES-STATE-SOURCE-OF-TRUTH]`

The compiler and tooling treat each state module as the single source of truth
for its cells. Cross-module reads and writes are both mediated by exported effect
operations; direct cell references cannot cross the boundary.

## Effects And Capabilities `[MODULES-EFFECTS]`

Modules do not hide effects. Exported functions carry ordinary Osprey effect
rows. Importing a module never grants ambient permission; a caller must still
handle or forward every effect.

State modules expose their state capability as algebraic effects:

```osprey
signature LedgerSig {
    effect Ledger {
        post : fn(int) -> int
        balance : fn() -> int
    }
}
```

This keeps application logic pure except for explicit `!Ledger`, while the
module decides whether state is in memory, SQLite, HTTP, or a test fake.

## Initialisation `[MODULES-INIT]`

Imports have no runtime effect. Module initialization is explicit.

- Pure `let` declarations may be evaluated at compile time or lowered as
  constants.
- Effectful setup for a plain module must live in an exported function.
- A state module's cell initializers run only when an installer creates its
  handler region; each installation receives fresh cells.
- Cyclic initialization between state modules is a compile-time error.

```osprey
state module DbStore {
    mut conn = None

    export effect DbStoreFx {
        connect : fn(string) -> Unit
    }

    export fn run(path: string, action: fn() -> Unit) -> Unit =
        handle DbStoreFx
            connect requestedPath => {
                conn = Some(perform Database.open(requestedPath))
            }
        in {
            perform DbStoreFx.connect(path)
            action()
        }
}
```

Here the exported installer never accesses `conn` directly; only its `connect`
handler arm does.

## Project Assembly `[MODULES-PROJECT]`

A project compile scans configured source roots, parses every
`.osp` and `.ospml` file, resolves each file's flavor, and builds one project
namespace graph.

```toml
[project]
name = "billing"
source_roots = ["src"]
default_namespace = "billing"
entry = "src/main.osp"

[modules]
allow_wildcard_imports = false
```

Single-file mode remains valid for scripts and examples. Project mode adds:

- all source files in the project graph;
- namespace merge;
- import resolution;
- signature checking;
- duplicate-name and ambiguity diagnostics;
- one entry point.

### Entry Point `[MODULES-ENTRYPOINT]`

In project mode, executable top-level statements are
allowed only in the designated entry file or in `fn main()`. Library files must
contain declarations only. This avoids hidden initialization order and makes
multi-file apps deterministic. `entry` is relative to the manifest and must name
a file under one source root. Test source roots belong to a separate test
configuration; a normal build does not silently compile them.

## Cycles `[MODULES-CYCLES]`

Namespace declarations may be mutually visible after merging,
but module implementation cycles are restricted.

- Pure type/function cycles are allowed only when ordinary Osprey recursion rules
  allow them.
- Signature cycles are allowed only through explicit opaque types.
- `state module` cycles are rejected.
- Parameterised modules may depend on signatures, not implementation bodies, to
  preserve separate compilation.

Recursive modules are a later feature and must require explicit signatures, as in
the ML literature.

## Name Mangling And ABI `[MODULES-ABI]`

Canonical symbol names include the namespace label and `::` path:

```text
billing::Tax::addTax
```

Codegen must mangle symbol paths deterministically and collision-free. The
mangled form is an implementation detail; diagnostics, docs, LSP, debugger, and
stack traces use source-level names.

Cross-flavor exports use the same ABI rules as
[Cross-Flavor Interop](/spec/0023-languageflavors/#cross-flavor-interop).

## Diagnostics `[MODULES-DIAG]`

Module diagnostics must be architecture-facing:

- unknown import: show candidate namespaces from the project graph;
- ambiguous import: show all providers and suggest aliases;
- exported private dependency: show the hidden type/value in the public signature;
- state scatter: show every state module in the namespace and require the state
  to be consolidated behind one algebraic-effect boundary;
- top-level mutable state: suggest `state module` or handler-owned state;
- path drift: warn, never change semantics.

## Examples `[MODULES-EXAMPLES]`

### Multi-file, Path-Independent Namespace `[MODULES-EXAMPLE-MULTIFILE]`

`src/a.osp`:

```osprey
namespace app;

fn hello(name: string) = "Hello ${name}"
```

`src/deeply/nested/b.ospml`:

```osprey
namespace app

greet name = hello name
```

Both files contribute to `app`. The path `deeply/nested` is irrelevant.

`src/http.osp` shows the optional slash label:

```osprey
namespace "app/http";

fn route() = "/"
```

That namespace is unrelated to `app`; import it with an alias when used from
ordinary expressions:

```osprey
import "app/http" as httpApp

let root = httpApp::route()
```

### Centralised State `[MODULES-EXAMPLE-STATE]`

```osprey
namespace app;

state module SessionStore {
    mut sessions = []

    export effect Sessions {
        add : fn(string) -> Unit
        count : fn() -> int
    }

    export fn run(action: fn() -> Unit) -> Unit =
        handle Sessions
            add id => { sessions = listAppend(sessions, id) }
            count => listLength(sessions)
        in action()
}

fn login(id: string) -> Unit !Sessions =
    perform Sessions.add(id)
```

Application code invokes `SessionStore::run` around a body that performs
`Sessions`. It cannot access `sessions`, and importing the module creates no
session store.

## References

- David L. Parnas. "On the Criteria To Be Used in Decomposing Systems into
  Modules." Communications of the ACM, 1972.
- David MacQueen. "Modules for Standard ML." LFP, 1984.
- John C. Mitchell and Gordon D. Plotkin. "Abstract Types Have Existential
  Type." POPL 1985 / TOPLAS 1988.
- Robert Harper and Mark Lillibridge. "A Type-Theoretic Approach to Higher-Order
  Modules with Sharing." POPL 1994.
- Xavier Leroy. "Manifest Types, Modules, and Separate Compilation." POPL 1994.
- Xavier Leroy. "Applicative Functors and Fully Transparent Higher-Order
  Modules." POPL 1995.
- Xavier Leroy. "A Modular Module System." JFP, 2000.
- Karl Crary, Robert Harper, and Sidd Puri. "What is a Recursive Module?" PLDI
  1999.
- Keiko Nakata and Jacques Garrigue. "Recursive Modules for Programming." ICFP
  2006.
- Andreas Rossberg, Claudio V. Russo, and Derek Dreyer. "F-ing Modules." TLDI
  2010 / JFP.
- Andreas Rossberg. "1ML - Core and Modules United." ICFP 2015 / JFP.
- Scott Kilpatrick, Derek Dreyer, Simon Peyton Jones, and Simon Marlow.
  "Backpack: Retrofitting Haskell with Interfaces." POPL 2014.
- Gordon Plotkin and Matija Pretnar. "Handlers of Algebraic Effects." ESOP 2009.
- John Launchbury and Simon L. Peyton Jones. "Lazy Functional State Threads."
  PLDI 1994.
- Simon Peyton Jones and Philip Wadler. "Imperative Functional Programming."
  POPL 1993.
- Ben Moseley and Peter Marks. "Out of the Tar Pit." 2006.
- Jean-Philippe Bernardy, Mathieu Boespflug, Ryan R. Newton, Simon Peyton Jones,
  and Arnaud Spiwack. "Linear Haskell: Practical Linearity in a Higher-Order
  Polymorphic Language." POPL 2018.
- Cong Ma, Zhaoyi Ge, Edward Lee, and Yizhou Zhang. "Lexical Effect Handlers,
  Directly." OOPSLA 2024.
- Microsoft. ".NET namespace guidance."
- Microsoft. "F# Namespaces" and "F# Modules."
- OCaml. "The OCaml Manual - The Module System."
- Simon Marlow, editor. "Haskell 2010 Language Report", Chapter 5, Modules.
- Elm. "Modules." Elm Guide.
- Clojure. "Namespaces." Clojure Guides.
- Oracle. "Java Language Specification", Section 6.1, Names.
- Redux. "Three Principles."