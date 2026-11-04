//! Handler lowering: arms (shared with kernels), the `return` and `finally`
//! clauses, and `mask`. Implements [EFFECTS-HANDLER-ARMS],
//! [EFFECTS-FINALIZATION] and [EFFECTS-MASK].

use super::lower::Lowerer;
use osprey_ast::{Expr, HandlerArm};
use tree_sitter::Node;

impl Lowerer<'_> {
    /// The operation, parameters and body shared by a handler arm and a kernel
    /// arm — the two differ only in whether the effect is written per arm.
    pub(crate) fn lower_arm(&self, arm: Node<'_>) -> HandlerArm {
        HandlerArm {
            operation: self.field_text(arm, "operation"),
            params: self
                .first_child_of_kind(arm, "handler_params")
                .map(|hp| self.texts_of_kind(hp, "identifier"))
                .unwrap_or_default(),
            body: self.lower_expr_field(arm, "body"),
            position: Some(self.pos(arm)),
        }
    }

    pub(super) fn lower_handler_arms(&self, node: Node<'_>) -> Vec<HandlerArm> {
        self.named_of_kind(node, "handler_arm")
            .iter()
            .map(|arm| self.lower_arm(*arm))
            .collect()
    }

    /// The `return` and `finally` clauses of one handler.
    pub(super) fn lower_handler_clauses(&self, node: Node<'_>) -> osprey_ast::Clauses {
        osprey_ast::Clauses {
            return_clause: self.lower_handler_return(node),
            finally: self.lower_handler_finally(node),
        }
    }

    /// `finally => cleanup` — the cleanup expression itself. Implements
    /// [EFFECTS-FINALIZATION].
    pub(super) fn lower_handler_finally(&self, node: Node<'_>) -> Option<Box<Expr>> {
        self.named_of_kind(node, "handler_finally")
            .first()
            .map(|clause| Box::new(self.lower_expr_field(*clause, "body")))
    }

    pub(super) fn lower_handler_return(&self, node: Node<'_>) -> Option<Box<Expr>> {
        self.named_of_kind(node, "handler_return")
            .first()
            .map(|clause| {
                Box::new(Expr::Lambda {
                    parameters: vec![osprey_ast::Parameter {
                        name: self.field_text(*clause, "parameter"),
                        ty: None,
                        inline_constraint: false,
                    }],
                    return_type: None,
                    body: Box::new(self.lower_expr_field(*clause, "body")),
                    position: Some(self.pos(*clause)),
                })
            })
    }

    /// `mask E { body }`. A lone value in the braces is the body itself, as
    /// in ML layout.
    pub(super) fn lower_mask(&self, node: Node<'_>) -> Expr {
        let body = match self.lower_expr_field(node, "body") {
            Expr::Block {
                statements,
                value,
                position,
            } => crate::desugar::block(statements, value, position),
            body => body,
        };
        Expr::Mask {
            effect: self.mentioned_effect(node),
            body: Box::new(body),
            position: Some(self.pos(node)),
        }
    }
}
