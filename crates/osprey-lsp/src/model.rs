//! The engine's neutral query/report vocabulary.
//!
//! These types keep `lsp_types` out of the engine: the server layer maps them
//! to the wire protocol. The same vocabulary can back an MCP surface, honouring
//! lspkit's "one engine, two surfaces" contract.

use lspkit_server::Diagnostic;
use lspkit_vfs::DocumentUri;

use crate::analysis::SymbolInfo;

/// A zero-based, half-open `(start_line, start_char, end_line, end_char)` span
/// in the negotiated position encoding.
pub type Span = (u32, u32, u32, u32);

/// A position inside a document, as supplied by the editor.
#[derive(Debug, Clone)]
pub struct At {
    /// Target document.
    pub uri: DocumentUri,
    /// Zero-based line.
    pub line: u32,
    /// Zero-based character offset (negotiated encoding).
    pub character: u32,
}

/// An analysis request answered by [`crate::engine::OspreyEngine`].
#[derive(Debug, Clone)]
pub enum Query {
    /// Diagnostics for a document.
    Diagnostics(DocumentUri),
    /// Document outline.
    Symbols(DocumentUri),
    /// Hover markdown at a position.
    Hover(At),
    /// Definition location(s) for the identifier at a position.
    Definition(At),
    /// All references to the identifier at a position.
    References {
        /// Where the cursor is.
        at: At,
        /// Whether to include the declaration itself.
        include_declaration: bool,
    },
    /// Signature help for the enclosing call at a position.
    SignatureHelp(At),
    /// Completion items at a position. Position-bearing because the answer
    /// depends on where the cursor is: a type annotation, a `receiver.` field
    /// and a declaration head admit entirely different lists.
    /// Implements [LSP-COMPLETION-CONTEXT].
    Completion(At),
}

/// A located span within a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The document URI as a string.
    pub uri: String,
    /// The span within the document.
    pub span: Span,
}

/// Rendered signature help for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInfo {
    /// Full signature label, e.g. `fn add(a: int, b: int) -> int`.
    pub label: String,
    /// Per-parameter labels, e.g. `a: int`.
    pub parameters: Vec<String>,
    /// Zero-based index of the active parameter.
    pub active_parameter: u32,
}

/// What sort of thing a completion item inserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// A language keyword.
    Keyword,
    /// A function.
    Function,
    /// A variable / `let` binding.
    Variable,
    /// A type or effect.
    Type,
}

/// A single completion suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// Insert/display label.
    pub label: String,
    /// Item category.
    pub kind: CompletionKind,
    /// Short detail line.
    pub detail: Option<String>,
    /// Snippet to insert (when richer than `label`).
    pub insert_text: Option<String>,
}

/// The payload produced for a [`Query`].
#[derive(Debug, Clone)]
pub enum Report {
    /// Result of [`Query::Diagnostics`].
    Diagnostics(Vec<Diagnostic>),
    /// Result of [`Query::Symbols`].
    Symbols(Vec<SymbolInfo>),
    /// Result of [`Query::Hover`] — markdown, or `None`.
    Hover(Option<String>),
    /// Result of [`Query::Definition`] / [`Query::References`].
    Locations(Vec<Location>),
    /// Result of [`Query::SignatureHelp`].
    Signature(Option<SignatureInfo>),
    /// Result of [`Query::Completion`].
    Completion(Vec<CompletionItem>),
}

/// Errors the engine can return.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The engine has been shut down and refuses further work.
    #[error("engine has shut down")]
    ShuttingDown,
}
