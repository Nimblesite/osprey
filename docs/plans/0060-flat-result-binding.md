# Plan 0060 — Result and error handling

`Result<T, E>` carries an exact error payload at type `E` end to end ([ERR-RESULT], [ERR-PAYLOAD], [ERR-RESULT-FORMAT]) and fallible steps read top to bottom through flat `use` binding ([BLOCK-USE], [BLOCK-USE-RESULT]); both ship, and the prelude's `andThen` is the only one. What remains is the rewrite of the last callback pyramids, the nesting lint as an error, and the whole-repository gate. The or-pattern and `min`/`max` gaps are in [their plan](or-patterns-and-min-max.md).

Spec homes: [Block Expressions](../specs/language/0008-BlockExpressions.md), [Error Handling](../specs/language/0013-ErrorHandling.md), [Type System](../specs/language/0004-TypeSystem.md), [Pattern Matching](../specs/language/0007-PatternMatching.md), [Testing](../specs/language/0027-TestingFramework.md), [Built-ins](../specs/runtime/0012-Built-InFunctions.md) and [ML Flavor Syntax](../specs/language/0024-MLFlavorSyntax.md). Related contracts: [TYPE-CONSTRAINTS], [PATTERN-NESTED], [TYPE-UNION-POSITIONAL] and [FLAVOR-ML-MATCH].

## Rewrite the remaining pyramids

- [ ] Rewrite every remaining callback pyramid with `use`, deepest first, naming inner values where the nesting is data rather than callbacks; remove the lambda type annotations the rewrite makes unnecessary, compare emitted IR before and after each project rewrite, and review each mechanical rewrite by hand. Check: the `closing-run` lint reports nothing under `src/osprey` (candidate sites remain in `src/osprey/examples/daw/test/{main,room,voice}.ospml` and `src/osprey/packages/synth/src/resources.ospml`).

## Nesting lint [LINT-NESTING-LAMBDA]

The static-analysis step of CI phase 1 makes nesting an error with no excluded file; the message points to `use`.

- [ ] Add the lambda-depth and `match`-depth rules (more than 2 deep) beside the closing-run rule in `src/toolchain/crates/osprey-syntax/src/closing_runs.rs` and `src/toolchain/crates/osprey-types/src/source_warnings.rs`, and describe all three in the `[LINT-NESTING-LAMBDA]` section of the Type System spec. The closing-run threshold stays at 8 closers on one line: counting newlines flagged 2865 sites of ordinary layout.
- [ ] Make `closing-run`, lambda depth and `match` depth `deny` by default in `src/toolchain/crates/osprey-types/src/lint_level.rs` (`closing-run` is `warn` today) once the rewrite leaves no site.

## Gate

- [ ] No function in `src/osprey` nests a lambda or a `match` more than 2 deep. Check: the three lint rules are green with no exclusions.
- [ ] `make ci` green with every generic-error, flat-binding and assertion regression; `src/toolchain/crates/run_test_corpus.sh` under GC, ARC and wasm32; the manager packages' E2E suites; `src/osprey/tests/coverage-thresholds.json` not down; deslop `top-offenders` showing no new duplication.
- [ ] Deliver or-patterns ([PATTERN-OR](../specs/language/0007-PatternMatching.md#or-patterns--pattern-or)) and `min`/`max` ([BUILTIN-MIN-MAX](../specs/runtime/0012-Built-InFunctions.md)) in [their plan](or-patterns-and-min-max.md). Check: that plan's items ticked.

## Design basis

`use x <- f(a, b)` desugars in the parser to `f(a, b, fn(x) => <rest of block>)`, so the checker, inference and codegen see only the ordinary call and lambda ([BLOCK-USE], [LEX-USE-ARROW]). It is general rather than `Result`-only, because Gleam shipped a `Result`-only `try` and then deleted it as redundant; the bind function is named at the call site because Osprey has no typeclasses or overloading; the binder comes first, as in Haskell and Jane Street's `let%bind`. Postfix `?` is rejected because it clashes with the ternary `? :` and the `?:` token ([LEX-RESULT-DEFAULT-TOKEN]) and returns from the whole function, and a `Refuse` effect is rejected because it turns total data flow into control flow and makes every caller install a handler.

Sources: [Haskell 2010 Report §3.14](https://www.haskell.org/onlinereport/haskell2010/haskellch3.html), [OCaml binding operators](https://ocaml.org/manual/5.2/bindingops.html), [Jane Street, Let syntax](https://blog.janestreet.com/let-syntax-and-why-you-should-use-it/), [F# computation expressions](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/computation-expressions), [Gleam `use` expressions](https://gleam.run/news/v0.25-introducing-use-expressions/), [Gleam, goodbye `try`](https://gleam.run/news/v0.27-hello-panic-goodbye-try/), [Rust `?`](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-try-propagation-expression), [Elm `Result.andThen`](https://package.elm-lang.org/packages/elm/core/latest/Result#andThen).
