//! Pure computation factories can be selected again under a static region.
//! Selection may read only immutable inputs and must return a closure without
//! executing work. Effectful factories and mutable selectors stay opaque.

use crate::{AstVisitor, Expr, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Candidates {
    immutable: BTreeSet<String>,
    bindings: BTreeMap<String, Expr>,
}

impl AstVisitor for Candidates {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Function { parameters, .. } => {
                self.immutable.extend(parameters.iter().map(|p| p.name.clone()));
            }
            Stmt::Let { name, mutable: false, value, .. } => {
                let _ = self.immutable.insert(name.clone());
                let _ = self.bindings.insert(name.clone(), value.clone());
            }
            _ => {}
        }
    }
}

pub(super) fn collect(program: &Program, definitions: &BTreeMap<String, Stmt>) -> BTreeMap<String, Expr> {
    let mut candidates = Candidates::default();
    crate::walk_program(program, &mut candidates);
    candidates.bindings.into_iter().filter(|(_, value)| {
        factory(value, definitions, &candidates.immutable)
    }).collect()
}

fn factory(value: &Expr, definitions: &BTreeMap<String, Stmt>, immutable: &BTreeSet<String>) -> bool {
    let Expr::Call { function, arguments, named_arguments } = value else { return false };
    let Expr::Identifier(name) = function.as_ref() else { return false };
    let Some(Stmt::Function { parameters, body, .. }) = definitions.get(name) else { return false };
    let names = parameters.iter().map(|p| p.name.clone()).collect();
    named_arguments.is_empty() && parameters.len() == arguments.len()
        && arguments.iter().all(|argument| pure_input(argument, immutable))
        && closure_selection(body, &names)
}

fn closure_selection(value: &Expr, parameters: &BTreeSet<String>) -> bool {
    match value {
        Expr::Lambda { .. } => true,
        Expr::Match { value, arms } => pure_input(value, parameters)
            && !arms.is_empty() && arms.iter().all(|arm| closure_selection(&arm.body, parameters)),
        Expr::Block { statements, value, .. } => statements.is_empty()
            && value.as_deref().is_some_and(|value| closure_selection(value, parameters)),
        _ => false,
    }
}

fn pure_input(value: &Expr, immutable: &BTreeSet<String>) -> bool {
    match value {
        Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => true,
        Expr::Identifier(name) => immutable.contains(name),
        Expr::Binary { op, left, right, .. } => matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||")
            && pure_input(left, immutable) && pure_input(right, immutable),
        _ => false,
    }
}
