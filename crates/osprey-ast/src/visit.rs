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

/// A borrowed syntax node whose immediate children can be visited in source order.
#[derive(Debug, Clone, Copy)]
pub enum AstNode<'a> {
    /// A declaration or executable statement.
    Statement(&'a Stmt),
    /// An expression, including any statements in a block.
    Expression(&'a Expr),
}

impl AstNode<'_> {
    /// Visit immediate children only. Callers retain control of recursion,
    /// lexical scopes, and branch-specific combination rules.
    pub fn for_each_child(self, mut visit: impl FnMut(Self)) {
        match self {
            Self::Statement(statement) => statement_children(statement, &mut visit),
            Self::Expression(expression) => expression_children(expression, &mut visit),
        }
    }
}

/// Walk every statement and expression in source order without recursive
/// visitor boilerplate. Implements the shared traversal required by
/// [LSP-HOVER-EFFECT-OPERATIONS] and [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS].
pub fn walk_program(program: &Program, visitor: &mut impl AstVisitor) {
    let mut pending: Vec<_> = program
        .statements
        .iter()
        .rev()
        .map(AstNode::Statement)
        .collect();
    while let Some(node) = pending.pop() {
        match node {
            AstNode::Statement(statement) => visitor.statement(statement),
            AstNode::Expression(expression) => visitor.expression(expression),
        }
        let start = pending.len();
        node.for_each_child(|child| pending.push(child));
        if let Some(children) = pending.get_mut(start..) {
            children.reverse();
        }
    }
}

fn statement_children<'a>(statement: &'a Stmt, visit: &mut impl FnMut(AstNode<'a>)) {
    match statement {
        Stmt::Namespace { body, .. } => {
            for statement in body {
                visit(AstNode::Statement(statement));
            }
        }
        Stmt::Module { body, .. } => {
            for item in body {
                visit(AstNode::Statement(&item.declaration));
            }
        }
        Stmt::Let { value, .. }
        | Stmt::Assignment { value, .. }
        | Stmt::Expr { value, .. }
        | Stmt::Function { body: value, .. } => visit(AstNode::Expression(value)),
        Stmt::Type { variants, .. } => {
            for field in variants.iter().flat_map(|variant| &variant.fields) {
                if let Some(constraint) = &field.constraint {
                    visit(AstNode::Expression(constraint));
                }
            }
        }
        Stmt::Import(_) | Stmt::Extern { .. } | Stmt::Effect { .. } | Stmt::Signature { .. } => {}
    }
}

fn expression_children<'a>(expression: &'a Expr, visit: &mut impl FnMut(AstNode<'a>)) {
    match expression {
        Expr::InterpolatedStr(parts) => {
            for part in parts {
                if let InterpolatedPart::Expr(value) = part {
                    visit(AstNode::Expression(value));
                }
            }
        }
        Expr::List(values, _) => visit_each(values, visit, |value| value),
        Expr::Map(entries) => {
            for entry in entries {
                visit(AstNode::Expression(&entry.key));
                visit(AstNode::Expression(&entry.value));
            }
        }
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => visit_each(fields, visit, |field| &field.value),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            visit(AstNode::Expression(left));
            visit(AstNode::Expression(right));
        }
        Expr::TypeApply {
            function: operand, ..
        }
        | Expr::Unary { operand, .. }
        | Expr::Spawn(operand)
        | Expr::Await(operand)
        | Expr::Recv(operand)
        | Expr::FieldAccess {
            target: operand, ..
        }
        | Expr::Lambda { body: operand, .. } => visit(AstNode::Expression(operand)),
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
            visit(AstNode::Expression(target));
            visit_each(arguments, visit, |argument| argument);
            visit_each(named_arguments, visit, |argument| &argument.value);
        }
        Expr::Index { target, index } => {
            visit(AstNode::Expression(target));
            visit(AstNode::Expression(index));
        }
        Expr::Match { value, arms } => {
            visit(AstNode::Expression(value));
            visit_each(arms, visit, |arm| &arm.body);
        }
        Expr::Block { statements, value } => {
            for statement in statements {
                visit(AstNode::Statement(statement));
            }
            if let Some(value) = value {
                visit(AstNode::Expression(value));
            }
        }
        Expr::Yield(value) | Expr::Resume(value) => {
            if let Some(value) = value {
                visit(AstNode::Expression(value));
            }
        }
        Expr::Send { channel, value } => {
            visit(AstNode::Expression(channel));
            visit(AstNode::Expression(value));
        }
        Expr::Select { arms } => visit_each(arms, visit, |arm| &arm.body),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            visit_each(arguments, visit, |argument| argument);
            visit_each(named_arguments, visit, |argument| &argument.value);
        }
        Expr::Handler { arms, body, .. } => {
            visit_each(arms, visit, |arm| &arm.body);
            visit(AstNode::Expression(body));
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Identifier(_)
        | Expr::Path(_) => {}
    }
}

fn visit_each<'a, T>(
    items: &'a [T],
    visit: &mut impl FnMut(AstNode<'a>),
    expression: impl Fn(&'a T) -> &'a Expr,
) {
    for item in items {
        visit(AstNode::Expression(expression(item)));
    }
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
