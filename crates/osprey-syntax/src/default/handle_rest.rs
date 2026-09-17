//! A `handle` that names no body handles the rest of its block. Written with
//! nothing after it, it handles nothing — and a region that silently does
//! nothing is a mistake, not a feature. Implements [EFFECTS-HANDLE-REST].

use crate::SyntaxError;
use tree_sitter::Node;

/// The message a handler with no region to handle reports.
const NOTHING: &str = "this `handle` names no body, so it handles the rest of \
                       its block — and nothing follows it. Put the statements \
                       it should handle after it, or name the region with `in`";

/// Report every `handle E { … }` that is the last item of its block.
pub(super) fn check(root: Node<'_>, errors: &mut Vec<SyntaxError>) {
    for node in empty_regions(root) {
        errors.push(SyntaxError {
            message: NOTHING.to_owned(),
            position: super::position_from_point(node.start_position()),
        });
    }
}

/// Every bodyless handler with no block item after it.
fn empty_regions(root: Node<'_>) -> Vec<Node<'_>> {
    let mut found = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "handler_expression" && handles_nothing(node) {
            found.push(node);
        }
        stack.extend(node.named_children(&mut cursor));
    }
    found
}

/// Whether this handler names no body and nothing in its block follows it.
fn handles_nothing(handler: Node<'_>) -> bool {
    if handler.child_by_field_name("body").is_some() {
        return false;
    }
    match block_item(handler) {
        Some(item) => item.next_named_sibling().is_none(),
        None => false,
    }
}

/// The block item this handler is the whole of, if it is one. A handler used as
/// a value — `let quiet = handle …` — is not a block item and needs `in`, which
/// the grammar already requires of it.
fn block_item(handler: Node<'_>) -> Option<Node<'_>> {
    let mut node = handler;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "expression" | "primary_expression" | "expression_statement" => node = parent,
            "statement" => return Some(parent),
            _ => return None,
        }
    }
    None
}
