//! Shared slice-walking helper for the AST expression visitors across the
//! backend and language server. Every visitor that recurses into a collection
//! node (argument lists, named arguments, field assignments, match arms)
//! repeated the same "for each element, recurse into `pick(element)`" step once
//! per collection kind. It lives here once, generic over the visitor's threaded
//! state — the free-variable collector and effect scanner
//! ([`osprey-codegen`]) and the symbol collector ([`osprey-lsp`]) all reuse it.

use crate::{Expr, InterpolatedPart, Program, Stmt};

/// Callbacks for one preorder walk over every statement and expression in a
/// program. Implementors override only the node kinds they need.
pub trait AstVisitor {
    /// Observe one statement before its nested expressions/statements.
    fn statement(&mut self, _statement: &Stmt) {}

    /// Observe one expression before its children.
    fn expression(&mut self, _expression: &Expr) {}
}

enum Node<'a> {
    Statement(&'a Stmt),
    Expression(&'a Expr),
}

/// Walk every statement and expression in source order without recursive
/// visitor boilerplate. Implements the shared traversal required by
/// [LSP-HOVER-EFFECT-OPERATIONS] and [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS].
pub fn walk_program(program: &Program, visitor: &mut impl AstVisitor) {
    let mut pending: Vec<_> = program
        .statements
        .iter()
        .rev()
        .map(Node::Statement)
        .collect();
    while let Some(node) = pending.pop() {
        match node {
            Node::Statement(statement) => {
                visitor.statement(statement);
                push_statement_children(statement, &mut pending);
            }
            Node::Expression(expression) => {
                visitor.expression(expression);
                push_expression_children(expression, &mut pending);
            }
        }
    }
}

fn push_statement_children<'a>(statement: &'a Stmt, pending: &mut Vec<Node<'a>>) {
    match statement {
        Stmt::Namespace { body, .. } => {
            pending.extend(body.iter().rev().map(Node::Statement));
        }
        Stmt::Module { body, .. } => pending.extend(
            body.iter()
                .rev()
                .map(|item| Node::Statement(item.declaration.as_ref())),
        ),
        Stmt::Let { value, .. }
        | Stmt::Assignment { value, .. }
        | Stmt::Expr { value, .. }
        | Stmt::Function { body: value, .. } => pending.push(Node::Expression(value)),
        Stmt::Type { variants, .. } => push_constraints(variants, pending),
        Stmt::Import(_) | Stmt::Extern { .. } | Stmt::Effect { .. } | Stmt::Signature { .. } => {}
    }
}

fn push_constraints<'a>(variants: &'a [crate::TypeVariant], pending: &mut Vec<Node<'a>>) {
    for variant in variants.iter().rev() {
        for field in variant.fields.iter().rev() {
            if let Some(constraint) = &field.constraint {
                pending.push(Node::Expression(constraint));
            }
        }
    }
}

fn push_expression_children<'a>(expression: &'a Expr, pending: &mut Vec<Node<'a>>) {
    match expression {
        Expr::InterpolatedStr(parts) => {
            for part in parts.iter().rev() {
                if let InterpolatedPart::Expr(value) = part {
                    pending.push(Node::Expression(value));
                }
            }
        }
        Expr::List(values) => push_each(values, pending, |value| value),
        Expr::Map(entries) => {
            for entry in entries.iter().rev() {
                pending.push(Node::Expression(&entry.value));
                pending.push(Node::Expression(&entry.key));
            }
        }
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => push_each(fields, pending, |field| &field.value),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            pending.push(Node::Expression(right));
            pending.push(Node::Expression(left));
        }
        Expr::Unary { operand, .. }
        | Expr::Spawn(operand)
        | Expr::Await(operand)
        | Expr::Recv(operand) => pending.push(Node::Expression(operand)),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            push_each(named_arguments, pending, |argument| &argument.value);
            push_each(arguments, pending, |argument| argument);
            pending.push(Node::Expression(function));
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            push_each(named_arguments, pending, |argument| &argument.value);
            push_each(arguments, pending, |argument| argument);
            pending.push(Node::Expression(target));
        }
        Expr::FieldAccess { target, .. } => pending.push(Node::Expression(target)),
        Expr::Index { target, index } => {
            pending.push(Node::Expression(index));
            pending.push(Node::Expression(target));
        }
        Expr::Lambda { body, .. } => pending.push(Node::Expression(body)),
        Expr::Match { value, arms } => {
            push_each(arms, pending, |arm| &arm.body);
            pending.push(Node::Expression(value));
        }
        Expr::Block { statements, value } => {
            if let Some(value) = value {
                pending.push(Node::Expression(value));
            }
            pending.extend(statements.iter().rev().map(Node::Statement));
        }
        Expr::Yield(value) | Expr::Resume(value) => {
            if let Some(value) = value {
                pending.push(Node::Expression(value));
            }
        }
        Expr::Send { channel, value } => {
            pending.push(Node::Expression(value));
            pending.push(Node::Expression(channel));
        }
        Expr::Select { arms } => push_each(arms, pending, |arm| &arm.body),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            push_each(named_arguments, pending, |argument| &argument.value);
            push_each(arguments, pending, |argument| argument);
        }
        Expr::Handler { arms, body, .. } => {
            pending.push(Node::Expression(body));
            push_each(arms, pending, |arm| &arm.body);
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Identifier(_)
        | Expr::Path(_) => {}
    }
}

fn push_each<'a, T>(
    items: &'a [T],
    pending: &mut Vec<Node<'a>>,
    expression: impl Fn(&'a T) -> &'a Expr,
) {
    pending.extend(
        items
            .iter()
            .rev()
            .map(|item| Node::Expression(expression(item))),
    );
}

/// Recurse into every element of `items`, projecting each to its
/// sub-expression with `pick` and visiting it with `recur` under the visitor's
/// threaded state `ctx`. `Ctx` is whatever the caller threads through its
/// traversal — a single `&mut Vec` for the symbol collector, or a pair of
/// `&mut BTreeSet`s wrapped in a tuple for the free-variable / effect scans.
pub fn walk_each<T, Ctx>(
    items: &[T],
    ctx: &mut Ctx,
    pick: impl Fn(&T) -> &Expr,
    recur: impl Fn(&Expr, &mut Ctx),
) {
    for item in items {
        recur(pick(item), ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_each_projects_and_recurses_threading_the_context() {
        // Project each pair to its `Expr` and sum the integer literals into the
        // threaded accumulator — exercising `pick`, `recur`, and `ctx`.
        let items = [("a", Expr::Integer(2)), ("b", Expr::Integer(5))];
        let mut total = 0i64;
        walk_each(
            &items,
            &mut total,
            |(_, e)| e,
            |e, acc| {
                if let Expr::Integer(n) = e {
                    *acc += n;
                }
            },
        );
        assert_eq!(total, 7);
    }
}
