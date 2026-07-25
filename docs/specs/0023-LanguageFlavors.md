# Language Flavors

Osprey has two source syntaxes over one canonical AST. A flavor owns parsing
and lowering; type checking, effect checking, project assembly, and code
generation are shared.

The ML surface is specified in [ML Flavor Syntax](0024-MLFlavorSyntax.md).

## Status

Both frontends are shipped. Default (`.osp`) uses tree-sitter. ML (`.ospml`)
uses a hand-written layout lexer, recursive-descent parser, and CST-to-AST
lowerer. Both produce `osprey_ast::Program`.

The CLI flag, source marker, extension selection, mixed-flavor projects, and
cross-flavor AST/IR equivalence tests are active. First-class handler values and
multi-handler `do` installation are not part of the language; their reserved ML
keywords are rejected.

## The One Law

`[FLAVOR-BOUNDARY]` Everything below the canonical AST is flavor-specific.
Everything at or above `osprey_ast::Program` is shared.

Type inference, effect checking, project resolution, and code generation must
not branch on `Flavor`. `Parsed.flavor` is retained for frontend and editor
presentation only.

## Flavors That Exist

| Flavor | Blocks | Calls | Function default | Extension |
| --- | --- | --- | --- | --- |
| Default | braces | `f(x: a, y: b)` | flat multi-parameter | `.osp` |
| ML | offside layout | `f a b` | curried | `.ospml` |

Default remains the default API and source flavor. One file uses one flavor;
projects may contain both extensions.

## The Pipeline

```text
.osp   -> Default CST -> Default lowerer --+
                                             -> osprey_ast::Program -> checker -> codegen
.ospml -> ML CST      -> ML lowerer -------+
```

## Flavor Frontend

`[FLAVOR-FRONTEND]` `crates/osprey-syntax/src/lib.rs` exposes the shared entry
points:

```rust
pub enum Flavor { Default, Ml }

pub struct Parsed {
    pub program: Program,
    pub errors: Vec<SyntaxError>,
    pub flavor: Flavor,
}

pub fn parse_program(source: &str) -> Parsed;
pub fn parse_program_with_flavor(source: &str, flavor: Flavor) -> Parsed;
```

`parse_program` selects Default. `parse_program_with_flavor` dispatches to
`default::parse` or `ml::parse_ml`.

The physical split is:

```text
crates/osprey-syntax/src/
  lib.rs       selection and dispatch
  strings.rs   shared interpolation and escape helpers
  default/     tree-sitter parsing and lowering
  ml/          layout lexing, parsing, CST, and lowering
```

Each flavor supplies its own interpolation fragment parser to the shared text
helpers.

## Flavor Selection

`[FLAVOR-SELECT]` `osprey_syntax::resolve_flavor(flag, path, source)` applies
this precedence:

1. an explicit resolver override (`--flavor default|ml` for single-file CLI
   use, or `[project].flavor` while loading a project);
2. a leading `// osprey: flavor=default|ml` marker;
3. `.osp` or `.ospml` extension;
4. Default.

Without an explicit flag, a marker that disagrees with the extension is an
error. The CLI exits with that error. The language server reports one
`flavor-error` diagnostic rather than parsing under a guessed flavor.

## The Lowering Contract

`[FLAVOR-LOWER-CONTRACT]` A flavor lowerer must:

- return only canonical `osprey_ast` nodes;
- preserve source positions, documentation comments, and parameter names;
- erase spelling-only differences before semantic analysis; and
- reject a construct when the canonical AST cannot represent its semantics.

## Flavor Concern vs Shared-Core Concern

`[FLAVOR-LAYER]` The following shipped forms use the same canonical vocabulary.
Curried and flat functions intentionally have different canonical shapes, as
specified in [Currying Canonicalisation](#currying-canonicalisation).

| Concept | Default | ML | Canonical AST |
| --- | --- | --- | --- |
| immutable binding | `let x = e` | `x = e` | `Stmt::Let { mutable: false }` |
| mutable binding | `mut x = e` | `mut x = e` | `Stmt::Let { mutable: true }` |
| assignment | `x = e` | `x := e` | `Stmt::Assignment` |
| flat function | `fn f(x, y) = e` | `f (x, y) = e` | one two-parameter `Stmt::Function` |
| curried function | `fn f(x) = fn(y) => e` | `f x y = e` | one-parameter `Function` returning `Lambda` |
| flat call | `f(x: a, y: b)` | `f (a, b)` | one two-argument `Expr::Call` |
| curried call | `f(a)(b)` | `f a b` | nested one-argument `Expr::Call`s |
| lambda | `fn(x) => e` | `\x => e` | `Expr::Lambda` |
| block | `{ statements; value }` | indented region | `Expr::Block` |
| match | braced arms | indented arms | `Expr::Match` |
| equational clauses | explicit `match` | adjacent `f 0 = a` / `f n = b` | `Function` over `Match` |
| union | `type T = A \| B(X)` | `type T = A \| B X` | `Stmt::Type` |
| record construction | `T { f: v }` | `T(f = v)` or layout | `Expr::TypeConstructor` |
| record update | `r { f: v }` | `r(f = v)` | `Expr::Update` |
| list | `[a, b]` | `[a, b]` | `Expr::List` |
| map | `{ k: v }` | `[k => v]` | `Expr::Map` |
| index | `xs[i]` | `xs[i]` | `Expr::Index` |
| external function | `extern fn f(x: T) -> U` | `extern f (x : T) -> U` | `Stmt::Extern` |
| effect | braced operations | layout operations | `Stmt::Effect` |
| lexical handler | `handle E ... in body` | layout `handle E ... in body` | `Expr::Handler` |
| fiber operations | `spawn`, `await`, `yield`, `send`, `recv` | same keywords with ML application | shared expression nodes |

Positional union payloads use numeric internal field names that source cannot
spell. Wildcard parameters lower to generated names that source cannot spell.
Equational clauses are merged before AST lowering and may select on at most one
parameter column.

## Currying Canonicalisation

`[FLAVOR-CURRY]` The canonical function type is flat:
`Type::Fun { params, ret }`. Currying is represented by nesting one-parameter
function types and lambda/call nodes.

- Default `fn f(x, y)` and ML `f (x, y)` are flat and do not partially apply.
- Default `fn f(x) = fn(y) => ...` and ML `f x y = ...` are curried.
- ML `f a b` is `Call(Call(f, [a]), [b])`; ML `f (a, b)` is
  `Call(f, [a, b])`.

The AST-equivalence tests assert three buckets: the curried twins are equal,
the flat twins are equal, and ML curried is not equal to Default flat.

## Shared-Core Additions

`[FLAVOR-HANDLER-VALUE]` First-class handler values, a `Handler E` type, and
multi-handler `do` installation are absent from the canonical AST and type
system. The ML lexer reserves `handler` and `do`; the parser reports
`not yet supported`. Both flavors currently use lexical
`Expr::Handler { effect, arms, body }`.

## Cross-Flavor Interop

`[FLAVOR-INTEROP]` Project assembly parses each source with its own flavor and
merges the resulting canonical declarations into one namespace graph.

A Default flat function is called from ML with `f (a, b)`. An ML curried
function remains curried when imported; a Default caller applies the returned
function value explicitly. Records, unions, results, and effects retain one
canonical identity across flavors.

## Cross-Flavor Equivalence Tests

`[FLAVOR-TEST]` `crates/osprey-cli/tests/cross_flavor_equiv.rs` compares
canonical ASTs after removing source positions. It covers equal curried twins,
equal flat twins, and the deliberately unequal curried/flat pair.

`[FLAVOR-IR-EQUIV]` `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs` compiles
paired `.osp` and `.ospml` examples and requires byte-identical LLVM IR. Each ML
example has a Default twin and shares its expected-output file, except for the
small explicit ML-only allowlist in that test.

## Cross-references

- [ML Flavor Syntax](0024-MLFlavorSyntax.md)
- [Syntax](0003-Syntax.md)
- [Type System](0004-TypeSystem.md)
- [Algebraic Effects](0017-AlgebraicEffects.md)
- [Modules and Namespaces](0025-ModulesAndNamespaces.md)
