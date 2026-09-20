//! Count continuation uses along syntactic control paths. Operation declarations
//! determine continuation ownership; this module never selects an arm mode.
//! Implements [MULTI-HANDLE-ONCE].

use crate::{AstNode, Expr, Stmt};

/// How many `resume` sites the longest single control path through `body`
/// crosses, counting only those belonging to the ENCLOSING handler arm.
///
/// Sequences add uses; mutually exclusive branches take their maximum.
/// Lambda bodies have their own continuation scope. This local check does not
/// establish safety for escaping or reusable continuation values.
#[must_use]
pub fn resumes_on_one_path(body: &Expr) -> u32 {
    match body {
        // A `resume` whose own argument resumes is two on one path.
        Expr::Resume(value) => 1 + value.as_deref().map_or(0, resumes_on_one_path),
        // Branch positions: only one arm runs, so the worst path is the worst arm.
        Expr::Match { value, arms } => {
            resumes_on_one_path(value)
                + arms
                    .iter()
                    .map(|a| resumes_on_one_path(&a.body))
                    .max()
                    .unwrap_or(0)
        }
        Expr::Select { arms } => arms
            .iter()
            .map(|arm| resumes_on_one_path(&arm.body))
            .max()
            .unwrap_or(0),
        // A lambda body is not on the arm's own control path. `resume` there is
        // rejected by [EFFECTS-RESUME] with a message about the dead
        // continuation, which is the accurate complaint; counting it here would
        // report the same defect twice under a rule it does not violate.
        Expr::Lambda { .. } => 0,
        // A nested handler's arms own their own `resume`s; only its handled
        // body sits on this arm's path.
        Expr::Handler { body, .. } => resumes_on_one_path(body),
        // Everything else composes sequentially: every child is crossed.
        _ => sequential_children(body),
    }
}

/// Sum the path length of every child evaluated on the way through `body`.
fn sequential_children(body: &Expr) -> u32 {
    let mut total = 0;
    AstNode::Expression(body).for_each_child(|child| {
        total += match child {
            AstNode::Statement(statement) => statement_resumes(statement),
            AstNode::Expression(expression) => resumes_on_one_path(expression),
        };
    });
    total
}

fn statement_resumes(stmt: &Stmt) -> u32 {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            resumes_on_one_path(value)
        }
        _ => 0,
    }
}
