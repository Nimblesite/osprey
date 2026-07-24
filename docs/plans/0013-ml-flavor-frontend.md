# Plan 0013 — ML Flavor Frontend

**Status:** The ML frontend is **implemented and green**. The layout lexer,
recursive-descent parser, CST, and lowerer are complete
(`crates/osprey-syntax/src/ml/`); flavor selection (flag > marker > extension)
works; **68 `.ospml` tested twins** run byte-identically to their `.osp`
counterparts (including effects, `handle … in`, and `resume`); cross-flavor
AST- and IR-equivalence tests pass
(`crates/osprey-cli/tests/cross_flavor_{equiv,ir_equiv}.rs`); the VSIX ships
full ML support (`osprey-ml` language, TextMate grammar, layout config,
snippets); specs 0023/0024 mirror to the website; **five ML must-reject
fixtures** golden-guard the frontend's rejection paths; and the **LSP now
answers in the authoring flavor** (`[LSP-FLAVOR-RENDER]`, spec 0020) with one
shared `[FLAVOR-SELECT]` precedence chain. **Two items remain:** first-class
handler *values* (Phase 0 — `handler E {}` / `handle a b do body` still error
as reserved words) and the optional `osprey convert` transliterator. See
[§What is left](#what-is-left-detailed).

## Summary

Add the **ML flavor** — a layout-based, curry-by-default source surface — as a
second frontend **alongside** the existing Default (brace) flavor, not as a
replacement. Both frontends lower to the same `osprey_ast::Program`; everything
from type inference onward is shared and flavor-blind. The normative contract is
[spec 0023 — Language Flavors](../specs/0023-LanguageFlavors.md); the ML surface
is [spec 0024 — ML Flavor Syntax](../specs/0024-MLFlavorSyntax.md).

This plan supersedes the earlier "one canonical layout form, remove braces"
rollout drafts. Osprey keeps both surfaces permanently. The work is therefore
**additive**: a new parser, a new lowerer, a flavor selector, and one
shared-core feature the ML examples depend on — never a migration that rewrites
the Default flavor out of existence.

**Implementation decision — hand-written Rust layout frontend.** The ML
frontend is implemented as a **hand-written Rust layout lexer +
recursive-descent (Pratt / precedence-climbing) parser** in
`crates/osprey-syntax/src/ml/` (`token.rs`, `lexer.rs`, `cst.rs`, `parser.rs`,
`lower.rs`, `mod.rs`). The parser produces an ML **concrete syntax tree (CST)**;
a separate lowerer (`lower.rs`) converts it to canonical `osprey_ast::Program`
(clean **CST→AST separation**). The lexer derives
layout markers (`Indent`/`Dedent`/`Newline`) from the **offside rule**
(Landin 1966) via an explicit indentation stack, with bracket depth suppressing
layout inside parentheses. This **supersedes** the earlier plan of a
`tree-sitter-osprey-ml` grammar with an external C scanner. Rationale: the
offside rule is naturally expressed with an explicit indent stack in safe Rust;
it stays panic-free / `Result`-returning and unit-testable (project rules), with
no `unsafe` C and no codegen-tool build dependency. Per
[`[FLAVOR-BOUNDARY]`](../specs/0023-LanguageFlavors.md#the-one-law) the parser
**mechanism** is a below-the-AST, flavor-internal concern, so this swap does not
change the architecture (many CSTs, one AST). The tree-sitter + `scanner.c`
approach is retained as a documented **fallback (escape hatch)** in Phase 2. The
parsing techniques are cited in
[spec 0024 References](../specs/0024-MLFlavorSyntax.md#references).

**Current state.** Phases 1–4 (flavor seam, ML lexer/parser/lowerer, flavor
selection) are **implemented and green**; 68 `.ospml` twins pass under the diff
harness. Phase 5 is largely done (twins + equivalence tests) **except** ML
must-reject cases. Phase 6/7 tooling and docs are done **except** the optional
`osprey convert` transliterator. **Phase 0 — first-class handler *values* —
remains the one real feature gap:** `perform` and `handle … in` work, but the
`handler E { … }` value form and `handle a b do body` multi-install still error
loudly (`handler`/`do` are `Reserved` tokens, `crates/osprey-syntax/src/ml/token.rs:128`),
because the shared-core `Expr::HandlerValue`/`Expr::Install` nodes do not exist
yet. This is a flavor-neutral shared-core addition, not ML-specific parser work.

> **Note.** The original plan sequenced Phase 0 *before* the ML frontend. In
> practice the frontend shipped first using the existing fused
> `Expr::Handler { effect, arms, body }` (which `handle … in`/`handle … do`
> both lower to), so ML effects work today. Handler *values* are now a
> follow-up, tracked jointly with the effects roadmap
> ([plan 0016](0016-algebraic-effects-and-handlers.md)).

## Why this is cheap (and where it is not)

The post-AST pipeline is already flavor-agnostic by construction:

- The type checker `check_program` / `infer_program`
  (`crates/osprey-types/src/check.rs:480`/`:493`) and code generator
  `compile_program` (`crates/osprey-codegen/src/lower.rs:20`) consume **only**
  `osprey_ast::Program` and the inferred type tables. Neither imports
  `osprey_syntax` or `tree_sitter`. No string `"flavor"` exists in the compiler.
- The Default lowerer (`crates/osprey-syntax/src/lower.rs`, `…/expr.rs`) already
  walks generic CST nodes by `kind()` and field name, so a second lowerer reuses
  the canonical AST vocabulary directly.
- **Currying needs no core change.** `Type::Fun` (`…/osprey-types/src/ty.rs:67`)
  is flat multi-arity; a curried function is nested `Fun` + nested one-param
  `Expr::Lambda` + nested one-arg `Expr::Call` — all implemented today
  (lambdas-as-values: [plan 0002](0002-codegen-generic-function-values.md)). The
  ML lowerer does the currying desugar; the checker and codegen are untouched.

The genuinely new work is two things: **(a)** a layout-sensitive parser — a
hand-written Rust layout lexer + recursive-descent (Pratt /
precedence-climbing) parser in `crates/osprey-syntax/src/ml/`, deriving layout
from the offside rule via an explicit indentation stack — and **(b)** one
shared-core feature — **first-class handler values + multi-install** — because
`Expr::Handler { effect, arms, body }` (`crates/osprey-ast/src/lib.rs:451`) fuses
construction and installation and cannot express `db = handler Db …; handle db
log do body`. That feature is flavor-neutral and lands first.

## Architecture (grounded)

| Stage | Today | After |
| --- | --- | --- |
| entry | `parse_program(src)` (`osprey-syntax/src/lib.rs:37`) | `parse_program_with_flavor(src, flavor)`; `parse_program` = Default wrapper |
| parse | tree-sitter brace grammar (`tree-sitter-osprey/`) | + hand-written Rust layout lexer + recursive-descent parser (`osprey-syntax/src/ml/`); tree-sitter + `scanner.c` retained as fallback |
| lower | `Lowerer` (`lower.rs`/`expr.rs`) → `Program` | + ML `lower.rs`: ML CST → the same `Program` (the parser builds the CST) |
| select | n/a | CLI flag > marker > extension > Default (`osprey-cli/src/main.rs:119`/`:200`) |
| check/codegen | `Program`-only, flavor-blind | **unchanged** |

## What is left (detailed)

The frontend is done. Three concrete gaps remain, each with a failing repro:

1. **First-class handler values (Phase 0) — the one real feature gap.**
   `handler Log { info m => … }` bound to a name, and multi-install
   `handle a b do body`, both error today (`unexpected token
   Reserved("handler")`). Needs the shared-core `Expr::HandlerValue` /
   `Expr::Install` nodes (§Phase 0 TODO below), a `Handler E` type, and
   Default + ML surfaces. Flavor-neutral; tracked jointly with
   [plan 0016](0016-algebraic-effects-and-handlers.md).
2. **ML must-reject cases** — ✅ **done.** Five `.ospo` + `// osprey: flavor=ml`
   fixtures now pin the handler-value gap, layout indentation, unterminated
   `(**`, the `->`/`=>` match-arm confusion, and the brace/`?` lexemes ML
   deliberately omits; the in-process corpus test was fixed to parse them
   under the ML frontend rather than the brace grammar. See §Phase 5.
3. **`osprey convert` transliterator** — optional Default ⇄ ML source
   conversion (§Phase 6 TODO). Nice-to-have, no dependency.

Everything else in the phase TODOs below is checked.

## Phase 0 — Shared-core: first-class handler values — ⬜ DEFERRED

Flavor-neutral. Originally sequenced first; in practice the ML frontend
shipped without it (using the fused `Expr::Handler`), so this is now a
follow-up — the **last** ML feature gap. See
[FLAVOR-HANDLER-VALUE](../specs/0023-LanguageFlavors.md#shared-core-additions)
and [plan 0016](0016-algebraic-effects-and-handlers.md), which owns the effect
runtime this builds on.

TODO:

- [ ] Add `Expr::HandlerValue { effect, arms }` and
      `Expr::Install { handlers: Vec<Expr>, body }` to `osprey-ast`.
- [ ] Make the existing `Expr::Handler { effect, arms, body }` sugar for
      `Install { [HandlerValue { … }], body }` so all current Default programs
      keep compiling unchanged.
- [ ] Add a `Handler E` type to `osprey-types`; check arm/operation coverage.
- [ ] Type-check `Install` handler lists; detect duplicate installed handlers.
- [ ] Preserve handler-owned `mut` state on the handler value
      ([Algebraic Effects](../specs/0017-AlgebraicEffects.md) `[EFFECTS-HANDLER-STATE]`).
- [ ] Codegen: a runtime handler-value representation; lower `Install` of N
      values to nested handler installation; preserve behaviour across the C
      HTTP-callback and fiber boundaries; keep `resume` working.
- [ ] Default-flavor surface for the feature: `let h = handler E { … }` value
      form and multi-handler `handle h1 h2 in { body }`; grammar + lowerer.
- [ ] Tests: handler value bound/returned/passed; state isolation vs sharing;
      multi-install; existing effect examples still pass byte-for-byte.

## Phase 1 — Flavor frontend seam

**Implemented and green.** No behaviour change; Default stays the default.

TODO:

- [x] Add `enum Flavor { Default, Ml }` and `flavor: Flavor` on `Parsed`
      (`osprey-syntax/src/lib.rs:28`).
- [x] Add `parse_program_with_flavor(src, flavor) -> Parsed`; keep
      `parse_program` as the `Flavor::Default` wrapper.
- [x] Define the `FlavorFrontend` trait (`parse_tree` / `lower` /
      `collect_errors`); reorganise the current code as `default_frontend`.
- [x] Thread flavor through the interpolation re-entry (`expr.rs`
      `parse_fragment`, which recurses into `parse_program`).
- [x] Update callers (CLI, LSP, tests) to pass a flavor; all default to
      `Default`.

## Phase 2 — ML layout lexer + recursive-descent parser — ✅ DONE

Hand-written Rust frontend in `crates/osprey-syntax/src/ml/` (`token.rs`,
`lexer.rs`, `cst.rs`, `parser.rs`, `lower.rs`, `mod.rs`): the parser builds an ML
**concrete syntax tree (CST)** and a separate `lower.rs` converts the CST to
canonical `osprey_ast::Program` (clean **CST→AST separation**). The tree-sitter + `scanner.c` approach is the
documented **fallback (escape hatch)** below, not the primary path.

TODO:

- [x] Layout lexer (`lexer.rs` + `token.rs`): `Indent`/`Dedent`/`Newline` from
      the offside rule via an indentation stack; bracket depth suppresses
      layout inside parentheses; blank/comment lines ignored; row/column
      preserved. Panic-free, `Result`-returning, unit-tested (`ml_coverage.rs`).
- [x] ML CST types (`cst.rs`): ML-spelling surface nodes, not desugared.
- [x] Recursive-descent parser (`parser.rs`): tokens → ML CST.
- [x] Lowerer (`lower.rs`): ML CST → canonical `Program`; currying desugar +
      `${…}` interpolation live here.
- [x] Pratt / precedence-climbing expression layer.
- [x] Rust unit tests for indentation, match/handler arms, and edge cases.
- [x] Module wiring: `mod ml`; no external build step, no `unsafe`.

> **Escape hatch (documented fallback, not the primary path).** If the
> hand-written layout frontend becomes onerous or accrues parsing bugs we cannot
> tame, we fall back to a `tree-sitter-osprey-ml` grammar with an external
> `INDENT`/`DEDENT`/`NEWLINE` `scanner.c` (an indentation-stack scanner the brace
> grammar has never needed — `tree-sitter-osprey/` ships no `scanner.c` today),
> a tree-sitter grammar for the ML rules, a tree-sitter corpus test suite, and a
> separate `MlLowerer`. The boundary law
> ([`[FLAVOR-BOUNDARY]`](../specs/0023-LanguageFlavors.md#the-one-law)) makes the
> parser mechanism a flavor-internal swap that leaves the AST and everything
> above it untouched.

## Phase 3 — ML lowerer (CST → canonical AST) — ✅ DONE (except handler values)

Obeys the [lowering contract](../specs/0023-LanguageFlavors.md#the-lowering-contract).

TODO:

- [x] ML `lower.rs` producing `osprey_ast::Program`; spans + doc comments
      preserved.
- [x] Bindings: `x = e` → `Let{false}`; `mut x = e` → `Let{true}`; `x := e` →
      `Assignment`.
- [x] **Currying desugar** ([FLAVOR-CURRY](../specs/0023-LanguageFlavors.md#currying-canonicalisation));
      equals Default explicit-curry AST, differs from Default multi-param
      (pinned by `cross_flavor_equiv.rs`).
- [x] Effects: `op : P => R` → `EffectOperation`; `handle … in`/`… do` →
      `Expr::Handler`; `perform E.op a` → `Expr::Perform`.
- [ ] **Handler values**: `handler E` → `HandlerValue`; `handle a b do body`
      → `Install` (blocked on Phase 0's shared-core nodes — the ONE lowering
      arm still missing; `handler`/`do` remain `Reserved` tokens).
- [x] Match: layout arms → `Match`/`MatchArm`; `Success value` →
      `Constructor { fields:["value"] }`.
- [x] Records: layout block → `TypeConstructor`; layout update → `Update`.
- [x] Diagnostics with flavor-aware fix wording.

## Phase 4 — Flavor selection wiring

**Implemented and green.**

TODO:

- [x] CLI `--flavor default|ml` on `Cli` (`osprey-cli/src/main.rs:34`), parsed in
      `parse_args` (`:119`); update `USAGE` (`:25`).
- [x] File marker `// osprey: flavor=ml` via the `directive` parser (`:521`),
      read in `run` (`:200`) before parsing.
- [x] Extension detection: `.ospml` ⇒ ML, `.osp` ⇒ Default (`Path::extension`).
- [x] Precedence flag > marker > extension > Default; **error** (hard, not a
      silent guess) when extension and marker disagree.
- [x] Diff harness (`crates/diff_examples.sh`) discovers `.ospml` **additively**
      and resolves flavor by extension; existing `.osp` discovery unchanged.
- [ ] Optional `osprey.toml` `flavor` key (deferred; not in the current
      precedence chain).
- [x] LSP resolves the same precedence per document — **done**. Every
      document-scoped feature routes through one `flavor_of` helper wrapping
      `osprey_syntax::resolve_flavor` (`osprey-lsp/src/features.rs`), so a
      `// osprey: flavor=ml` marker outranks the extension in hover, completion
      and signature help exactly as it does for the CLI. `completion` had been
      sniffing `Path::extension` directly and ignoring the marker. A
      marker/extension **conflict** is now a `flavor-error` diagnostic on the
      marker line instead of a silent `unwrap_or(Default)` that reported the
      file as green while the CLI refused to build it
      (`diagnostics::compute`). Pinned by
      `a_flavor_marker_that_fights_the_extension_is_reported_not_guessed` and
      `ml_completions_never_offer_a_keyword_the_ml_frontend_does_not_have`.

## Phase 5 — Tests, examples, equivalence — ✅ DONE

TODO:

- [x] **68 `.ospml` tested twins** under `examples/tested/**` with shared
      `.expectedoutput` goldens, covering currying/partial application, `=>`
      effect operations, `handle … in`, `resume`, layout match/records,
      bindings/mutation, and interpolation.
- [x] **No regressions**: `.ospml` discovery is additive; every `.osp` fixture
      still passes byte-for-byte.
- [x] **Cross-flavor equivalence tests** (`cross_flavor_equiv.rs` AST-level,
      `cross_flavor_ir_equiv.rs` byte-identical LLVM IR, in Rust).
- [x] **ML must-reject cases** under `examples/failscompilation/` — five
      fixtures, each pinning a distinct ML rejection path: `handler` at item
      position ([FLAVOR-ML-HANDLER], the deferred Phase 0 gap), inconsistent
      layout indentation ([FLAVOR-ML-LAYOUT]), an unterminated `(**` doc
      comment ([FLAVOR-ML-COMMENTS]/[DOC-SIGIL-ML]), a `->` where a match arm
      needs `=>` ([FLAVOR-ML-MATCH]), and the brace-record + `?` sigil that ML
      deliberately does not lex ([FLAVOR-BOUNDARY]). Ratchet stays honest:
      `FC_REJECT` 68 → 73, `FC_ESCAPE` unchanged at 11 =
      `FC_EXPECTED_ESCAPES`. Each was cross-checked so the marker path and
      `--flavor ml` emit byte-identical diagnostics, while `--flavor default`
      emits a completely different tree-sitter error — proving the rejection
      comes from the ML frontend, not from ML source merely being invalid
      Default syntax.
- [x] Decide the ML negative-case extension story — **`.ospo` + a leading
      `// osprey: flavor=ml` marker**, documented in `examples/README.md`
      §Must-reject fixtures. `.ospo` is already discovered by both harnesses
      and is excluded from every *source* harness, and
      `flavor_from_extension` returns `None` for it, so `resolve_flavor`'s
      marker branch selects ML with no extension/marker conflict. A bare
      `.ospml` in `failscompilation/` would be **invisible** to
      `crates/diff_examples.sh` (`find … -name '*.ospo'`). Zero harness edits
      were needed.
- [x] Make the in-process corpus test flavor-honest — `compile()` in
      `crates/osprey-cli/tests/examples_compile.rs` called
      `parse_program(source)`, hardwired to `Flavor::Default`, so an ML
      fixture would have been graded by the brace grammar and "rejected" for
      the wrong reason. It now threads the path through
      `parse_program_for_path`. New test
      `ml_flavor_negative_cases_are_rejected_by_the_ml_frontend` asserts not
      merely rejection but that the diagnostic carries the ML-specific
      fragment.
- [x] WASM harness runs portable examples with the feature-gap SKIP
      classification.

## VS Code extension (VSIX) — hard requirement — ✅ DONE

The built/published VSIX (`nimblesite.osprey`) ships full ML flavor support
(`vscode-extension/package.json`).

- [x] **ML language registered** — `osprey-ml` id, `.ospml` extension,
      distinct from `osprey`/`.osp`.
- [x] **ML TextMate grammar** — `syntaxes/osprey-ml.tmLanguage.json`
      (`scopeName: source.osprey-ml`).
- [x] **Layout-aware language configuration** — `language-configuration-ml.json`
      (no `{}` auto-pairing; layout `onEnter`).
- [x] **ML snippets** — `snippets/osprey-ml.json`.
- [x] **Commands include the ML flavor** — run/compile/check gated on
      `resourceLangId == osprey || osprey-ml`.
- [x] **Shipped in the VSIX** — all of the above bundled and packaged.

## Phase 6 — Tooling — mostly done

TODO:

- [x] VS Code ML editor support (grammar, layout config, snippets, command
      wiring — see the VSIX section above).
- [x] Formatter formats within a flavor (`osprey-fmt` is flavor-neutral, text
      based; the corpus round-trips both flavors).
- [x] LSP: hover/completion/signature help rendered in the **authoring**
      flavor — **done**, specified as `[LSP-FLAVOR-RENDER]` (spec 0020) and
      implemented in `osprey-lsp/src/mlrender.rs` (pure, total string
      functions) applied across `symbol_hover`, `signature_help` and
      `completion`. Hover fences as `osprey-ml` (its own TextMate grammar) and
      respells `fn inc(x: int) -> int` as `inc : int -> int`; declaration
      binders juxtapose (`type Box T`) while function binders stay bracketed
      (`pick<T> : T -> T -> T`); function-typed parameters keep balanced arrows
      (`map : (int -> int) -> int -> int`). Keyword completion is now
      flavor-partitioned: ML genuinely has **no `fn`, `let` or `if`**
      (`ml/token.rs` `keyword_or_ident`), so those are no longer offered — they
      lexed as plain identifiers — and every remaining snippet expands to
      layout, not braces. Pinned by
      `an_ml_document_is_answered_in_the_ml_flavor_end_to_end`,
      `ml_completions_never_offer_a_keyword_the_ml_frontend_does_not_have`, and
      the `mlrender` unit tests.
      Still open, deliberately: completion *around* effect operations and
      handler arms is position-insensitive (the list is whole-document), and
      signature help does not yet show partial application for curried calls.
- [ ] Optional `osprey convert` to transliterate Default ⇄ ML (separate from
      the formatter). Not started; nice-to-have.

## Phase 7 — Docs — ✅ DONE

TODO:

- [x] Specs 0023/0024 mirror to the website
      (`website/src/spec/0023-languageflavors.md`,
      `0024-mlflavorsyntax.md`).
- [x] Flavor cross-reference notes on the specs that gained a second spelling
      (0003/0005/0007/0008/0017 carry `osprey-ml` code blocks).
- [x] `examples/README.md` documents the `.osp`/`.ospml` convention.

## Risks

- **ML lowerer must be its own exhaustive matcher.** The hand-written ML
  `lower.rs` converts the ML CST to canonical AST; it must produce only canonical
  nodes and never reuse the Default `Lowerer`'s `kind()` matching (whose wildcard
  arms on unknown kinds would silently corrupt the AST). (frontend-parse map)
- **Layout-lexer correctness.** Indentation tracking across tabs/spaces, blank
  lines, comments, trailing newlines, and bracket-suppressed layout is the
  hardest single piece; budget for it and cover the hand-written lexer with Rust
  unit tests. (frontend-parse map)
- **Currying conflation.** Default multi-param and ML curried functions must stay
  distinct in the AST; the golden non-equivalent bucket guards this. (types map)
- **Diagnostic hardcoding.** Existing fix messages assume Default spelling; ML
  needs its own fix wording behind the flavor-blind semantic code. (cli map)
- **Escape-hatch drift.** If the tree-sitter + `scanner.c` fallback is ever
  taken, it must remain a flavor-internal swap that produces the identical
  canonical AST; rely on the cross-flavor equivalence test to catch semantic
  drift. (frontend-parse map)

## Acceptance

- A `.ospml` program with curried functions, `=>` effect operations, first-class
  handlers, and `handle … do` compiles, runs, and matches its `.expectedoutput`
  byte-for-byte under `make test`.
- The equivalent-bucket golden tests prove Default explicit-curry ≡ ML curry and
  Default `handle … in` ≡ ML `handle … do` at the canonical AST.
- The non-equivalent-bucket golden tests prove Default multi-param ≢ ML curry.
- `grep` finds no flavor inspection in `osprey-types` or `osprey-codegen`.
- Every existing Default `.osp` example still passes unchanged.

## References

The parsing techniques behind the hand-written ML frontend — recursive-descent /
predictive parsing, the Pratt (precedence-climbing) expression layer, and the
offside-rule layout lexer — are cited with verified sources in
[spec 0024 References](../specs/0024-MLFlavorSyntax.md#references).
