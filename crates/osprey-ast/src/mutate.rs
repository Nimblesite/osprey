//! In-place child traversal.
//!
//! [`visit`](crate::visit) walks the tree by shared reference, which is all a
//! collector needs and all a borrow checker allows one walker to offer. A
//! rewriting pass — [`stage`](crate::stage) discharging a static handler — must
//! *replace* nodes, so it needs the same traversal by unique reference. The two
//! cannot share one implementation without interior mutability, so this module
//! is the mutable half of the same shape: it hands each direct child to a
//! callback and lets the caller decide whether to recurse.

use crate::{Expr, InterpolatedPart, Stmt};

/// Apply `visit` to every direct child expression of `expression`.
///
/// Only one level deep: a rewriter recurses by calling this again from inside
/// `visit`, which is what lets it choose pre- or post-order.
pub fn children_mut(expression: &mut Expr, visit: &mut impl FnMut(&mut Expr)) {
    match expression {
        Expr::InterpolatedStr(parts) => {
            for part in &mut *parts {
                if let InterpolatedPart::Expr(value) = part {
                    visit(value);
                }
            }
        }
        Expr::List(values) => {
            for value in &mut *values {
                visit(value);
            }
        }
        Expr::Map(entries) => {
            for entry in &mut *entries {
                visit(&mut entry.key);
                visit(&mut entry.value);
            }
        }
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => {
            for field in &mut *fields {
                visit(&mut field.value);
            }
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            visit(left);
            visit(right);
        }
        Expr::Unary { operand, .. }
        | Expr::Spawn(operand)
        | Expr::Await(operand)
        | Expr::Recv(operand)
        | Expr::Lambda { body: operand, .. } => visit(operand),
        Expr::Call {
            function: target,
            arguments,
            named_arguments,
        }
        | Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            visit(target);
            for argument in &mut *arguments {
                visit(argument);
            }
            for argument in &mut *named_arguments {
                visit(&mut argument.value);
            }
        }
        Expr::FieldAccess { target, .. } => visit(target),
        Expr::Index { target, index } => {
            visit(target);
            visit(index);
        }
        Expr::Match { value, arms } => {
            visit(value);
            for arm in &mut *arms {
                visit(&mut arm.body);
            }
        }
        other => effect_children_mut(other, visit),
    }
}

/// The concurrency, block, and effect forms — split out so neither half of the
/// traversal exceeds one screen.
fn effect_children_mut(expression: &mut Expr, visit: &mut impl FnMut(&mut Expr)) {
    match expression {
        Expr::Block { statements, value } => {
            for statement in &mut *statements {
                statement_children_mut(statement, visit);
            }
            if let Some(value) = value {
                visit(value);
            }
        }
        Expr::Yield(value) | Expr::Resume(value) => {
            if let Some(value) = value {
                visit(value);
            }
        }
        Expr::Send { channel, value } => {
            visit(channel);
            visit(value);
        }
        Expr::Select { arms } => {
            for arm in &mut *arms {
                visit(&mut arm.body);
            }
        }
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            for argument in &mut *arguments {
                visit(argument);
            }
            for argument in &mut *named_arguments {
                visit(&mut argument.value);
            }
        }
        Expr::Handler { arms, body, .. } => {
            for arm in &mut *arms {
                visit(&mut arm.body);
            }
            visit(body);
        }
        // Leaves, and every form the first half already covered.
        _ => {}
    }
}

/// Apply `visit` to every direct child expression of `statement`.
pub fn statement_children_mut(statement: &mut Stmt, visit: &mut impl FnMut(&mut Expr)) {
    match statement {
        Stmt::Namespace { body, .. } => {
            for nested in &mut *body {
                statement_children_mut(nested, visit);
            }
        }
        Stmt::Module { body, .. } => {
            for item in &mut *body {
                statement_children_mut(&mut item.declaration, visit);
            }
        }
        Stmt::Let { value, .. }
        | Stmt::Assignment { value, .. }
        | Stmt::Expr { value, .. }
        | Stmt::Function { body: value, .. } => visit(value),
        Stmt::Type { variants, .. } => {
            for variant in &mut *variants {
                for field in &mut variant.fields {
                    if let Some(constraint) = &mut field.constraint {
                        visit(constraint);
                    }
                }
            }
        }
        Stmt::Import(_) | Stmt::Extern { .. } | Stmt::Effect { .. } | Stmt::Signature { .. } => {}
    }
}
