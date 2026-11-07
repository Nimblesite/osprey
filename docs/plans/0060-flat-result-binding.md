# Plan 0060: Flat Binding — No More Callback Pyramids

Code that runs fallible steps one after another must read top to bottom, one
step per line, with no nesting and no run of closing brackets. Today the only
way to sequence `Result`s without a hand-written `match` ladder is to nest
`andThen(x, fn(v) => andThen(...))`. That nesting is now banned, and so is the
equivalent `match` ladder.

Spec homes: [Block Expressions](../specs/language/0008-BlockExpressions.md),
[Error Handling](../specs/language/0013-ErrorHandling.md) and
[ML Flavor Syntax](../specs/language/0024-MLFlavorSyntax.md). The `[BLOCK-USE-*]`
and `[LINT-NESTING-*]` rules below are added to them when this plan lands.

> **Gate:** `AGENTS.md` puts the repo in stabilization mode. `use` is new
> syntax, so the user must approve this plan before Phases 1–3 start. Phase 4
> (the lint, shipped red with its findings as the backlog) does not depend on
> the new syntax.

## Evidence: the problem, measured

Measured on 2026-11-07 with `zai/glm-5.3` through pi; three functions were
checked by hand.

- **Spread:** 226 functions in 46 files call `andThen`.
  - 30 of them nest it 3 or more levels deep.
  - 18 nest it 5 or more levels deep.
- **Redundant annotations:** the lambdas carry 122 type annotations (42 in
  `andThen` functions). These break the AGENTS.md rule against redundant
  annotations.
- **Guards:** 65 `fn(_) =>` guard steps.
- **Worst cases:**

| depth | where | closing run |
| ---: | --- | ---: |
| 12 | `tests/core/gpu/transformer.test.osp:77` `blockParams` | 14 |
| 12 | `tests/effects/staged/tensor_backends.test.osp:127` `blockParams` | 14 |
| 10 | `tests/effects/staged/tensor_backends.test.osp:175` `block` | 11 |
| 7 | `packages/manager/core/src/governance_handover.osp:38` `acceptHandover` | 10 |
| 7 | `packages/manager/core/src/governance_transitions.osp:187` `authorize` | 8 |
| 6 | `governance.osp:187` `requireAdmin` and 5 more governance transitions | 8–10 |

**Why it happens:** Osprey has no sugar for "bind this, then carry on". A
function with *n* fallible steps in a row therefore ends up nested *n* levels
deep.

## Evidence: how other ML-family languages handle it

Every major ML-family language reached the same answer: **a binding form that
turns the rest of the block into the callback.** The programmer writes flat
steps, and the compiler builds the nesting.

| Authority | Construct | What the primary source says |
| --- | --- | --- |
| Haskell 2010 Report §3.14 | `do { p <- e; stmts }` | The report defines it as plain sugar: `do {p <- e; stmts}` = `let ok p = do {stmts} … in e >>= ok`, and `do {e; stmts}` = `e >> do {stmts}`. |
| OCaml manual, *Binding operators* (since 4.08) | `let* x = e in body` | "introduced to offer convenient syntax for working with monads"; "`let<op> x1 = e1 in e` can be desugared into an application `( let<op> ) e1 (fun x1 -> e)`". |
| Jane Street, *Let syntax, and why you should use it* | `let%bind` (ppx_let, the precursor of `let*`) | Nested `>>=` lambdas are replaced so that "the meaningful variable name comes first", and the diffs become "more targeted". |
| F# language reference, *Computation Expressions* | `result { let! x = e … }` | "`{{ let! pattern = expr in cexpr }}` → `builder.Bind(expr, (fun pattern -> {{ cexpr }}))`". `do!` binds a step whose value is unit, which is the guard case. |
| Gleam v0.25, *Introducing use expressions* | `use x <- f(args)` | "syntactic sugar that turns all following expressions into an anonymous function that gets passed as an additional argument to a function call". It was introduced because callback code had too much indentation. |
| Gleam v0.27, *Hello panic, goodbye try* | removed the Result-only `try x = e` keyword | "Now that we have `use` expressions, the less general `try` expressions are redundant. We always prefer to have fewer ways to do the same thing." The documented replacement is `use file <- try(open_file())`. |
| Rust Reference, `[expr.try]` | `e?` | `Ok(val)` evaluates to `val`; `Err(e)` returns `Err(From::from(e))` from the function. The error passes through unchanged. |
| Elm core, `Result.andThen` | `r \|> andThen f \|> andThen g` | A flat pipeline, which works only when a step does not need an earlier step's value. |
| OCaml 5 manual, *Effect handlers* | `perform` / `discontinue` | "a generalization of exception handlers". |

What Osprey takes from this:

1. **The binding form is sugar, and its desugaring is the callback.** Haskell,
   OCaml, F# and Gleam all define it this way. It needs no new types, no
   runtime support and no codegen change.
2. **Make it general, not specific to `Result`.** Gleam shipped a `try` that
   only worked on `Result`, then deleted it as redundant once `use` existed.
   Osprey should skip that dead end. A general `use` covers `Result`, the
   tensor `andThen`s in the tests, resource scopes and handler callbacks.
3. **The bind function is named at the call site.** Gleam does this, and so
   does OCaml when the `let*` operator is opened locally. Osprey has no
   typeclasses or operator overloading, so naming the function explicitly is
   the only honest choice. It also keeps the code readable without an IDE.
4. **The binder comes first** (Jane Street, Haskell). The name a step
   produces sits on the left edge of its line.
5. **Rust's `?` is rejected.** Postfix `?` clashes with the ternary `? :` and
   with the `?:` token (`[LEX-RESULT-DEFAULT-TOKEN]`). It is also a return
   that leaves the whole function, and Osprey has no returns.
6. **Effects are rejected for this case.** A `Refuse` effect with an abort arm
   (Koka and OCaml 5 style) would work. It is the wrong tool here, though: it
   turns plain, total data flow into control flow, and every caller needs a
   handler just to get its `Result` back. Effects stay the tool for real
   cross-cutting concerns, such as logging.

## The design

### `[BLOCK-USE]` `use` binding

- **Default flavor:** `use x <- f(a, b)`. **ML flavor:** `use x <- f (a, b)`.
- Either form is a statement inside a block. It desugars to
  `f(a, b, fn(x) => <rest of block>)`, which is Gleam's rule exactly.
- **Binders:** `use _ <- f(...)` makes a guard step (F#'s `do!`), and
  `use <- f(...)` passes a callback that takes no argument.
  `use a, b <- f(...)` binds a callback with several parameters.
- **Lowering:** the parser does all of this in AST lowering. The checker, the
  type inference and codegen see only the ordinary call and lambda. The
  desugared code is therefore *the same AST* as today's hand-written pyramid.
  That gives the implementation an exact test: the IR is identical before and
  after the rewrite.

### `[BLOCK-USE-RESULT]` One bind function for `Result`

The standard library gains one `Result` bind function, `try(result, next)`.
Its behaviour is today's `Checked::andThen`: it passes `Error { message }`
through unchanged (`[ERR-PAYLOAD]`). Every copy of `andThen` in packages and
tests is deleted and replaced by this one function.

### `[BLOCK-USE-REJECT]` Rejections

Each of these is rejected with a message that names the fix:

- `use` as the last line of a block. The callback would have no body. F#
  rejects the same thing: "`let!` still requires a following body".
- `use` outside a block.
- A right-hand side that is not a call.
- A callee whose last parameter is not a function of the bound arity.

A bare `Result` in statement position stays rejected (`[ERROR-RESULT-DISCARD]`).

### `[LEX-USE-ARROW]` Lexing

`<-` is a single token, but only in the head of a `use` statement. Everywhere
else, `x<-1` still lexes as `x < -1`, so no existing program changes meaning.

### `[LINT-NESTING-LAMBDA]` Nesting is an error

The static-analysis step of CI phase 1 rejects any Osprey source that has:

- lambdas nested more than **2** deep,
- `match` arms nested more than **2** deep, or
- a run of **4 or more** closing delimiters (`)`, `}` or `]`, ignoring
  whitespace).

This is an error, not a warning, and no file is excluded. The message points
to `use`.

## Before and after: `acceptHandover`

Before (depth 7, ending in a run of 10 closing brackets):

```osprey
Checked::andThen(Governance::transferOf(registry, transferId), fn(t: Governance::Transfer) =>
    Checked::andThen(liveTransfer(t), fn(_) =>
        ... five more levels ...
                    Success { value: ... })))))))
```

After, in the ML flavor:

```osprey-ml
export acceptHandover (registry, transferId, proof) =
    use t <- try (G::transferOf (registry, transferId))
    use _ <- try (live t)
    use m <- try (G::membershipOf (registry, t.proposed))
    use p <- try (G::principalOf (registry, m.principal))
    use _ <- try (liveAcceptingHuman (p, proof))
    use pkg <- try (G::packageOf (registry, t.package))
    use _ <- try (G::requireProof (proof, m.principal, t.proposed, acceptance (t, pkg)))
    Success (value = G::replaceTransfer (registry, t { acceptance = Maybe::some proof, status = G::Accepted }))
```

There is one level of indentation, no lambda annotations and no closing run.
The binder is on the left of every step, and the answer is the last line.
`blockParams` (depth 12) becomes 12 `use` lines followed by a record.

## Phases

- [ ] **1. Tests first (red).**
  - Add a regression to `src/osprey/tests/regressions/` with ML and Default
    twins that share one golden output. It covers:
    - 1, 2, 8 and 12 binds in a row;
    - an `Error` at every position, with the exact payload checked;
    - `_` guards and the binder-less `use <-`;
    - binders with several parameters;
    - `use` inside `match` arms, handler arms and lambdas;
    - a non-`Result` callee (a resource scope);
    - a check that the IR is byte-identical to the equivalent hand-written
      pyramid.
  - Add one `failscompilation` program per rule in `[BLOCK-USE-REJECT]`.
  - Add a `[LEX-USE-ARROW]` test showing that `x<-1` is still a comparison.
- [ ] **2. Compiler.**
  - Add `use` and `<-` to the tree-sitter grammar and the ML lexer.
  - Add the `use` statement to both parsers.
  - Desugar it in AST lowering.
  - Add `try` to the prelude.
  - Update the specs, and the LSP's hover and highlighting.
- [ ] **3. Rewrite every pyramid.** Work through the 46 files, deepest first:
  - Rewrite each pyramid with `use`, and delete every local `andThen`.
  - Remove the lambda type annotations the rewrite makes unnecessary.
  - Import `registry::Governance as G`.
  - Flatten the nested `match bool` ladders (`withinHold`, `stillCurrent`)
    into guard steps.
  - Merge the duplicate guard helpers (`requiredTransfer` and
    `Governance::required`).
  - Hand the mechanical rewrites to pi, in the order AGENTS.md sets. Review
    every diff by hand.
- [ ] **4. Lint.** Add `[LINT-NESTING-LAMBDA]` to CI phase 1. It must be green
  on the whole tree after Phase 3, with no exclusions.
- [ ] **5. Prove it.**
  - All three corpus runs pass: `run_test_corpus.sh` under GC, ARC and wasm32.
  - Run the manager packages' E2E suites.
  - `coverage-thresholds.json` does not go down.
  - deslop `top-offenders` shows no new duplication.

## Done when

- `rg '\bandThen\(' src/osprey` returns nothing.
- The nesting lint is green with no exclusions.
- No function in `src/osprey` nests a lambda or a `match` more than 2 deep.

## Sources

- Haskell 2010 Report, §3.14 Do Expressions:
  <https://www.haskell.org/onlinereport/haskell2010/haskellch3.html>
- OCaml manual, Binding operators:
  <https://ocaml.org/manual/5.2/bindingops.html>
- OCaml manual, Effect handlers: <https://ocaml.org/manual/5.2/effects.html>
- Jane Street, Let syntax, and why you should use it:
  <https://blog.janestreet.com/let-syntax-and-why-you-should-use-it/>
- F# language reference, Computation Expressions:
  <https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/computation-expressions>
- Gleam v0.25, Introducing use expressions:
  <https://gleam.run/news/v0.25-introducing-use-expressions/>
- Gleam v0.27, Hello panic, goodbye try:
  <https://gleam.run/news/v0.27-hello-panic-goodbye-try/>
- Rust Reference, the try propagation expression:
  <https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-try-propagation-expression>
- Elm core, `Result.andThen`:
  <https://package.elm-lang.org/packages/elm/core/latest/Result#andThen>
