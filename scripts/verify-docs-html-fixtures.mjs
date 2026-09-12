// The documented inputs the browser gate asserts against ([DOC-EXPORT-HTML],
// [DOC-EXPORT-PAGES], [DOC-EXPORT-CSS]).
//
// They sit beside the assertions rather than among them, and are written rather
// than committed: they exist to be documented. A guide tree with spaces in its
// names and a project whose modules deliberately share a member name are inputs
// to keep honest, not results to keep reading.

const GETTING_STARTED = `# Getting Started

Read the [deep guide](<Deep Dive/Advanced Topics.md>).

| Platform | Command |
|---|---|
| macOS | \`brew install osprey\` |

- [x] download
- [ ] ~~configure by hand~~

> Blockquotes render.

\`\`\`osprey
fn double(x) = x * 2
\`\`\`

Raw HTML is shown, never run: <script>window.__PWNED__=1</script>
`;

const ADVANCED_TOPICS = `# Advanced Topics

Back to [the start](../Getting%20Started.md#install) and across to
[the notes](<Release Notes.md>).

This must not execute: [click me](javascript:window.__PWNED__=1).
`;

// A project whose modules deliberately share a member name, so scope-sensitive
// symbol resolution has something to get wrong.
const SYMBOL_LIBRARY = `(** Prices and the ledger that records them. *)
namespace shop

(** Money helpers. *)
module Money
    (** Convert cents. Uses [helper], names [Ledger.post] and [toString].
    \`total\` is defined here and in [Ledger], so [total] means this one.
    \`summarize\` is defined only in two unrelated modules, so [summarize]
    names nothing here and stays text. *)
    export parse : int -> int
    parse cents = cents

    (** Shared by this module only. *)
    export helper : int -> int
    helper x = x

    (** The running total. *)
    export total : int -> int
    total x = x

(** Reporting. *)
module Reports
    (** Summarise. *)
    export summarize : int -> int
    summarize x = x

(** Auditing. *)
module Audit
    (** Summarise. *)
    export summarize : int -> int
    summarize x = x

(** The ledger. *)
module Ledger
    (** Records an amount. Uses [helper]. *)
    export post : int -> int
    post amount = amount

    (** Shared by this module only. *)
    export helper : int -> int
    helper x = x

    (** The running total. *)
    export total : int -> int
    total x = x
`;

/** Every fixture file, keyed by its path inside the work directory. */
export const FIXTURES = {
  'guides/Getting Started.md': GETTING_STARTED,
  'guides/Deep Dive/Advanced Topics.md': ADVANCED_TOPICS,
  'guides/Deep Dive/Release Notes.md': '# Release Notes\n\nNotes about websocket support.\n',
  'brand.css': ':root { --accent: rgb(255, 0, 128); }\n',
  'override.css': 'body { background: rgb(1, 2, 3); }\n',
  'symbols/osprey.toml': '[project]\nname = "symbols"\nsource_roots = ["src"]\ndefault_namespace = "shop"\n',
  'symbols/src/lib.ospml': SYMBOL_LIBRARY,
};
