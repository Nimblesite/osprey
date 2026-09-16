//! Test-only parse helpers shared by both flavors' unit tests.
//!
//! Every frontend test opens the same way: parse a source string, fail loudly on
//! any syntax error, then work with the lowered statements. Each `mod tests` used
//! to carry its own copy of that preamble, so the copies drifted apart on their
//! failure messages while asserting the same thing. The flavor is the only real
//! difference, and [`Parsed`] already names itself, so one pair of helpers per
//! flavor covers the crate.

use crate::Parsed;
use osprey_ast::{Program, Stmt};

/// The statements of a clean **Default**-flavor parse.
pub(crate) fn stmts(src: &str) -> Vec<Stmt> {
    clean_statements(crate::parse_program(src))
}

/// The single statement of a clean Default-flavor parse.
pub(crate) fn one_stmt(src: &str) -> Stmt {
    only(stmts(src))
}

/// The statements of a clean **ML**-flavor parse.
pub(crate) fn ml_stmts(src: &str) -> Vec<Stmt> {
    clean_statements(crate::ml::parse_ml(src))
}

/// The single statement of a clean ML-flavor parse.
pub(crate) fn ml_one_stmt(src: &str) -> Stmt {
    only(ml_stmts(src))
}

/// The whole **Default**-flavor [`Program`] of a clean parse. Inner docs
/// (`//!`) attach to the program itself, so those tests need the program and
/// not just its statements ([DOC-SIGIL-INNER]).
pub(crate) fn program(src: &str) -> Program {
    clean_program(crate::parse_program(src))
}

/// The whole **ML**-flavor [`Program`] of a clean parse. Twin of [`program`].
pub(crate) fn ml_program(src: &str) -> Program {
    clean_program(crate::ml::parse_ml(src))
}

/// Statements of a parse that must be error-free. The flavor names itself in the
/// failure so the message says which frontend rejected the source.
fn clean_statements(parsed: Parsed) -> Vec<Stmt> {
    clean_program(parsed).statements
}

/// The program of a parse that must be error-free, named by flavor on failure.
fn clean_program(parsed: Parsed) -> Program {
    assert!(
        parsed.errors.is_empty(),
        "{} errors: {:?}",
        parsed.flavor,
        parsed.errors
    );
    parsed.program
}

/// The one statement a single-declaration source must lower to.
fn only(mut statements: Vec<Stmt>) -> Stmt {
    assert_eq!(
        statements.len(),
        1,
        "expected exactly one statement: {statements:?}"
    );
    // `remove(0)` is panic-free given the length assertion above and avoids the
    // repository-forbidden `unwrap()`.
    statements.remove(0)
}

/// The doc comment lowered onto a statement, or `None`. Both flavors' tests
/// ask this same question of the same `Stmt`, so it is answered once.
pub(crate) fn stmt_doc(stmt: &Stmt) -> Option<&osprey_ast::DocComment> {
    match stmt {
        Stmt::Expr { doc, .. } | Stmt::Let { doc, .. } | Stmt::Function { doc, .. } => doc.as_ref(),
        _ => None,
    }
}

/// Assert the doc summary lowered onto `stmt` — `None` demanding that no doc
/// was invented for it. Every doc-lowering case asks exactly this.
pub(crate) fn assert_summary(stmt: &Stmt, expected: Option<&str>) {
    assert_eq!(stmt_doc(stmt).map(|d| d.summary.as_str()), expected);
}

/// Assert a documented expression statement is followed by a documented
/// declaration that kept its OWN doc — the shape both flavors pin, differing
/// only in how each spells the source.
pub(crate) fn assert_doc_pair(all: &[Stmt], first: &str, second: &str) {
    let [statement, declaration] = all else {
        panic!(
            "expected exactly two statements, got {}: {all:?}",
            all.len()
        );
    };
    assert_summary(statement, Some(first));
    assert!(matches!(declaration, Stmt::Function { .. }));
    assert_summary(declaration, Some(second));
}
