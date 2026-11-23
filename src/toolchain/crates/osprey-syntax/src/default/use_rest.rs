//! Default `use` statements: `use p <- f(a)` passes the rest of its block to the
//! call as a final callback. The checks here run before lowering; the lowering
//! itself is in `expr/calls.rs`, beside `handle`'s. Implements [BLOCK-USE] and
//! [BLOCK-USE-REJECT].

use crate::{SyntaxError, USE_NEEDS_A_BLOCK, USE_NEEDS_A_CALL, USE_NEEDS_A_REST};
use osprey_ast::{Expr, Parameter};
use tree_sitter::Node;

pub(super) fn check(root: Node<'_>, errors: &mut Vec<SyntaxError>) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_statement" {
            if let Some(message) = rejection(node) {
                errors.push(SyntaxError {
                    message: message.to_owned(),
                    position: super::position_from_point(node.start_position()),
                });
            }
        }
        stack.extend(node.named_children(&mut cursor));
    }
}

fn rejection(statement: Node<'_>) -> Option<&'static str> {
    let item = statement.parent()?;
    if item.parent().is_none_or(|block| block.kind() != "block") {
        return Some(USE_NEEDS_A_BLOCK);
    }
    if !super::handle_rest::rest_follows(item) {
        return Some(USE_NEEDS_A_REST);
    }
    let call = statement.child_by_field_name("call").and_then(innermost);
    (!call.is_some_and(is_call)).then_some(USE_NEEDS_A_CALL)
}

/// The expression under its `expression` / `primary_expression` wrappers.
fn innermost(mut node: Node<'_>) -> Option<Node<'_>> {
    while matches!(node.kind(), "expression" | "primary_expression") {
        node = node.named_child(0)?;
    }
    Some(node)
}

/// A call with an argument list, not a field read or an index.
fn is_call(node: Node<'_>) -> bool {
    node.kind() == "call_expression"
        && node.child_by_field_name("member").is_none()
        && node.child_by_field_name("index").is_none()
}

/// The callback's parameters: none for `use <-`, else one per binder.
pub(crate) fn parameters(names: Vec<String>) -> Vec<Parameter> {
    names
        .into_iter()
        .map(|name| Parameter {
            name,
            ty: None,
            inline_constraint: false,
        })
        .collect()
}

/// `call` with `callback` appended as its last positional argument. Anything
/// else was already rejected by [`check`], so it is returned as written.
pub(crate) fn with_callback(call: Expr, callback: Expr) -> Expr {
    match call {
        Expr::Call {
            function,
            mut arguments,
            named_arguments,
        } => {
            arguments.push(callback);
            Expr::Call {
                function,
                arguments,
                named_arguments,
            }
        }
        Expr::MethodCall {
            target,
            method,
            mut arguments,
            named_arguments,
        } => {
            arguments.push(callback);
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            }
        }
        other => other,
    }
}
