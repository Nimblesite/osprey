# Documentation Comments

Default `///` comments and ML `(** ... *)` comments lower to the same
structured documentation model. Equivalent comment bodies therefore have the
same representation after flavor lowering ([FLAVOR-BOUNDARY]).

## Shared model `[DOC-MODEL]`

`DocComment` stores:

- a one-line `summary` from the first paragraph and the remaining Markdown in
  `body`;
- ordered parameter, error, example, and related-link entries;
- optional return, version, deprecation, and author text; and
- the comment scope.

`DocExample` stores the Osprey source, optional expected output, and whether the
example is runnable. Both syntax flavors use this model.

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
indistinguishable from one that was never written.

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

## Example extraction `[DOC-DOCTEST-HARNESS]`

Inside a recognized examples section, each fenced block labeled `osprey`
becomes a `DocExample`. An immediately following fence labeled `output` supplies
`expected_output` and sets `run` to `true`; without one, `expected_output` is
absent and `run` is `false`.

`osprey <file-or-project> --doctests` validates the original sources, then checks
each example independently using the owning file's resolved flavor. A project
may contain both flavors and does not need an application entry for documentation.
The example has access to the documented declaration's lexical scope, including
private module helpers. Application entry statements and `main` are not run.
Bindings from one example never become visible to another.

Every example is type-checked, including declarations it does not call. An
example without an output fence is compile-only. A runnable example compiles
through the ordinary native or `wasm32` backend, must exit successfully, and
must produce the exact bytes in the output fence plus its final newline. An
empty output fence expects no stdout bytes. Spaces and blank lines are
significant. A failure names the source, declaration, and example ordinal;
the command exits unsuccessfully if any example fails.

Native examples support `--memory=default|gc|arc`; ARC execution additionally
requires exactly one zero-live-object exit sentinel. `wasm32` uses the default
allocator and `OSPREY_WASM_RUN` (default `wasmtime`) as its executable host.
Each runnable example has a 30-second execution limit, configurable through a
positive `OSPREY_DOCTEST_TIMEOUT_MS`. Compilation is outside this execution limit.

The existing corpus harness runs documentation examples in its native allocator
passes and its WASM pass. It rejects malformed result summaries and output
drift, and requires at least six successful examples across the two source flavors.

## Declaration attachment `[DOC-ATTACH]`

An outer documentation comment attaches to the following declaration. Both
flavors attach docs to functions, `let`/`mut` bindings, types, effects, externs,
modules, and signatures, including declarations inside modules. Only these
declaration forms receive a documentation field.

An inner comment attaches instead to the scope containing it — the file, a
namespace, or a module ([DOC-SIGIL-INNER]) — and is recorded separately from
that scope's outer comment.

Docs do not attach separately to variants, fields, or parameters.

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

## Markdown rendering `[DOC-EXPORT]`

`DocComment::render_markdown` emits the summary, body, and populated structured
sections in model order. LSP declaration hovers append this rendering beneath
the declaration signature or type ([LSP-HOVER-DOCS]). Authorship is rendered
after the version section when present.

`osprey --docs --docs-dir <directory>` exports the built-in reference. Supply
`--source <file-or-project>` or a positional source to include user APIs. The
exporter validates syntax, module contracts, and types before writing pages.
Files inherit the same flavor selection as compilation; projects select the
flavor per source. Library projects do not need an application entry.

User pages include file and namespace documentation, module documentation,
public functions and values, types, effects and their operations, externs, and
module signatures. Public declarations appear even without comments. Module
visibility is the compiler's finalized surface, including signature ascription;
private members and opaque type representations are excluded. Public type pages
show their representation, fields and variants. Function signatures come from
the editor's inferred type model and retain generic binders. ML source signatures
and example fences are presented in the ML flavor.

Markdown is the default format. Built-ins live under `functions/`, user APIs
under `api/`, and additional pages under `guides/`. Names are made safe and unique
on case-insensitive filesystems; `api/index.md` is reserved for the API listing.
Multiple contributions to a namespace share its page. File documentation keeps
the source's relative path identity. Regeneration removes obsolete pages recorded
in the format's manifest, while the historical built-in `functions/*.md` tree
remains wholly generated. Unrelated files outside that tree are preserved.
Duplicate output paths, malformed manifests, and symlinks inside the destination
are rejected before writing the output set.

### HTML sites `[DOC-EXPORT-HTML]`

`--docs-format html` produces a complete static site with a root `index.html`,
API and built-in pages, navigation, search, and local styles. It needs no website
framework, build step, CDN, or network connection. Navigation and search work
when opened directly from the filesystem or served from a static server, including
under a URL prefix. Layout adapts to mobile screens and keyboard navigation.

The renderer supports CommonMark plus tables, footnotes, task lists and
strikethrough. Raw HTML in Markdown is displayed as text; it cannot inject scripts
or markup into generated pages. Source code stays escaped. Internal Markdown
links point to generated HTML pages, and resolvable documentation symbol links
point to the matching API declaration.

### Additional pages `[DOC-EXPORT-PAGES]`

Repeat `--docs-page <file.md-or-directory>` to include authored Markdown. A
directory is scanned recursively for Markdown pages, preserving its relative
structure under `guides/`. A page's first level-one heading supplies its title.
Additional pages appear in navigation and search alongside generated APIs.
Missing inputs and conflicting page paths are errors.

### Themes and custom CSS `[DOC-EXPORT-CSS]`

HTML supports `--docs-theme osprey|midnight|paper`: a warm light theme, a dark
theme, and a minimal light theme. The themes share layout and CSS custom
properties so customization does not require replacing the renderer.

Repeat `--docs-css <file.css>` to copy custom stylesheets into the output site.
They are linked after the selected theme, in argument order. Stylesheets may
override theme properties and ordinary selectors. CSS and nondefault themes
require HTML output. Unknown themes, formats and options, and missing argument
values fail with a usage error instead of silently generating a different result.
