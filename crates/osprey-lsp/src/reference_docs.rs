//! Hover summaries for the names no source file declares.
//!
//! Two kinds of token reach the cursor with no declaration site to navigate to
//! and no `///` block to quote: the built-in **type names** written in
//! annotations (`int`, `Result`, …) and the reserved **keywords** of both
//! flavors (`match`, `handle`, `in`, …). The highlighter colours them the same
//! as ordinary keywords, so an author expects both to hover — yet neither is a
//! symbol, a built-in function or a parameter, so every declaration-driven
//! resolution path in [`crate::hover`] returns nothing for them. This module
//! owns their fixed reference tables and the two lookups over them.
//! Implements [LSP-HOVER-WRITTEN], [LSP-HOVER-KEYWORD].

use osprey_syntax::Flavor;

use crate::mlrender;

/// A written built-in **type name**'s one-line meaning. A declared type is
/// already a symbol and never reaches here; this covers the built-in
/// constructors, which have no declaration site to navigate to.
/// Implements [LSP-HOVER-WRITTEN].
pub(crate) fn type_hover(word: &str, flavor: Flavor) -> Option<String> {
    BUILTIN_TYPE_DOCS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(name, summary)| format!("{}\n\n{summary}", mlrender::fenced(flavor, name)))
}

/// A reserved **keyword**'s one-line meaning. Keywords are reserved, so one can
/// never be a declared symbol, a built-in, a parameter or a written type — none
/// of the declaration-driven paths can answer for `match`, `handle`, `in` or
/// any other keyword, and without this branch hovering one returns nothing at
/// all, even though the highlighter colours it exactly like the built-in types
/// that do hover. Implements [LSP-HOVER-KEYWORD].
pub(crate) fn keyword_hover(word: &str, flavor: Flavor) -> Option<String> {
    KEYWORD_DOCS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(name, summary)| format!("{}\n\n{summary}", mlrender::fenced(flavor, name)))
}

/// One-line summaries for the type names no source file declares.
const BUILTIN_TYPE_DOCS: [(&str, &str); 8] = [
    (osprey_types::names::INT, "The 64-bit integer primitive."),
    (osprey_types::names::FLOAT, "The floating-point primitive."),
    (osprey_types::names::STRING, "The string primitive."),
    (osprey_types::names::BOOL, "The boolean primitive."),
    (
        osprey_types::names::UNIT,
        "The type of an expression with no meaningful value.",
    ),
    (
        osprey_types::names::RESULT,
        "`Result<ok, err>` — `Success { value }` or `Error { message }`.",
    ),
    (
        osprey_types::names::LIST,
        "`List<elem>` — a persistent list.",
    ),
    (
        osprey_types::names::MAP,
        "`Map<key, value>` — a persistent map.",
    ),
];

/// One-line meanings for the reserved keywords of both flavors, drawn from
/// `ml::token::keyword_or_ident` and the Default grammar. Reserved words are the
/// same across flavors even though `fn`/`let`/`if`/`else` have no ML spelling,
/// so a single table serves both — the fence language it renders in still
/// follows the document.
const KEYWORD_DOCS: [(&str, &str); 30] = [
    ("fn", "Declares a function: `fn name(params) = body`."),
    ("let", "Binds an immutable value: `let name = value`."),
    ("mut", "Binds a reassignable variable: `mut name = value`."),
    ("if", "Conditional expression: `if cond { … } else { … }`."),
    ("else", "The alternative branch of an `if` expression."),
    (
        "match",
        "Pattern-matches a value against its variants, checked for exhaustiveness.",
    ),
    (
        "type",
        "Declares a type — a record or a tagged union of variants.",
    ),
    (
        "effect",
        "Declares an algebraic effect: a set of operations a body may perform.",
    ),
    (
        "perform",
        "Invokes an effect operation, dispatching to its installed handler.",
    ),
    (
        "handle",
        "Installs a handler for an effect over a body: `handle E … in body`.",
    ),
    (
        "resume",
        "From inside a handler, resumes the suspended `perform` with a value.",
    ),
    (
        "in",
        "Separates a `handle` block's arms from the body they guard.",
    ),
    (
        "spawn",
        "Starts a fiber: a lightweight, isolated concurrent task.",
    ),
    (
        "await",
        "Waits for a spawned fiber to finish and yields its result.",
    ),
    (
        "yield",
        "Cooperatively hands control back to the scheduler.",
    ),
    ("send", "Sends a message to a fiber's channel."),
    ("recv", "Receives the next message from a fiber's channel."),
    (
        "select",
        "Waits on several channels at once, taking the first that is ready.",
    ),
    (
        "extern",
        "Declares a function implemented outside Osprey (FFI).",
    ),
    (
        "import",
        "Brings a namespace or module into scope: `import ns::Module`.",
    ),
    (
        "namespace",
        "Declares the logical namespace a file contributes to.",
    ),
    (
        "module",
        "A closed module boundary grouping related declarations.",
    ),
    (
        "signature",
        "An explicit module interface listing its exported names and types.",
    ),
    (
        "export",
        "Marks a declaration as visible outside its module.",
    ),
    (
        "opaque",
        "Exports a type by name only, hiding its representation.",
    ),
    ("state", "Declares a durable, stateful module."),
    ("as", "Aliases an import: `import ns::Module as Alias`."),
    (
        "where",
        "Attaches variance or constraints to a declaration's type parameters.",
    ),
    ("true", "The boolean `true`."),
    ("false", "The boolean `false`."),
];
