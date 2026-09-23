# Plan 0013 — ML Flavor Frontend

**Status:** The ML frontend is implemented and passing its tests. The layout lexer,
recursive-descent parser, CST, and lowerer are complete
(`crates/osprey-syntax/src/ml/`); flavor selection (flag > marker > extension)
works; **78 `.ospml` tested twins** run byte-identically to their `.osp`
counterparts (including effects, handlers, and `resume`); cross-flavor
AST- and IR-equivalence tests pass
(`crates/osprey-cli/tests/cross_flavor_{equiv,ir_equiv}.rs`); the VSIX ships ML
support (`osprey-ml` language, TextMate grammar, layout config,
snippets); specs 0023/0024 mirror to the website; **15 ML must-reject
fixtures** cover the frontend's rejection paths; and the **LSP now
answers in the authoring flavor** (`[LSP-FLAVOR-RENDER]`, spec 0020) with one
shared `[FLAVOR-SELECT]` precedence chain. Effects delivery is owned by
[plan 0016](0016-algebraic-effects-and-handlers.md), including the callable-handler
prototype. The optional `osprey convert` transliterator remains in this plan.

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
(a separate **CST→AST** step). The lexer derives
layout markers (`Indent`/`Dedent`/`Newline`) from the **offside rule**
(Landin 1966) via an explicit indentation stack, with bracket depth suppressing
layout inside parentheses. This **supersedes** the earlier plan of a
`tree-sitter-osprey-ml` grammar with an external C scanner. Rationale: the
offside rule uses an explicit indent stack in safe Rust; the implementation is
panic-free / `Result`-returning and unit-testable (project rules), with
no `unsafe` C and no codegen-tool build dependency. Per
[`[FLAVOR-BOUNDARY]`](../specs/0023-LanguageFlavors.md#the-one-law) the parser
**mechanism** is a below-the-AST, flavor-internal concern, so this swap does not
change the architecture (many CSTs, one AST). The tree-sitter + `scanner.c`
approach is retained as a documented fallback in Phase 2.

**Current state.** Phases 1–5 (flavor seam, frontend, selection, and tests) are
implemented; 78 `.ospml` twins pass under the differential harness and 15 ML
must-reject fixtures cover rejection paths. Phase 6/7 tooling and docs are done
except for the optional `osprey convert` transliterator. Handler values and other effects work are tracked only in
[plan 0016](0016-algebraic-effects-and-handlers.md).
## Implementation scope

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
  (lambdas-as-values: [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-FN]`,
  plan 0002 retired). The
  ML lowerer does the currying desugar; the checker and codegen are untouched.

This plan owns the layout frontend and its shared-AST boundary. The effects
plan owns changes to handler semantics and their implementation in both flavors.

## Architecture

| Stage | Today | After |
| --- | --- | --- |
| entry | `parse_program(src)` (`osprey-syntax/src/lib.rs:37`) | `parse_program_with_flavor(src, flavor)`; `parse_program` = Default wrapper |
| parse | tree-sitter brace grammar (`tree-sitter-osprey/`) | + hand-written Rust layout lexer + recursive-descent parser (`osprey-syntax/src/ml/`); tree-sitter + `scanner.c` retained as fallback |
| lower | `Lowerer` (`lower.rs`/`expr.rs`) → `Program` | + ML `lower.rs`: ML CST → the same `Program` (the parser builds the CST) |
| select | n/a | CLI flag > marker > extension > Default (`osprey-cli/src/main.rs:119`/`:200`) |
| check/codegen | `Program`-only, flavor-blind | **unchanged** |

## What is left (detailed)

The optional `osprey convert` transliterator remains (§Phase 6). All effects
work, including ML handler values and their conformance tests, is consolidated
in [plan 0016](0016-algebraic-effects-and-handlers.md).

## Phase 0 — Effects work moved to plan 0016

[FLAVOR-HANDLER-VALUE](../specs/0023-LanguageFlavors.md#shared-core-additions)
links the shared contract. Callable handlers already have a both-flavor
prototype; this frontend plan does not prescribe a separate `Handler` type or
installation mechanism.

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
**concrete syntax tree (CST)**; `lower.rs` converts it to canonical
`osprey_ast::Program` in a separate **CST→AST** step. The tree-sitter/scanner
alternative remains the documented fallback below.

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

> **Fallback.** If the hand-written layout frontend cannot maintain the required
> parser behavior, use a `tree-sitter-osprey-ml` grammar with an external
> `INDENT`/`DEDENT`/`NEWLINE` `scanner.c` (an indentation-stack scanner the brace
> grammar has never needed — `tree-sitter-osprey/` ships no `scanner.c` today),
> a tree-sitter grammar for the ML rules, a tree-sitter corpus test suite, and a
> separate `MlLowerer`. The boundary law
> ([`[FLAVOR-BOUNDARY]`](../specs/0023-LanguageFlavors.md#the-one-law)) makes the
> parser mechanism a flavor-internal swap that leaves the AST and everything
> above it untouched.

## Phase 3 — ML lowerer (CST → canonical AST) — ✅ DONE

Obeys the [lowering contract](../specs/0023-LanguageFlavors.md#the-lowering-contract).

TODO:

- [x] ML `lower.rs` producing `osprey_ast::Program`; spans + doc comments
      preserved.
- [x] Bindings: `x = e` → `Let{false}`; `mut x = e` → `Let{true}`; `x := e` →
      `Assignment`.
- [x] **Currying desugar** ([FLAVOR-CURRY](../specs/0023-LanguageFlavors.md#currying-canonicalisation));
      equals Default explicit-curry AST, differs from Default multi-param
      (pinned by `cross_flavor_equiv.rs`).
- [x] Effects: layout declarations, handlers and operations lower to the shared
      AST. Advanced effects syntax and conformance are tracked only in
      [plan 0016](0016-algebraic-effects-and-handlers.md).
- [x] Match: layout arms → `Match`/`MatchArm`; `Success value` →
      `Constructor { fields:["value"] }`.
- [x] Records: layout block → `TypeConstructor`; layout update → `Update`.
- [x] Diagnostics with flavor-aware fix wording.

## Phase 4 — Flavor selection wiring

Implemented.

TODO:

- [x] CLI `--flavor default|ml` on `Cli` (`osprey-cli/src/main.rs:34`), parsed in
      `parse_args` (`:119`); update `USAGE` (`:25`).
- [x] File marker `// osprey: flavor=ml` via the `directive` parser (`:521`),
      read in `run` (`:200`) before parsing.
- [x] Extension detection: `.ospml` ⇒ ML, `.osp` ⇒ Default (`Path::extension`).
- [x] Precedence flag > marker > extension > Default; report an error rather
      than selecting a flavor when extension and marker disagree.
- [x] Diff harness (`crates/run_test_corpus.sh`) discovers `.ospml` **additively**
      and resolves flavor by extension; existing `.osp` discovery unchanged.
- [x] Optional `osprey.toml` `flavor` key — **landed** (this item's "deferred;
      not in the current precedence chain" text was stale). `ProjectConfig.flavor:
      Option<Flavor>` (`osprey-project/src/manifest.rs`) is parsed by the
      `("project", "flavor")` arm through `parse_flavor`, and
      `osprey-project/src/lib.rs` threads it into `parse_sources(paths,
      config.flavor)` — so it is the project-scope fallback beneath the flag,
      marker and extension. Pinned by `parses_project_and_module_policy`
      (`manifest.rs`), which asserts `config.flavor == Some(Flavor::Ml)` from an
      `osprey.toml` carrying `flavor = "ml"`.
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

- [x] **78 `.ospml` tested twins** under `tests/regressions/**` with shared
      `.expectedoutput` goldens, covering currying/partial application, `=>`
      effect operations, handlers, `resume`, layout match/records,
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
      deliberately does not lex ([FLAVOR-BOUNDARY]). Harness counts are
      `FC_REJECT` 68 → 73 and `FC_ESCAPE` unchanged at 11 =
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
      `crates/run_test_corpus.sh` (`find … -name '*.ospo'`). Zero harness edits
      were needed.
- [x] Make the in-process corpus test use the selected flavor — `compile()` in
      `crates/osprey-cli/tests/examples_compile.rs` called
      `parse_program(source)`, hardwired to `Flavor::Default`, so an ML
      fixture would have been graded by the brace grammar and "rejected" for
      the wrong reason. It now threads the path through
      `parse_program_for_path`. New test
      `ml_flavor_negative_cases_are_rejected_by_the_ml_frontend` asserts that
      the diagnostic carries the ML-specific fragment.
- [x] WASM harness runs portable examples with the feature-gap SKIP
      classification.

## VS Code extension (VSIX) — ✅ DONE

The built/published VSIX (`nimblesite.osprey`) ships ML flavor support
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
      the formatter). Not started; no other item depends on it.

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
  main parser-specific risk; cover the hand-written lexer with Rust unit tests.
  (frontend-parse map)
- **Currying conflation.** Default multi-param and ML curried functions must stay
  distinct in the AST; the golden non-equivalent bucket guards this. (types map)
- **Diagnostic hardcoding.** Existing fix messages assume Default spelling; ML
  needs its own fix wording behind the flavor-blind semantic code. (cli map)
- **Escape-hatch drift.** If the tree-sitter + `scanner.c` fallback is ever
  taken, it must remain a flavor-internal swap that produces the identical
  canonical AST; rely on the cross-flavor equivalence test to catch semantic
  drift. (frontend-parse map)

## Acceptance

- [x] ML curried functions, `=>` effect operations, callable `handler E`
      values and block-scoped `handle E` run against the same goldens as their
      Default twins. The replacement syntax is owned by plan 0016; both flavors
      reject the former `in`/`do` application forms.
- [x] The equivalent-bucket golden tests prove Default explicit-curry ≡ ML curry
      at the canonical AST.
- [x] The non-equivalent-bucket golden tests prove Default multi-param ≢ ML curry.
- [x] `grep` finds no flavor inspection in `osprey-types` or `osprey-codegen` —
      only explanatory comments, no `Flavor` branching.
- [x] Every existing Default `.osp` example still passes unchanged.
