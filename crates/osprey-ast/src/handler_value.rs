//! Callable handler values share the ordinary closure and lexical handler core.

use crate::{Expr, HandlerArm, Parameter, Position, Stage};

// `$` cannot occur in a source identifier in either flavor.
const ACTION: &str = "$handler_action";

/// Build a reusable handler taking a zero-argument computation.
/// Implements [EFFECTS-HANDLER-VALUE] and [FLAVOR-HANDLER-VALUE].
#[must_use]
pub fn handler_value(
    stage: Stage,
    effect: String,
    arms: Vec<HandlerArm>,
    return_clause: Option<Box<Expr>>,
    position: Option<Position>,
) -> Expr {
    let body = Expr::Handler {
        stage,
        effect,
        arms,
        return_clause,
        body: Box::new(invoke_action()),
        position,
    };
    Expr::Lambda {
        parameters: vec![Parameter {
            name: ACTION.into(),
            ty: None,
            inline_constraint: false,
        }],
        return_type: None,
        body: Box::new(body),
        position,
    }
}

fn invoke_action() -> Expr {
    Expr::Call {
        function: Box::new(Expr::Identifier(ACTION.into())),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    }
}
