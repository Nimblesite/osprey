//! Test-only parse helpers shared by both flavors' unit tests.
//!
//! Every frontend test opens the same way: parse a source string, fail loudly on
//! any syntax error, then work with the lowered statements. Each `mod tests` used
//! to carry its own copy of that preamble, so the copies drifted apart on their
//! failure messages while asserting the same thing. The flavor is the only real
//! difference, and [`Parsed`] already names itself, so one pair of helpers per
//! flavor covers the crate.

use crate::Parsed;
use osprey_ast::Stmt;

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

/// Statements of a parse that must be error-free. The flavor names itself in the
/// failure so the message says which frontend rejected the source.
fn clean_statements(parsed: Parsed) -> Vec<Stmt> {
    assert!(
        parsed.errors.is_empty(),
        "{} errors: {:?}",
        parsed.flavor,
        parsed.errors
    );
    parsed.program.statements
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
