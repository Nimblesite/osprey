//! Shared `Stmt` metadata accessors: the name, position and doc slots that
//! several pipeline stages read off the same statement forms.
//!
//! Every accessor answers exactly the variant set its callers share. Those
//! sets differ on purpose — documentation pages are built from declarations
//! only, position rebasing leaves import slots untouched — so a caller whose
//! filter is stricter than a shared set keeps its own match rather than
//! broadening one of these.

use crate::{DocComment, Position, Stmt};

/// The declared name of the five scalar named declarations: `let`, `fn`,
/// `extern fn`, `type` and `effect`. Signature and module declarations are
/// excluded; their callers name them through signature items and module
/// paths instead of a single name field.
#[must_use]
pub fn declaration_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Let { name, .. }
        | Stmt::Function { name, .. }
        | Stmt::Extern { name, .. }
        | Stmt::Type { name, .. }
        | Stmt::Effect { name, .. } => Some(name),
        _ => None,
    }
}

/// The recorded position of the seven declaration forms that carry a doc
/// slot ([DOC-MODEL]). Namespaces, assignments, expression statements and
/// imports never name a documented declaration, so they answer `None`.
#[must_use]
pub fn declaration_position(statement: &Stmt) -> Option<Position> {
    match statement {
        Stmt::Let { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Module { position, .. } => *position,
        _ => None,
    }
}

/// The recorded position of every statement form, imports included.
#[must_use]
pub fn statement_position(statement: &Stmt) -> Option<Position> {
    match statement {
        Stmt::Import(import) => import.position,
        Stmt::Namespace { position, .. }
        | Stmt::Let { position, .. }
        | Stmt::Assignment { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Module { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Expr { position, .. } => *position,
    }
}

/// The mutable position slot of every statement form except imports: the
/// string-rebase pass rewrites positions recorded in source text and leaves
/// import slots untouched.
#[must_use]
pub fn position_mut(statement: &mut Stmt) -> Option<&mut Option<Position>> {
    match statement {
        Stmt::Namespace { position, .. }
        | Stmt::Let { position, .. }
        | Stmt::Assignment { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Module { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Expr { position, .. } => Some(position),
        Stmt::Import(_) => None,
    }
}

/// The mutable outer doc slot of the seven declaration forms that carry one
/// ([DOC-MODEL]). Namespaces answer `None`: their docs are a scope's, and
/// every attach site scopes them with the namespace itself.
#[must_use]
pub fn doc_mut(statement: &mut Stmt) -> Option<&mut Option<DocComment>> {
    match statement {
        Stmt::Let { doc: slot, .. }
        | Stmt::Function { doc: slot, .. }
        | Stmt::Extern { doc: slot, .. }
        | Stmt::Type { doc: slot, .. }
        | Stmt::Effect { doc: slot, .. }
        | Stmt::Module { doc: slot, .. }
        | Stmt::Signature { doc: slot, .. } => Some(slot),
        _ => None,
    }
}
