# Documentation Comments

Default `///` comments and ML `(** ... *)` comments lower to the same
structured documentation model. Equivalent comment bodies therefore have the
same representation after flavor lowering ([FLAVOR-BOUNDARY]).

Compiler and language rules are separate from integration tooling. See [Documentation tools](../tooling/DocumentationTools.md) for the extracted contract.

## Shared model `[DOC-MODEL]`

`DocComment` stores:

- a one-line `summary` from the first paragraph and the remaining Markdown in
  `body`;
- ordered parameter, error, example, and related-link entries;
- optional return, version, deprecation, and author text; and
- the comment scope.

`DocExample` stores the Osprey source, optional expected output, and whether the
example is runnable or requires native capabilities. Both syntax flavors use
this model.

## Default sigil `[DOC-SIGIL-DEFAULT]`

In Default files, consecutive `///` lines document the declaration that
follows. The marker and one optional following space are removed. A blank
`///` line separates paragraphs.

```osprey
/// Doubles its argument.
fn double(x) = x * 2
```

Ordinary `//` comments do not produce documentation.

## ML sigil `[DOC-SIGIL-ML]`

In ML files, `(** ... *)` documents the declaration that follows.

```osprey-ml
(** Doubles its argument. *)
double x = x * 2
```

ML block comments nest. `(**` starts documentation only when content follows
the opener; `(**)` and all-star banners such as `(*****)` remain ordinary
comments. An unterminated documentation comment is a syntax error.

Each prose line drops its leading whitespace and one optional `*` continuation marker. A line inside a fenced code block drops only the margin its opening fence carried, so an example keeps the indentation ML layout depends on.

## Python docstrings `[DOC-PYTHON-DOCSTRING-CLEAN]`

A Python-flavor docstring is cleaned as PEP 257 `inspect.cleandoc` cleans it:
the first line is trimmed, and every later line loses the indentation that all
later non-blank lines share. A fenced example inside an indented docstring keeps
only its own relative indentation (`python/documents.rs`, `clean_docstring`).

## Inner sigil `[DOC-SIGIL-INNER]`

`//!` documents the scope that encloses it rather than the declaration that
follows. It is spelled the same in both flavors: only the outer sigil differs.
Consecutive `//!` lines form one comment, and the marker plus one optional
following space is removed from each.

A `//!` block is legal as the first item of a file, of a `namespace` brace body,
or of a `module` body, and it lowers to that scope's own documentation. A
file-scoped `namespace name;` header opens no body, so the file's `//!` documents
the file.

```osprey
//! Payment primitives shared by the billing modules.

/// Converts cents to a display string.
fn format(cents) = "${cents}"
```

An outer and an inner comment describe the same scope from opposite sides, so a
scope may carry both at once and neither replaces the other.

```osprey
/// What callers need to know about the module.
module Ledger {
    //! What a maintainer reading the body needs to know.
    export let entries = []
}
```

`//!` written anywhere else documents nothing and is rejected. It is never
treated as an ordinary `//` comment: a discarded documentation comment is
indistinguishable from one that was never written. Nor is any of it read as
code — the text after the marker is comment text wherever the marker sits. Both
flavors reject it with the same error at the position of the `//!`, naming the
flavor's own outer sigil as the alternative: `` `//!` documents the enclosing
file, namespace or module; write it as the first item of one, or use `///` to
document the declaration that follows `` (`(** … *)` in ML).

## Body markup `[DOC-BODY-MARKDOWN]`

The stripped body is Markdown in both flavors and passes through one parser.
The first paragraph becomes a whitespace-normalized summary. Text after the
first blank line becomes the body, except for recognized sections and aliases
described below.

### Symbol links `[DOC-LINK]`

`[Name]` and dotted `[Owner.member]` forms are symbol links. Standard Markdown
links such as `[text](https://example.com)` are not symbol links. In LSP hover,
hovering a symbol link shows the referenced declaration; a dotted link resolves
to its owner declaration.

## Recognized sections `[DOC-SECTIONS]`

The following case-insensitive level-one headings populate structured fields:

| Heading | Field | Content |
|---|---|---|
| `# Parameters` / `# Params` | `params` | `- name: description` bullets |
| `# Returns` / `# Return` | `returns` | prose |
| `# Raises` / `# Errors` | `raises` | `- name: description` bullets |
| `# Examples` / `# Example` | `examples` | Osprey and output fences |
| `# See also` / `# See` | `see_also` | comma- or line-separated entries |
| `# Since` | `since` | prose |
| `# Deprecated` | `deprecated` | prose |

The aliases `@param`, `@return`, `@returns`, `@raise`, `@raises`, `@throws`,
`@see`, `@since`, `@deprecated`, and `@author` populate the same fields.
Unrecognized Markdown remains text in its current region.

## Declaration attachment `[DOC-ATTACH]`

An outer documentation comment attaches to the following declaration. Both
flavors attach docs to functions, `let`/`mut` bindings, types, effects, externs,
modules, and signatures, including declarations inside modules. Only these
declaration forms receive a documentation field.

An inner comment attaches instead to the scope containing it — the file, a
namespace, or a module ([DOC-SIGIL-INNER]) — and is recorded separately from
that scope's outer comment.

Docs do not attach separately to variants, fields, or parameters. A parameter
is documented in its function's `# Parameters` section, and hovering the
parameter shows that entry ([LSP-HOVER-PARAM-DOCS](../tooling/0020-LanguageServerAndEditors.md)).

### Exported declarations `[DOC-EXPORT-REQUIRED]`

An export is a package's public API, so it must be documented. In a package
(a project whose `osprey.toml` has a `[package]` table) `--check`, `--run`,
every build and the language server report `undocumented-export` as an
**error** that fails the build. An application's exports are its own, so an
application, or a file under no manifest, gets no report at all. Both
defaults are lint levels a project can change ([LINT-LEVELS]). The rule
covers:

- an `export`ed function, value, type, effect, extern, signature or module
  with no outer documentation comment (a module may use an inner `//!` comment
  instead), and
- a module that exports anything but has neither an outer nor an inner
  comment.

The warning names the declaration with its module path, for example
`error: exported function `Api::double` has no documentation comment`. A documented
type covers its variants and fields, which take no documentation of their
own. Private declarations are not reported.

### Exported examples `[DOC-EXPORT-EXAMPLE-REQUIRED]`

In a package, `--check` and the language server report `unexampled-export` at
information severity: the CLI labels it `info:` and the editor shows it as
information, not a warning. It is not counted as a warning, so `--check
--warnings-as-errors` still passes. An application gets no report. Both are
lint levels a project can change ([LINT-LEVELS]). The rule covers:

- a documented exported function, value or extern whose comment has no
  `# Examples` section, or whose section holds no example fence, and
- an operation of an exported effect whose documentation shows no example.

A type, module, namespace or signature is covered by the examples of the
members that use it, so it is not reported. An undocumented export is reported
once, as `undocumented-export`. The note names the declaration with its
module path, for example `exported function `Api::triple` has no example`, and
an operation as `Effect.operation` (`osprey-types/src/undocumented.rs`).

### Lint levels `[LINT-LEVELS]`

Every analyzer rule reports at a level, and a project sets the level of any
rule, by its rule id, in the `[lints]` table of its `osprey.toml`:

```toml
[lints]
undocumented-export = "warn"
unexampled-export = "deny"
closing-run = "allow"
```

| Level | CLI | Editor | Fails the build |
|---|---|---|---|
| `allow` | not reported | not reported | never |
| `info` | `info:` | information | never |
| `warn` | `warning:` | warning | only with `--warnings-as-errors` |
| `deny` | `error:` | error | always, even with `--no-warnings` |

A rule the table leaves out takes its default. `undocumented-export` is `deny`
in a package and `allow` in an application; `unexampled-export` is `info` in
a package and `allow` in an application; `module-deep-hierarchy` and
`namespace-reverse-domain` are `allow` in a package ([MODULES-STYLE]); every
other rule is `warn`. A
project is a package when its manifest has a `[package]` table, and a file
under no manifest is an application. A project's levels apply to all of its
sources, including sources it borrows from a package through
`source_roots`. The CLI and the editor read the same levels
(`osprey-types/src/lint_level.rs`).

The table names only analyzer rules, the ones `LINT_RULES` lists:
`undocumented-export`, `unexampled-export`, `redundant-annotation`,
`redundant-callback`, `repeated-literal`, `consecutive-print`, `closing-run`,
`unused-private`, `unused-variable`, `unused-parameter`,
`unused-pattern-binding`, `unused-handler-parameter`, `module-deep-hierarchy`,
`state-boundary`, `namespace-folder-drift`, `namespace-reverse-domain` and
`unused-import`. An unknown rule or level is a manifest error at its line, in
the compiler and in the package manager, so a misspelt rule never silently
keeps its default. Type errors, syntax errors and skipped tests are not lints:
no table can lower them.

### Test support `[DOC-EXPORT-EXAMPLE-TEST-SUPPORT]`

A source under a `tests` or `test` folder serves its suite, not readers of the
API: its exports still need documentation (`undocumented-export`), but never
an example, so `unexampled-export` is not reported there, in the CLI or the
editor. Generated oracle modules (`tests/expected.py`) are the main case: an
example per data constant would only restate the constant and add a doctest
build to every CI run (`osprey-types/src/source_warnings.rs`).

### Effect operations `[DOC-EFFECT-OP]`

An effect operation is the one nested form that carries its own documentation.
A `///` (Default) or `(** … *)` (ML) block written directly above an operation
line attaches to that operation, and both flavors lower it to the same
`EffectOperation::doc`. An operation documents ITSELF: the owning effect's doc
describes the capability as a whole and serves only as a fallback for an
undocumented operation. Without per-operation docs, sibling operations hovered
identically and said nothing about the operation under the cursor
([LSP-HOVER-EFFECT-OPERATIONS]).

The operation's recorded position anchors on its NAME, not on its declaration
node, so a leading doc block does not drag the position up onto the comment and
defeat position-based hover resolution.
