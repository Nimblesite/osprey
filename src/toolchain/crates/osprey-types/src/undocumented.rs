//! Undocumented public API. Implements [DOC-EXPORT-REQUIRED].
//!
//! An exported declaration is a library's contract with its callers, so it
//! must say what it is for. A module that exports anything must be documented
//! too, by an outer or an inner comment. Variants and fields are documented by
//! their type ([DOC-ATTACH]), so a documented type covers its constructors.

use osprey_ast::{ModuleItem, Position, Program, Stmt, Visibility};

use crate::TypeWarning;

/// Shared rule identifier for compiler and editor diagnostics.
pub const UNDOCUMENTED_EXPORT: &str = "undocumented-export";

/// Every undocumented export in `program`, in source order.
#[must_use]
pub fn undocumented_exports(program: &Program) -> Vec<TypeWarning> {
    let mut warnings = Vec::new();
    statements(&program.statements, &mut warnings);
    warnings
}

fn statements(statements: &[Stmt], warnings: &mut Vec<TypeWarning>) {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => self::statements(body, warnings),
            Stmt::Module { .. } => module(statement, "", false, warnings),
            _ => {}
        }
    }
}

/// A module, its exported items, and the modules nested in it. One exported
/// as an item is reported once, as that item, never again as a module.
fn module(statement: &Stmt, owner: &str, exported: bool, warnings: &mut Vec<TypeWarning>) {
    let Stmt::Module {
        path,
        body,
        doc,
        inner_doc,
        position,
        ..
    } = statement
    else {
        return;
    };
    let name = qualified(owner, &path.to_string());
    let exports = body
        .iter()
        .any(|item| item.visibility == Visibility::Exported);
    if exports && !exported && doc.is_none() && inner_doc.is_none() {
        warnings.push(warning(
            format!("module `{name}` exports an API but has no documentation comment"),
            *position,
        ));
    }
    for item in body {
        self::item(item, &name, warnings);
    }
}

fn item(item: &ModuleItem, owner: &str, warnings: &mut Vec<TypeWarning>) {
    let exported = item.visibility == Visibility::Exported;
    let declaration = item.declaration.as_ref();
    if exported {
        if let Some((kind, name, position)) = undocumented(declaration) {
            let name = qualified(owner, &name);
            warnings.push(warning(
                format!("exported {kind} `{name}` has no documentation comment"),
                position,
            ));
        }
    }
    module(declaration, owner, exported, warnings);
}

/// The kind, name and position of a declaration that carries no doc. A
/// module counts as documented by an outer or an inner comment.
fn undocumented(declaration: &Stmt) -> Option<(String, String, Option<Position>)> {
    if let Stmt::Module {
        path,
        doc,
        inner_doc,
        position,
        ..
    } = declaration
    {
        let documented = doc.is_some() || inner_doc.is_some();
        return (!documented).then(|| ("module".to_owned(), path.to_string(), *position));
    }
    let declared = declaration.documented()?;
    let kind = declared.kind.to_lowercase();
    declared
        .doc
        .is_none()
        .then(|| (kind, declared.name.to_owned(), declared.position))
}

fn qualified(owner: &str, name: &str) -> String {
    if owner.is_empty() {
        name.to_owned()
    } else {
        format!("{owner}::{name}")
    }
}

fn warning(message: String, position: Option<Position>) -> TypeWarning {
    TypeWarning {
        message,
        position,
        rule: UNDOCUMENTED_EXPORT,
    }
}
