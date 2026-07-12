# Plan 0018 — Documentation Comments (Both Flavors)

**Subsystem:** `crates/osprey-ast` + `crates/osprey-syntax` (default + ml) +
`crates/osprey-lsp` + `crates/osprey-cli` + `crates/diff_examples.sh`
**Status:** Phases 1–2 done (structured model, both flavors capture docs on all
six declaration forms, hover renders them, `[Symbol]` links hover). Phase 3
(doctest execution + user-declaration `--docs` export) remains.
**Spec:** [0026-DocumentationComments.md](../specs/0026-DocumentationComments.md)
(`[DOC-*]`); LSP hover [0020](../specs/0020-LanguageServerAndEditors.md)
`[LSP-HOVER-DOCS]`; lowering contract [0023](../specs/0023-LanguageFlavors.md)
`[FLAVOR-LOWER-CONTRACT]`.

## Summary

Osprey has one documentation system with two idiomatic sigils — `///` (Default)
and `(** … *)` (ML) — lowering to one structured `DocComment` on the canonical
AST, rendered by the LSP, exported by `--docs`, and with executable examples run
through the existing golden harness. Today only a fragment exists.

## What works today

- **Default `///` lexing**: `tree-sitter-osprey/grammar.js` `doc_comment` /
  `_doc_comment_line` (`token(prec(1, seq('///', …)))`) attaches to all six
  declaration forms (`let`, `function`, `extern`, `type`, `effect`, `module`).
- **Extraction**: `crates/osprey-syntax/src/default/lower.rs` `doc_text` /
  `strip_doc_line` produce clean joined prose.
- **AST carries raw doc on two forms**: `Stmt::Function.doc` and `Stmt::Let.doc`
  are `Option<String>` (`crates/osprey-ast/src/lib.rs`).
- **LSP hover** renders `SymbolInfo.doc` beneath the signature
  (`crates/osprey-lsp/src/features.rs` `symbol_hover`).
- **`--docs` exporter**: `crates/osprey-cli/src/docs.rs` writes
  Markdown-with-front-matter — but from `BuiltinDocView` only (builtins), taking
  no source file.
- **Render model precedent**: `BuiltinDocView` (`crates/osprey-types/src/builtin_docs.rs`)
  — signature + summary + typed params + return + example — is the shape
  `DocComment` rendering converges on.

## Where it stops

1. **No structured model.** `doc` is free text; there are no params/returns/
   raises/examples/since fields, so nothing machine-extracts "what does param
   `x` mean" or runs an example.
2. **Default drops docs on four declaration forms.** `doc_text` is only called
   for `fn`/`let`; a `///` above a `type`/`effect`/`extern`/`module` parses and
   is then silently discarded at lowering (and the AST has nowhere to put it).
3. **The ML flavor captures no docs at all.** The ML lexer treats every comment
   (`//`, `(* *)`) as trivia; `ml/lower.rs` hardcodes `doc: None`. This
   **violates** `[FLAVOR-LOWER-CONTRACT]`'s "preserve documentation comments"
   requirement.
4. **No doctests.** Doc examples are inert; nothing compiles or runs them.
5. **`--docs` exports no user declarations** and takes no source file.

## Implementation plan (phased, each phase CI-green)

### Phase 1 — Shared model + Default full coverage

1. Add `DocComment`, `DocExample`, `DocScope` to `crates/osprey-ast` (new
   `doc.rs` to stay < 500 LOC). Keep it a plain data type; parsing lives in
   `osprey-syntax`.
2. Add `doc: Option<DocComment>` to `Stmt::Type`, `Stmt::Effect`, `Stmt::Extern`,
   `Stmt::Module` (and `TypeVariant`/`TypeField`/`EffectOperation`/`Parameter`
   for granular docs); migrate `Stmt::Function`/`Stmt::Let` from
   `Option<String>`.
3. A **flavor-neutral doc-body parser** (new shared module, mirroring the
   `strings.rs` shared pattern): raw doc text → `DocComment` (summary split,
   recognised `#` sections, `@tag` aliases, `[Symbol]` links left unresolved,
   ` ```osprey ```/```output ``` extraction). One parser, both flavors.
4. Call it from the Default lowerer for **all six** declaration forms.
5. LSP `sym_of`/`decl_sym` stop hardcoding `None` for type/effect/extern; render
   the structured model in hover.

### Phase 2 — ML sigil

6. ML lexer: specialise the existing nesting `(* *)` scanner
   (`skip_block_comment`) so `(**`-opened blocks are **retained** as a
   `DocComment` token instead of discarded, with the banner/empty
   disambiguation from `[DOC-SIGIL-ML]`.
7. Thread the token through `ml/cst.rs` + `ml/parser.rs` and attach it to the
   following declaration.
8. `ml/lower.rs` stops hardcoding `doc: None`; runs the **same** Phase-1 body
   parser. Closes the `[FLAVOR-LOWER-CONTRACT]` breach.
9. Cross-flavor equivalence: a `///`-documented `.osp` and its `(** *)` twin
   `.ospml` lower to the same `DocComment` (extend
   `cross_flavor_equiv.rs`).

### Phase 3 — Doctests + user-doc export

10. Doctest extraction pre-pass for `crates/diff_examples.sh`: emit
    `.osp`/`.ospml` + `.expectedoutput` from each `DocExample` (compiled under
    the file's flavor), run by the existing harness.
11. `osprey --docs` accepts a source file and emits user-declaration pages from
    `DocComment`, reusing the `docs.rs` Markdown-front-matter emitter and the
    `BuiltinDocView` page shape.
12. `//!` inner/module docs in the Default grammar (mirror the `prec(1)` trick).

## Testing

- Unit: the ML lexer retains `(** *)` and rejects unterminated; the shared body
  parser lowers each recognised section and each `@tag` alias; banner/empty
  `(**)` stay ordinary comments.
- Cross-flavor: `///` and `(** *)` twins → identical `DocComment`.
- Examples: expand `function_composition_test.osp` / an ML twin with a fully
  documented declaration; a doctest whose `output` fence matches.
- failscompilation: a dangling doc (doc with no following declaration) warns; a
  doctest whose output drifts fails the harness.

## Risks / considerations

- **`Option<String>` → `Option<DocComment>` is a breaking AST change** touching
  every `doc:` construction site; do Phase 1 atomically.
- **`(**` vs banner disambiguation** must exactly match odoc so ASCII banners
  don't become docs — pin with lexer tests.
- **Doctest flavor inheritance**: a snippet compiles under the enclosing file's
  flavor (`// osprey: flavor=` / extension) — the extractor must carry it.
- Keep one body parser for both flavors — duplicating it would reintroduce the
  boundary-law violation the whole design avoids.

## TODO

- [x] Phase 1: `DocComment`/`DocExample`/`DocScope` model + AST fields on all
      six declaration forms; shared body parser (sections, `@tag` aliases,
      `[Symbol]` links, doctest extraction); Default lowering for all six forms;
      LSP hover renders the structured model.
- [x] Phase 2: ML `(** *)` lexing (nesting + banner/empty disambiguation) /
      CST / parser / lowering via the shared parser; cross-flavor `DocComment`
      equivalence test; **closes `[FLAVOR-LOWER-CONTRACT]`** (ML no longer drops
      docs).
- [x] `[Symbol]` intra-doc links hover to the referenced element (bare +
      dotted); the doc-link extractor (`docparse::doc_links`).
- [ ] Phase 3a: doctest **execution** — wire extracted `DocExample`s into the
      `diff_examples.sh` golden harness (compiled under the file's flavor,
      output byte-checked).
- [ ] Phase 3b: `osprey --docs` exports **user** declarations from
      `DocComment` (builtins already export from `BuiltinDocView`).
- [ ] Phase 3c: `//!` inner/module-scope grammar attachment in the tree-sitter
      grammar (lexer + lowerer already recognise `//!`).
- [x] `make ci` green (Phases 1–2).
