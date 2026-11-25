//! Private module members nothing reads. Implements [TYPE-WARNINGS-UNUSED-PRIVATE].
//!
//! Follow private function references from exports, initializers and nested
//! modules. A private value is reported only when nothing mentions its name.
//! Name matching is conservative: a field, path segment or method of the same
//! spelling can hide unused code. Signatures supply their own public surface.

use std::collections::{HashMap, HashSet};

use osprey_ast::{
    walk_statement, AstVisitor, Expr, ModuleItem, Position, Program, Stmt, Visibility,
};

use crate::{undocumented::qualified, TypeWarning};

/// Shared rule identifier for compiler and editor diagnostics.
pub const UNUSED_PRIVATE: &str = "unused-private";

/// Unreachable private module functions and unread values, in source order.
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
        warnings.extend(unread_items(body, &name));
    }
    for item in body {
        module(&item.declaration, &name, warnings);
    }
}

fn unread_items(body: &[ModuleItem], owner: &str) -> Vec<TypeWarning> {
    let mut mentioned = HashSet::new();
    let mut roots = Vec::new();
    let mut dependencies: HashMap<&str, Vec<String>> = HashMap::new();
    for item in body {
        let names = mentions(&item.declaration);
        mentioned.extend(names.iter().cloned());
        match (item.visibility, item.declaration.as_ref()) {
            (Visibility::Private, Stmt::Function { name, .. }) => {
                dependencies.entry(name).or_default().extend(names);
            }
            _ => roots.extend(names),
        }
    }
    let reachable = reachable_names(roots, dependencies);
    body.iter()
        .filter_map(|item| unread(item, owner, &mentioned, &reachable))
        .collect()
}

fn reachable_names(
    mut pending: Vec<String>,
    mut dependencies: HashMap<&str, Vec<String>>,
) -> HashSet<String> {
    let mut reached = HashSet::new();
    while let Some(name) = pending.pop() {
        if let Some(next) = dependencies.remove(name.as_str()) {
            pending.extend(next);
        }
        let _ = reached.insert(name);
    }
    reached
}

fn unread(
    item: &ModuleItem,
    owner: &str,
    mentioned: &HashSet<String>,
    reachable: &HashSet<String>,
) -> Option<TypeWarning> {
    if item.visibility == Visibility::Exported {
        return None;
    }
    let (kind, name, position) = declared(&item.declaration)?;
    let used = if matches!(item.declaration.as_ref(), Stmt::Function { .. }) {
        reachable
    } else {
        mentioned
    };
    (!used.contains(name)).then(|| TypeWarning {
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
