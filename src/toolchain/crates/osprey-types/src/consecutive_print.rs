//! Consecutive prints. Implements [TYPE-WARNINGS-CONSECUTIVE-PRINT].
//!
//! Osprey writes one interpolated `print` where other languages write a run
//! of them: the output is one value the reader sees whole, and one call is
//! one effect. Two or more `print` calls in a row, with nothing between them,
//! are reported once, at the first of the run.

use osprey_ast::{walk_program, AstVisitor, Expr, Position, Program, Stmt};

use crate::TypeWarning;

/// Shared rule identifier for compiler and editor diagnostics.
pub const CONSECUTIVE_PRINT: &str = "consecutive-print";

/// The builtin this rule watches.
const PRINT: &str = "print";

/// One step of a statement sequence: a `print` call where it was written,
/// or anything else, which ends a run.
#[derive(Clone, Copy)]
enum Step {
    Print(Option<Position>),
    Other,
}

/// Every run of two or more `print` calls in `program`, in source order.
#[must_use]
pub fn consecutive_prints(program: &Program) -> Vec<TypeWarning> {
    let mut runs = Runs::default();
    runs.sequence(program.statements.iter().map(statement_step));
    walk_program(program, &mut runs);
    runs.warnings
        .sort_by_key(|warning| warning.position.map(|at| (at.line, at.column)));
    runs.warnings
}

#[derive(Default)]
struct Runs {
    warnings: Vec<TypeWarning>,
}

impl AstVisitor for Runs {
    fn statement(&mut self, statement: &Stmt) {
        if let Stmt::Namespace { body, .. } = statement {
            self.sequence(body.iter().map(statement_step));
        }
    }

    fn expression(&mut self, expression: &Expr) {
        if let Expr::Block {
            statements,
            value,
            position,
        } = expression
        {
            let trailing = value
                .as_deref()
                .map(|value| expression_step(value, *position));
            self.sequence(statements.iter().map(statement_step).chain(trailing));
        }
    }
}

impl Runs {
    /// Report each run of two or more prints in one sequence.
    fn sequence(&mut self, steps: impl Iterator<Item = Step>) {
        let mut run: Vec<Option<Position>> = Vec::new();
        for step in steps {
            match step {
                Step::Print(position) => run.push(position),
                Step::Other => self.close(&mut run),
            }
        }
        self.close(&mut run);
    }

    fn close(&mut self, run: &mut Vec<Option<Position>>) {
        if run.len() >= 2 {
            self.warnings.push(TypeWarning {
                message: format!(
                    "{} `print` calls in a row: join them into one interpolated `print`",
                    run.len()
                ),
                position: run.first().copied().flatten(),
                rule: CONSECUTIVE_PRINT,
            });
        }
        run.clear();
    }
}

fn statement_step(statement: &Stmt) -> Step {
    match statement {
        Stmt::Expr {
            value, position, ..
        } => expression_step(value, *position),
        _ => Step::Other,
    }
}

fn expression_step(expression: &Expr, position: Option<Position>) -> Step {
    match expression {
        Expr::Call { function, .. } if matches!(function.as_ref(), Expr::Identifier(name) if name == PRINT) => {
            Step::Print(position)
        }
        _ => Step::Other,
    }
}
