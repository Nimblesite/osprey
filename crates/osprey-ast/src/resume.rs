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

use crate::{Expr, InterpolatedPart, Stmt};

/// True when `e` contains a `resume` belonging to the ENCLOSING handler arm —
/// a nested handler's body owns its own `resume`s, so they don't count.
#[must_use]
pub fn contains_resume(e: &Expr) -> bool {
    match e {
        Expr::Resume(_) => true,
        Expr::InterpolatedStr(parts) => parts
            .iter()
            .any(|p| matches!(p, crate::InterpolatedPart::Expr(inner) if contains_resume(inner))),
        Expr::List(xs, _) => xs.iter().any(contains_resume),
        Expr::Map(entries) => entries
            .iter()
            .any(|entry| contains_resume(&entry.key) || contains_resume(&entry.value)),
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => fields.iter().any(|f| contains_resume(&f.value)),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            contains_resume(left) || contains_resume(right)
        }
        Expr::TypeApply {
            function: operand, ..
        }
        | Expr::Unary { operand, .. } => contains_resume(operand),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            contains_resume(function)
                || arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            contains_resume(target)
                || arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        Expr::FieldAccess { target, .. } => contains_resume(target),
        Expr::Index { target, index } => contains_resume(target) || contains_resume(index),
        Expr::Lambda { body, .. } | Expr::Spawn(body) | Expr::Await(body) | Expr::Recv(body) => {
            contains_resume(body)
        }
        Expr::Yield(Some(value)) => contains_resume(value),
        Expr::Send { channel, value } => contains_resume(channel) || contains_resume(value),
        Expr::Match { value, arms } => {
            contains_resume(value) || arms.iter().any(|arm| contains_resume(&arm.body))
        }
        Expr::Block { statements, value } => {
            statements.iter().any(stmt_contains_resume)
                || value.as_deref().is_some_and(contains_resume)
        }
        Expr::Select { arms } => arms.iter().any(|arm| contains_resume(&arm.body)),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        // A nested handler owns its own `resume`; do not mark the outer handler
        // as a resuming region because of it.
        Expr::Handler { body, .. } => contains_resume(body),
        _ => false,
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
    let each = |xs: &[Expr]| xs.iter().map(resumes_on_one_path).sum();
    match body {
        Expr::InterpolatedStr(parts) => parts
            .iter()
            .map(|part| match part {
                InterpolatedPart::Expr(inner) => resumes_on_one_path(inner),
                InterpolatedPart::Text(_) => 0,
            })
            .sum(),
        Expr::List(values, _) => each(values),
        Expr::Map(entries) => entries
            .iter()
            .map(|entry| resumes_on_one_path(&entry.key) + resumes_on_one_path(&entry.value))
            .sum(),
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => fields.iter().map(|f| resumes_on_one_path(&f.value)).sum(),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            resumes_on_one_path(left) + resumes_on_one_path(right)
        }
        Expr::TypeApply {
            function: operand, ..
        }
        | Expr::Unary { operand, .. } => resumes_on_one_path(operand),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            resumes_on_one_path(function)
                + each(arguments)
                + named_arguments
                    .iter()
                    .map(|n| resumes_on_one_path(&n.value))
                    .sum::<u32>()
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            resumes_on_one_path(target)
                + each(arguments)
                + named_arguments
                    .iter()
                    .map(|n| resumes_on_one_path(&n.value))
                    .sum::<u32>()
        }
        Expr::FieldAccess { target, .. } => resumes_on_one_path(target),
        Expr::Index { target, index } => resumes_on_one_path(target) + resumes_on_one_path(index),
        Expr::Spawn(inner) | Expr::Await(inner) | Expr::Recv(inner) => resumes_on_one_path(inner),
        Expr::Yield(value) => value.as_deref().map_or(0, resumes_on_one_path),
        Expr::Send { channel, value } => resumes_on_one_path(channel) + resumes_on_one_path(value),
        Expr::Block { statements, value } => {
            statements.iter().map(statement_resumes).sum::<u32>()
                + value.as_deref().map_or(0, resumes_on_one_path)
        }
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            each(arguments)
                + named_arguments
                    .iter()
                    .map(|n| resumes_on_one_path(&n.value))
                    .sum::<u32>()
        }
        _ => 0,
    }
}

fn statement_resumes(stmt: &Stmt) -> u32 {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            resumes_on_one_path(value)
        }
        _ => 0,
    }
}
