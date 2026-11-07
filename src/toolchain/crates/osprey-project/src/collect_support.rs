//! Small declaration helpers shared by graph collection.

use crate::assemble::source_error;
use crate::model::DeclKind;
use crate::{ProjectError, SourceMetadata};
use osprey_ast::{stmt_meta, Position, SignatureItem, Stmt};

/// The diagnostics sink every assembly pass shares: one `error` wording,
/// located through the pass's own source table.
pub(crate) trait Diagnostics {
    /// The pass's diagnostics and the source table that locates them.
    fn sink(&mut self) -> (&[SourceMetadata], &mut Vec<ProjectError>);

    /// Record `message` at `source`, located when that index names a known
    /// source and unlocated otherwise [MODULES-DIAG].
    fn error(&mut self, source: usize, position: Option<Position>, message: impl Into<String>) {
        let (sources, errors) = self.sink();
        errors.push(source_error(sources, source, position, message));
    }
}

pub(crate) fn primary_name(statement: &Stmt) -> Option<String> {
    match statement {
        Stmt::Signature { name, .. } => Some(name.clone()),
        Stmt::Module { path, .. } => path.last().map(str::to_string),
        _ => stmt_meta::declaration_name(statement).map(str::to_string),
    }
}

pub(crate) fn signature_name_kind(item: &SignatureItem) -> (&str, DeclKind) {
    match item {
        SignatureItem::Value { name, .. } => (name, DeclKind::Value),
        SignatureItem::Function { name, .. } => (name, DeclKind::Function),
        SignatureItem::Type { name, .. } => (name, DeclKind::Type),
        SignatureItem::Effect { name, .. } => (name, DeclKind::Effect),
        SignatureItem::Module { path, .. } => (path.last().unwrap_or_default(), DeclKind::Module),
    }
}

pub(crate) fn kind_matches(actual: DeclKind, expected: DeclKind) -> bool {
    actual == expected
        || (matches!(expected, DeclKind::Function) && matches!(actual, DeclKind::Extern))
}
