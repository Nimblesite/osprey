//! Small declaration helpers shared by graph collection.

use crate::model::DeclKind;
use osprey_ast::{SignatureItem, Stmt};

pub(crate) fn primary_name(statement: &Stmt) -> Option<String> {
    match statement {
        Stmt::Let { name, .. }
        | Stmt::Function { name, .. }
        | Stmt::Extern { name, .. }
        | Stmt::Type { name, .. }
        | Stmt::Effect { name, .. }
        | Stmt::Signature { name, .. } => Some(name.clone()),
        Stmt::Module { path, .. } => path.last().map(str::to_string),
        _ => None,
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
