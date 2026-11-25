//! Private module members nothing reads. Implements [TYPE-WARNINGS-UNUSED-PRIVATE].
//!
//! A module is a closed boundary: an item it does not export can be named only
//! from inside it. A private function or value whose name no expression of its
//! module mentions is therefore dead. The test is by spelling, not binding: an
//! identifier, path segment, field, method or updated record of the same name
//! counts as a read, so the rule can miss dead code but never flags live code.
//! A module ascribed a signature is skipped, since the signature names its
//! surface.

use std::collections::HashSet;

use osprey_ast::{
    walk_statement, AstVisitor, Expr, ModuleItem, Position, Program, Stmt, Visibility,
};

use crate::{undocumented::qualified, TypeWarning};

/// Shared rule identifier for compiler and editor diagnostics.
pub const UNUSED_PRIVATE: &str = "unused-private";

/// Every private module function or value `program` never names, in source order.
#[must_use]
pub fn unused_privates(program: &Program) -> Vec<TypeWarning> {
    let mut warnings = Vec::new();
    statements(&program.statements, "", &mut warnings);
    warnings
}

fn statements(statements: &[Stmt], owner: &str, warnings: &mut Vec<TypeWarning>) {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => self::statements(body, owner, warnings),
            Stmt::Module { .. } => module(statement, owner, warnings),
            _ => {}
        }
    }
}

/// A module's unread private items, then those of the modules nested in it.
fn module(statement: &Stmt, owner: &str, warnings: &mut Vec<TypeWarning>) {
    let Stmt::Module {
        path,
        body,
        signature,
        ..
    } = statement
    else {
        return;
    };
    let name = qualified(owner, &path.to_string());
    if signature.is_none() {
        warnings.extend(unread_items(statement, body, &name));
    }
    for item in body {
        module(&item.declaration, &name, warnings);
    }
}

/// The private items of `body` no expression of `module` names.
fn unread_items(module: &Stmt, body: &[ModuleItem], owner: &str) -> Vec<TypeWarning> {
    let mentioned = mentions(module);
    body.iter()
        .filter_map(|item| unread(item, owner, &mentioned))
        .collect()
}

fn unread(item: &ModuleItem, owner: &str, mentioned: &HashSet<String>) -> Option<TypeWarning> {
    if item.visibility == Visibility::Exported {
        return None;
    }
    let (kind, name, position) = declared(&item.declaration)?;
    (!mentioned.contains(name)).then(|| TypeWarning {
        message: format!(
            "private {kind} `{}` is never used; delete it",
            qualified(owner, name)
        ),
        position,
        rule: UNUSED_PRIVATE,
    })
}

fn declared(statement: &Stmt) -> Option<(&'static str, &str, Option<Position>)> {
    match statement {
        Stmt::Function { name, position, .. } => Some(("function", name, *position)),
        Stmt::Let { name, position, .. } => Some(("value", name, *position)),
        _ => None,
    }
}

/// Every name any expression inside `statement` spells.
fn mentions(statement: &Stmt) -> HashSet<String> {
    let mut mentions = Mentions(HashSet::new());
    walk_statement(statement, &mut mentions);
    mentions.0
}

struct Mentions(HashSet<String>);

impl AstVisitor for Mentions {
    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Identifier(name)
            | Expr::FieldAccess { field: name, .. }
            | Expr::MethodCall { method: name, .. }
            | Expr::Update { record: name, .. } => {
                let _ = self.0.insert(name.clone());
            }
            Expr::Path(path) => self.0.extend(path.segments.iter().cloned()),
            _ => {}
        }
    }
}
