//! Block-scoped handlers must govern actual following code.
//! Implements [EFFECTS-HANDLE-REST].

use crate::SyntaxError;
use tree_sitter::Node;

pub(super) fn check(root: Node<'_>, errors: &mut Vec<SyntaxError>) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "handler_expression" {
            if let Some(message) = region_error(node) {
                errors.push(SyntaxError {
                    message: message.to_owned(),
                    position: super::position_from_point(node.start_position()),
                });
            }
        }
        stack.extend(node.named_children(&mut cursor));
    }
}

fn region_error(handler: Node<'_>) -> Option<&'static str> {
    let Some(item) = block_item(handler) else {
        return Some(
            "`handle` must be a block statement; use `handler` to construct a callable handler",
        );
    };
    let follows = std::iter::successors(item.next_named_sibling(), Node::next_named_sibling)
        .any(|node| matches!(node.kind(), "statement" | "expression"));
    (!follows).then_some("this `handle` has nothing to handle; put the handled statements after it")
}

fn block_item(handler: Node<'_>) -> Option<Node<'_>> {
    let mut node = handler;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "expression" | "primary_expression" | "expression_statement" => node = parent,
            "statement" if parent.parent().is_some_and(|block| block.kind() == "block") => {
                return Some(parent)
            }
            _ => return None,
        }
    }
    None
}
