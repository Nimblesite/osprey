//! Pure computation factories can be selected again under a static region.
//! Selection may read only immutable inputs and must return a closure without
//! executing work. Effectful factories and mutable selectors stay opaque.

use super::bindings::positional_call;
use crate::{AstVisitor, Expr, Parameter, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

type Definitions = BTreeMap<String, Stmt>;
type Names = BTreeSet<String>;

#[derive(Default)]
struct Candidates {
    immutable: Names,
    bindings: BTreeMap<String, Expr>,
}

impl AstVisitor for Candidates {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Function { parameters, .. } => {
                self.immutable
                    .extend(parameters.iter().map(|p| p.name.clone()));
            }
            Stmt::Let {
                name,
                mutable: false,
                value,
                ..
            } => {
                let _ = self.immutable.insert(name.clone());
                let _ = self.bindings.insert(name.clone(), value.clone());
            }
            _ => {}
        }
    }
}

pub(super) fn collect(program: &Program, definitions: &Definitions) -> BTreeMap<String, Expr> {
    let mut candidates = Candidates::default();
    crate::walk_program(program, &mut candidates);
    candidates
        .bindings
        .into_iter()
        .filter(|(_, value)| factory(value, definitions, &candidates.immutable))
        .collect()
}

fn factory(value: &Expr, definitions: &Definitions, immutable: &Names) -> bool {
    let Some((function, arguments)) = positional_call(value) else {
        return false;
    };
    definition(function, definitions).is_some_and(|(parameters, body)| {
        let names = parameters.iter().map(|p| p.name.clone()).collect();
        parameters.len() == arguments.len()
            && arguments
                .iter()
                .all(|argument| pure_input(argument, immutable))
            && closure_selection(body, &names)
    })
}

fn definition<'a>(
    function: &Expr,
    definitions: &'a Definitions,
) -> Option<(&'a [Parameter], &'a Expr)> {
    let Expr::Identifier(name) = function else {
        return None;
    };
    match definitions.get(name)? {
        Stmt::Function {
            parameters, body, ..
        } => Some((parameters, body)),
        _ => None,
    }
}

fn closure_selection(value: &Expr, parameters: &Names) -> bool {
    match value {
        Expr::Lambda { .. } => true,
        Expr::Match { value, arms } => {
            pure_input(value, parameters)
                && !arms.is_empty()
                && arms
                    .iter()
                    .all(|arm| closure_selection(&arm.body, parameters))
        }
        Expr::Block {
            statements, value, ..
        } => statements.is_empty() && selected_value(value.as_deref(), parameters),
        _ => false,
    }
}

fn selected_value(value: Option<&Expr>, parameters: &Names) -> bool {
    value.is_some_and(|value| closure_selection(value, parameters))
}

fn pure_input(value: &Expr, immutable: &Names) -> bool {
    match value {
        Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => true,
        Expr::Identifier(name) => immutable.contains(name),
        Expr::Binary {
            op, left, right, ..
        } => {
            matches!(
                op.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
            ) && pure_input(left, immutable)
                && pure_input(right, immutable)
        }
        _ => false,
    }
}
