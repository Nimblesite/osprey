//! Callable handler values share the ordinary closure and lexical handler core.

use crate::{Expr, HandlerArm, Parameter, Position, Stage, Stmt};

/// A handler's optional completion clauses: the unary `return` lambda and the
/// `finally` cleanup expression. Implements [EFFECTS-HANDLER-ARMS] and
/// [EFFECTS-FINALIZATION].
#[derive(Debug, Clone, Default)]
pub struct Clauses {
    /// `return x => …`, as a unary lambda.
    pub return_clause: Option<Box<Expr>>,
    /// `finally => …`, the cleanup expression.
    pub finally: Option<Box<Expr>>,
}

// `$` cannot occur in a source identifier in either flavor.
const ACTION: &str = "$handler_action";
const FINALIZED_ANSWER: &str = "$finalized_answer";

/// Build a reusable handler taking a zero-argument computation.
/// Implements [EFFECTS-HANDLER-VALUE] and [FLAVOR-HANDLER-VALUE].
#[must_use]
pub fn handler_value(
    stage: Stage,
    effect: String,
    arms: Vec<HandlerArm>,
    clauses: Clauses,
    position: Option<Position>,
) -> Expr {
    Expr::Lambda {
        parameters: vec![action_parameter()],
        return_type: None,
        body: Box::new(handled_action(stage, effect, arms, clauses, position)),
        position,
    }
}

/// A named handler function over a zero-argument computation: the module
/// idiom `fn name(work) = { handle E { … }\n work() }`, which a module may
/// export where a module `let` must be a compile-time constant.
/// Implements [FLAVOR-PYTHON-HANDLER].
#[must_use]
pub fn handler_function(
    name: String,
    stage: Stage,
    effect: String,
    arms: Vec<HandlerArm>,
    position: Option<Position>,
) -> Stmt {
    Stmt::Function {
        name,
        type_params: Vec::new(),
        parameters: vec![action_parameter()],
        return_type: None,
        effects: Vec::new(),
        effect_tail: None,
        effect_row_present: false,
        body: handled_action(stage, effect, arms, Clauses::default(), position),
        doc: None,
        position,
    }
}

fn action_parameter() -> Parameter {
    Parameter {
        name: ACTION.into(),
        ty: None,
        inline_constraint: false,
    }
}

/// The handler installed over the invocation of its computation.
fn handled_action(
    stage: Stage,
    effect: String,
    arms: Vec<HandlerArm>,
    clauses: Clauses,
    position: Option<Position>,
) -> Expr {
    Expr::Handler {
        stage,
        effect,
        arms,
        return_clause: clauses.return_clause,
        finally: clauses.finally,
        body: Box::new(Expr::Call {
            function: Box::new(Expr::Identifier(ACTION.into())),
            arguments: Vec::new(),
            named_arguments: Vec::new(),
        }),
        position,
    }
}

/// `answer` followed by its activation's `cleanup`: the answer is computed
/// first (after any return clause), the `Unit` cleanup runs, and the answer is
/// the block's value. A discharged static region has no runtime activation to
/// abandon, so this sequence is its whole finalization. Implements
/// [EFFECTS-FINALIZATION].
pub(crate) fn finalized(answer: Expr, cleanup: Expr) -> Expr {
    Expr::Block {
        statements: vec![
            Stmt::Let {
                name: FINALIZED_ANSWER.into(),
                mutable: false,
                ty: None,
                value: answer,
                doc: None,
                position: None,
            },
            Stmt::Expr {
                value: cleanup,
                doc: None,
                position: None,
            },
        ],
        value: Some(Box::new(Expr::Identifier(FINALIZED_ANSWER.into()))),
        position: None,
    }
}
