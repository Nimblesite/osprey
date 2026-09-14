//! Whether an expression performs an explicit `resume` — the property that
//! splits handler-arm semantics: a resuming arm's value is the handler's
//! ANSWER, a non-resuming arm's value substitutes for the operation's RESULT.
//! Shared by the type checker (arm typing) and codegen (arm emission).
//!
//! Two questions about the same sites live here because they read the same
//! tree and must not drift: whether an arm resumes AT ALL, which decides its
//! mode, and how many times the worst single control path through it does,
//! which is the affine rule multiplicity enforces.
//! Implements [EFFECTS-RESUME], [MULTI-HANDLE-ONCE].

use crate::{AstNode, Expr, Stmt};

/// True when `e` contains a `resume` belonging to the ENCLOSING handler arm —
/// a nested handler's body owns its own `resume`s, so they don't count.
#[must_use]
pub fn contains_resume(e: &Expr) -> bool {
    match e {
        Expr::Resume(_) => true,
        // Only the handled body belongs to the enclosing arm.
        Expr::Handler { body, .. } => contains_resume(body),
        _ => {
            let mut found = false;
            AstNode::Expression(e).for_each_child(|child| {
                found = found
                    || match child {
                        AstNode::Statement(statement) => stmt_contains_resume(statement),
                        AstNode::Expression(expression) => contains_resume(expression),
                    };
            });
            found
        }
    }
}

fn stmt_contains_resume(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            contains_resume(value)
        }
        _ => false,
    }
}

/// How many `resume` sites the longest single control path through `body`
/// crosses, counting only those belonging to the ENCLOSING handler arm.
///
/// This is deliberately not [`contains_resume`](crate::contains_resume), which
/// asks a branch-blind "any" question to decide an arm's *mode*. The affine
/// rule is a branch-AWARE question about one path: two `resume`s in sequence
/// are a violation, two on different `match` branches are not, and reading them
/// the same way would reject `tests/regressions/effects/abort_vs_resume.test.osp`.
/// Osprey has no loop construct ([BUILTIN-ITER]), so sequence and branch are the
/// only two shapes this fold needs. Implements [MULTI-HANDLE-ONCE].
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
