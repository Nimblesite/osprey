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
absent and `run` is `false`. This contract covers extraction into the model,
not execution by the example harness.

## Declaration attachment `[DOC-ATTACH]`

A documentation comment attaches to the following declaration. Both flavors
attach docs to functions, `let`/`mut` bindings, types, effects, externs,
modules, and signatures, including declarations inside modules. Only these
declaration forms receive a documentation field.

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
the declaration signature or type ([LSP-HOVER-DOCS]). The `--docs` command
exports built-in documentation only; it does not export user declarations.
