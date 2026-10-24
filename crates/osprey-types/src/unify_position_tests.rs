//! Unify diagnostics carry source positions.
//!
//! Every "cannot unify" error used to print with no `line:col` at all — the
//! CLI's `path: message` shape instead of `path:line:col: message` — so a
//! mismatch in a large program said nothing about where to look. A unify
//! failure now carries the position of the expression being inferred when it
//! failed, and a deferred-arithmetic site carries the position of the
//! operator that wrote it.

use crate::testutil::typecheck;
use osprey_ast::Position;
use osprey_syntax::Flavor;

/// The `+` that mixes an int with a string is where the failure is reported.
#[test]
fn a_mixed_binary_operand_names_its_operator() {
    let errors = typecheck(
        Flavor::Default,
        "let a = 1\nlet b = \"x\"\nlet c = a + b\nprint(toString(c))",
    );
    let mismatch = errors
        .iter()
        .find(|e| e.message.contains("cannot unify"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(mismatch.position.unwrap().line, 3);
}

/// A call argument that cannot unify with its parameter names the statement
/// that wrote the call.
#[test]
fn a_call_argument_mismatch_names_its_statement() {
    let errors = typecheck(
        Flavor::Default,
        "fn id(x: int) = x\nprint(toString(id(1.5)))",
    );
    let mismatch = errors
        .iter()
        .find(|e| e.message.contains("cannot unify"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(mismatch.position.unwrap().line, 2);
}

/// A deferred site that still conflicts — an integer call against a float
/// comparison — names the operator whose overload conflicted.
#[test]
fn a_deferred_site_conflict_names_its_operator() {
    let errors = typecheck(
        Flavor::Default,
        "fn f(a, b) = (a - b) <= 1.0\nprint(toString(f(1, 2)))",
    );
    let mismatch = errors
        .iter()
        .find(|e| e.message.contains("cannot unify"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(
        mismatch.position,
        Some(Position {
            line: 1,
            column: 16
        })
    );
}

/// A violated `abs` obligation names the expression that wrote the call, not
/// nothing at all.
#[test]
fn an_abs_obligation_names_its_expression() {
    let errors = typecheck(Flavor::Default, "let x = abs(\"text\")\nprint(toString(x))");
    let violation = errors
        .iter()
        .find(|e| e.message.contains("`abs` requires int or float"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(violation.position.unwrap().line, 1);
}

/// The ML flavor gets the same positions: the twins must agree.
#[test]
fn the_ml_flavor_carries_the_same_positions() {
    let errors = typecheck(
        Flavor::Ml,
        "a = 1\nb = \"x\"\nc = a + b\nprint (toString c)",
    );
    let mismatch = errors
        .iter()
        .find(|e| e.message.contains("cannot unify"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(mismatch.position.unwrap().line, 3);
}
