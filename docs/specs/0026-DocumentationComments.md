# Documentation Comments

Osprey has a **single documentation system** with two idiomatic surface
spellings — one for each [language flavor](0023-LanguageFlavors.md). Both
spellings lower to the **same** structured `DocComment` attached to the
canonical AST, so the type checker, the LSP, and the doc exporter never see a
flavor difference ([FLAVOR-BOUNDARY]). A `///` comment in the Default flavor and
a `(** … *)` comment in the ML flavor that carry the same body produce a
byte-identical doc model and byte-identical rendered output.

This chapter is the authoritative contract for that system: the sigils, the body
markup, the recognised sections, executable examples, and the shared model.

- [The Shared Doc Model](#the-shared-doc-model)
- [Default-Flavor Sigil](#default-flavor-sigil)
- [ML-Flavor Sigil](#ml-flavor-sigil)
- [Body Markup](#body-markup)
- [Recognised Sections](#recognised-sections)
- [Executable Examples (Doctests)](#executable-examples-doctests)
- [Attachment Rules](#attachment-rules)
- [Export & Tooling](#export--tooling)
- [Worked Example — One Function, Both Flavors](#worked-example--one-function-both-flavors)
- [Status](#status)
- [References](#references)

## The Shared Doc Model

`[DOC-MODEL]` Both flavors lower into one structure on the canonical AST. It
replaces the earlier free-text `doc: Option<String>` field.

```rust
/// A structured documentation comment. BOTH flavors' surface syntaxes
/// (`///` / `//!` in Default, `(** … *)` in ML) lower into this one model;
/// the body markup is identical across flavors — only the sigil differs.
pub struct DocComment {
    /// First sentence / first paragraph — the one-line summary.
    pub summary: String,
    /// Full Markdown body EXCLUDING the recognised structured sections.
    /// `[Symbol]` intra-doc links are left unresolved here (a later pass
    /// resolves them against the symbol table).
    pub body: String,
    /// `# Parameters` bullets (name → Markdown text), in author order.
    pub params: Vec<(String, String)>,
    /// `# Returns`.
    pub returns: Option<String>,
    /// `# Raises` / `# Errors` — effect or error name → text. Aligns with
    /// Osprey's `!Effect` rows and `Result<T, E>` convention.
    pub raises: Vec<(String, String)>,
    /// Extracted ```osprey``` examples with optional expected output.
    pub examples: Vec<DocExample>,
    /// `# See also` — `[Symbol]` references and external links.
    pub see_also: Vec<String>,
    /// `# Since` — version introduced.
    pub since: Option<String>,
    /// `# Deprecated` — present ⇒ deprecated; text is the reason.
    pub deprecated: Option<String>,
    /// Module/inner (`//!`) authorship, optional.
    pub author: Option<String>,
    /// Whether this was an outer or an inner/module doc.
    pub scope: DocScope,
}

pub struct DocExample {
    /// The Osprey snippet, compiled under the enclosing file's flavor.
    pub code: String,
    /// Expected stdout for the golden harness, when an `output` block follows.
    pub expected_output: Option<String>,
    /// Compile-only (no run) for type-level examples.
    pub run: bool,
}

pub enum DocScope { Outer, Inner }
```

`params`, `raises`, and `see_also` are ordered vectors, not maps: rendering
order is the author's order. `body` keeps `[Symbol]` links unresolved so the
doc-body parser stays flavor- and symbol-table-agnostic.

## Default-Flavor Sigil

`[DOC-SIGIL-DEFAULT]` The Default flavor uses **`///`** for an *outer* doc
comment (documents the declaration that follows) and **`//!`** for an *inner*
doc comment (documents the enclosing module or file).

```osprey
/// Doubles its argument.
fn double(x) = x * 2

//! Arithmetic helpers used across the billing pipeline.
module Billing { … }
```

`///` out-prioritises the ordinary `//` line comment by maximal munch, so an
ordinary comment is never mistaken for a doc comment. A blank `///` line is a
paragraph break in the body.

## ML-Flavor Sigil

`[DOC-SIGIL-ML]` The ML flavor uses **`(** … *)`** — the OCaml/odoc convention,
where the doc sigil is a specialisation of the ordinary `(* … *)` block
comment.

```osprey
(** Doubles its argument. *)
double x = x * 2
```

Disambiguation from ordinary comments and ASCII-art banners:

- `(**` opens a doc comment **only** when it is followed by content that is not
  immediately another `*` or the close. This matches odoc.
- `(*` not followed by `*` is an ordinary comment.
- The empty `(**)` and all-star banners such as `(*****)` are ordinary
  comments, never docs.
- `(* … *)` block comments **nest**; a doc comment nests the same way, so a
  commented-out region inside a doc closes correctly.

The ML flavor's ordinary comment forms remain `//` (line) and `(* … *)` (block).

## Body Markup

`[DOC-BODY-MARKDOWN]` The doc **body is CommonMark Markdown, identical in both
flavors**. Only the surrounding sigil differs; everything inside is the same
markup, parsed by one flavor-neutral parser and rendered by one backend.

Two additions on top of plain CommonMark, both shared by both flavors:

- **Intra-doc links** `[DOC-LINK]` — `[Symbol]` with no URL resolves against
  Osprey's symbol table (the rustdoc convention). Bare (`[helper]`) and dotted
  (`[Console.emit]`, resolving to the owner) forms are supported; a
  `[text](url)` Markdown link is left alone. In the editor, hovering a
  `[Symbol]` link shows the referenced element's own hover. Inline code that is
  *not* a link uses backticks.
- **Example fences** — a ` ```osprey ` fenced block is an executable example
  (see [Doctests](#executable-examples-doctests)); an optional following
  ` ```output ` fence is its expected stdout.

## Recognised Sections

`[DOC-SECTIONS]` Structure is **convention-based**, not tag-based: a small set
of recognised Markdown `#` headings lower into the model's fields; everything
else is free body. This keeps the body a clean Markdown document with no tag
sigil, and degrades gracefully — an unrecognised heading is just prose.

| Heading (case-insensitive) | Lowers to | Form |
|---|---|---|
| `# Parameters` | `params` | `- name: description` bullets |
| `# Returns` | `returns` | prose |
| `# Raises` / `# Errors` | `raises` | `- Name: description` bullets |
| `# Examples` | `examples` | ` ```osprey ` / ` ```output ` fences |
| `# See also` | `see_also` | `[Symbol]` links / URLs |
| `# Since` | `since` | a version string |
| `# Deprecated` | `deprecated` | reason prose |

As a concession to ML muscle memory, the ocamldoc/Javadoc block tags
`@param name`, `@return`, `@raise Name`, `@see`, `@since`, and `@deprecated` are
accepted as **aliases** that lower into the same fields. The heading convention
is the canonical, documented form; the tags exist so an OCaml/F# author is never
surprised.

## Executable Examples (Doctests)

`[DOC-DOCTEST-HARNESS]` A ` ```osprey ` fenced block in a doc comment is an
**executable example**. An optional ` ```output ` fence immediately after it is
the example's expected stdout.

Doctests are extracted into the **existing differential golden harness**
([crates/diff_examples.sh](../../crates/diff_examples.sh)) — there is no second
test runner. The extractor emits each example as a synthetic `.osp`/`.ospml`
program (compiled under the enclosing file's flavor) plus a `.expectedoutput`
from the `output` fence, and the harness compiles, runs, and byte-compares as it
does for every `examples/tested/` program. A doc example whose output drifts
fails CI, which is exactly the obsolescence defense the design calls for
[Uddin & Robillard 2015; Wrenn & Krishnamurthi 2019].

An example with no `output` fence is compiled but not run (a type-level or
illustrative snippet); `DocExample.run` records which.

## Attachment Rules

`[DOC-ATTACH]` A doc comment documents the declaration **immediately following**
it. Attachment is **before-only** in both flavors — Osprey does not adopt
Haddock's attach-after `-- ^` form, which doubles the parsing surface and
creates "which declaration owns this?" ambiguity. Every documentable
declaration is covered:

- functions, `let`/`mut` bindings, `type` declarations, `effect` declarations,
  `extern` declarations, and modules;
- and, granularly, type variants, record fields, effect operations, and
  function parameters (the last also expressible via `# Parameters`).

A doc comment separated from its declaration by a blank line still attaches; a
doc comment with no following declaration is a warning (a dangling doc).

## Export & Tooling

`[DOC-EXPORT]` The doc system feeds three consumers from the one model:

- **LSP hover** renders the `DocComment` as Markdown beneath the signature
  ([0020-LanguageServerAndEditors.md](0020-LanguageServerAndEditors.md)
  `[LSP-HOVER-DOCS]`).
- **`osprey --docs --docs-dir <dir>`** exports Markdown-with-front-matter pages.
  This already exports built-in functions from the same render model
  (`BuiltinDocView`); the user-declaration path emits the same page shape from
  `DocComment`, so built-in and user docs are visually uniform.
- **The website `generate-docs` pipeline** consumes that Markdown unchanged.

Built-in documentation and user documentation therefore share one render model
and one exporter — the `BuiltinDocView` shape and `DocComment` converge on the
same rendered page.

## Worked Example — One Function, Both Flavors

The same function, documented in each flavor, lowering to the **identical**
`DocComment`.

**Default flavor (`.osp`):**

```osprey
/// Divides `numerator` by `denominator`, rounding toward zero.
///
/// Use [safeDivide] when a zero denominator should be a recoverable
/// `Result` rather than a raised effect.
///
/// # Parameters
/// - numerator: The value being divided.
/// - denominator: The divisor. Must be non-zero.
///
/// # Returns
/// The integer quotient.
///
/// # Raises
/// - DivByZero: When `denominator` is `0`.
///
/// # Examples
/// ```osprey
/// print(divide(7, 2))
/// ```
/// ```output
/// 3
/// ```
///
/// # Since
/// 0.4.0
fn divide(numerator, denominator) = intDiv(numerator, denominator)
```

**ML flavor (`.ospml`):**

```osprey
(** Divides [numerator] by [denominator], rounding toward zero.

    Use [safeDivide] when a zero denominator should be a recoverable
    [Result] rather than a raised effect.

    # Parameters
    - numerator: The value being divided.
    - denominator: The divisor. Must be non-zero.

    # Returns
    The integer quotient.

    # Raises
    - DivByZero: When [denominator] is [0].

    # Examples
    ```osprey
    print (divide 7 2)
    ```
    ```output
    3
    ```

    # Since
    0.4.0 *)
divide numerator denominator = intDiv (numerator, denominator)
```

Both lower to:

```rust
DocComment {
    summary: "Divides `numerator` by `denominator`, rounding toward zero.",
    body: "Use [safeDivide] when a zero denominator should be a recoverable …",
    params: [("numerator", "The value being divided."),
             ("denominator", "The divisor. Must be non-zero.")],
    returns: Some("The integer quotient."),
    raises: [("DivByZero", "When `denominator` is `0`.")],
    examples: [DocExample { code: "print(divide(7, 2))",
                            expected_output: Some("3\n"), run: true }],
    since: Some("0.4.0"),
    ..
}
```

The two surfaces differ only in the sigil (`///` per line vs one `(** … *)`
block) and in idiomatic inline-code habit — each reads native to its family,
and the lowered model is the same.

## Status

**Implemented — capture, model, hover, and links are live; doctest-harness
wiring and user-declaration `--docs` export remain.**

Shipped and tested:

- The structured `DocComment` / `DocExample` / `DocScope` model
  ([crates/osprey-ast/src/doc.rs](../../crates/osprey-ast/src/doc.rs)), on the
  `doc` field of **every** declaration form (`fn`, `let`/`mut`, `type`,
  `effect`, `extern`, `module`).
- The **shared flavor-neutral body parser**
  ([crates/osprey-syntax/src/docparse.rs](../../crates/osprey-syntax/src/docparse.rs)):
  summary/body split, recognised `#` sections, `@tag` aliases, `[Symbol]` link
  extraction, and ```osprey```/```output``` doctest extraction. One parser,
  both flavors.
- **Default `///` + `//!`** lowering for all six declaration forms
  ([default/lower.rs](../../crates/osprey-syntax/src/default/lower.rs)).
- **ML `(** … *)`** lexing (nesting, banner/empty disambiguation), CST/parser
  threading, and lowering
  ([ml/lexer.rs](../../crates/osprey-syntax/src/ml/lexer.rs),
  [ml/lower.rs](../../crates/osprey-syntax/src/ml/lower.rs)) — this **closes the
  `[FLAVOR-LOWER-CONTRACT]` breach**: the ML flavor previously dropped every
  comment and hardcoded `doc: None`.
- **Cross-flavor equivalence** is machine-checked: a `///` Default function and
  its `(** *)` ML twin lower to an identical `DocComment`
  (`crates/osprey-cli/tests/cross_flavor_equiv.rs`).
- **LSP hover** renders the structured `DocComment` (summary, body, sections)
  for every declaration kind in both flavors, and a **`[Symbol]` intra-doc link
  under the cursor hovers to the referenced element** — bare (`[helper]`) and
  dotted (`[Console.emit]`) ([crates/osprey-lsp/src/features.rs](../../crates/osprey-lsp/src/features.rs)).

Remaining (tracked in [plan 0018](../plans/0018-documentation-comments.md)
Phase 3):

- Doctest **execution**: the extractor populates `DocExample`s; wiring them into
  the golden harness so their output is byte-checked is not yet done.
- `osprey --docs` exporting **user** declarations (it exports builtins today).
- `//!` module-scope grammar in the Default tree-sitter grammar (the lexer and
  lowerer already recognise `//!`; adding the grammar attachment point is the
  remainder).

## References

Full academic grounding for the design decisions above. Citations verified
against primary bibliographic records; where a detail is unverifiable it is
flagged rather than invented.

**Literate programming — the lineage.**

- Knuth, D. E. (1984). "Literate Programming." *The Computer Journal*, 27(2),
  97–111. DOI: [10.1093/comjnl/27.2.97](https://doi.org/10.1093/comjnl/27.2.97).
  The founding thesis: programs are literature addressed to humans; code and
  explanatory prose are interwoven at the source.
- Knuth, D. E. (1983). *The WEB System of Structured Documentation* (Stanford
  CS Report STAN-CS-83-980). The tangle/weave architecture and automatic
  cross-referencing — the concrete system behind the 1984 paper.
- Ramsey, N. (1994). "Literate Programming Simplified." *IEEE Software*, 11(5),
  97–105. DOI: [10.1109/52.311070](https://doi.org/10.1109/52.311070).
  Argues for a minimal, language-independent documentation mechanism (the
  *noweb* tool) — the principle behind one shared model across flavors.

**API documentation — design and empirical evidence.**

- Kramer, D. (1999). "API documentation from source code comments: a case study
  of Javadoc." *Proc. SIGDOC '99*, 147–153. ACM. DOI:
  [10.1145/318372.318577](https://doi.org/10.1145/318372.318577). The design
  rationale for authoring API docs as structured, co-located source comments.
- Robillard, M. P. (2009). "What Makes APIs Hard to Learn? Answers from
  Developers." *IEEE Software*, 26(6), 27–34. DOI:
  [10.1109/MS.2009.193](https://doi.org/10.1109/MS.2009.193). The dominant
  obstacle to learning an API is missing examples and missing intent — the
  evidence for first-class example and rationale sections.
- Robillard, M. P., & DeLine, R. (2011). "A field study of API learning
  obstacles." *Empirical Software Engineering*, 16(6), 703–732. DOI:
  [10.1007/s10664-010-9150-8](https://doi.org/10.1007/s10664-010-9150-8). The
  journal-length evidence base for documenting intent, examples, and task→API
  mappings.
- Uddin, G., & Robillard, M. P. (2015). "How API Documentation Fails." *IEEE
  Software*, 32(4), 68–75. DOI:
  [10.1109/MS.2014.80](https://doi.org/10.1109/MS.2014.80). A taxonomy of
  documentation failure modes — incompleteness, ambiguity, incorrectness,
  obsolescence, fragmentation — that the structured sections and doctests are
  designed to prevent.

**Executable / semantically-integrated documentation.**

- Wrenn, J., & Krishnamurthi, S. (2019). "Executable Examples for Programming
  Problem Comprehension." *Proc. ICER '19*, 131–139. ACM. DOI:
  [10.1145/3291279.3339416](https://doi.org/10.1145/3291279.3339416). Empirical
  support that machine-checked examples improve comprehension — the case for
  doctests.
- Flatt, M., Barzilay, E., & Findler, R. B. (2009). "Scribble: closing the book
  on ad hoc documentation tools." *Proc. ICFP '09*, 109–120. ACM. DOI:
  [10.1145/1596550.1596569](https://doi.org/10.1145/1596550.1596569). The
  definitive argument for binding-aware documentation where cross-references are
  resolved and examples run — the basis for `[Symbol]` intra-doc links and
  executable examples.

**Comments and program comprehension.**

- Woodfield, S. N., Dunsmore, H. E., & Shen, V. Y. (1981). "The effect of
  modularization and comments on program comprehension." *Proc. ICSE '81*,
  215–223. IEEE Press. (Pre-DOI proceedings; cite by pages.) The classic
  controlled experiment showing comments causally improve comprehension.
- Tenny, T. (1988). "Program readability: procedures versus comments." *IEEE
  Transactions on Software Engineering*, 14(9), 1271–1279. DOI:
  [10.1109/32.6171](https://doi.org/10.1109/32.6171). Comments carry real
  comprehension weight, complementary to structure.
- Steidl, D., Hummel, B., & Jürgens, E. (2013). "Quality analysis of source code
  comments." *Proc. ICPC '13*, 83–92. IEEE. DOI:
  [10.1109/ICPC.2013.6613836](https://doi.org/10.1109/ICPC.2013.6613836). A
  machine-assessable model of comment quality — motivation for a precise model
  over undefined "good docs."

**A note on Markdown.** Osprey's doc body is CommonMark. Markdown has no
seminal peer-reviewed source; its lineage is Gruber's 2004 informal
specification and the later CommonMark specification, cited here as **informal
specifications**, explicitly distinct from the peer-reviewed sources above. The
*executable* and *binding-aware* aspects of the design rest on the scholarly
sources (Wrenn & Krishnamurthi; Flatt, Barzilay & Findler), not on Markdown
itself.
