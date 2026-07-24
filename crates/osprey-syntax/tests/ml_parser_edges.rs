//! Integration coverage for the ML parser's edge and recovery paths: the
//! declaration forms that carry their own delimiter diagnostics (`extern`,
//! `effect`, signatures, effect rows), the literal/postfix expression forms,
//! and every pattern spelling plus the two shapes the flavor deliberately
//! refuses (nested constructor patterns, clause heads on a lambda).
//!
//! Each malformed case asserts the *message*, not just that something failed,
//! so a recovery path that silently swallows the token is still caught.
#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test assertions: a failed match or out-of-bounds index is a test failure, not a production panic"
)]

use osprey_ast::{Expr, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Parse ML source, asserting a clean parse, and return the statements.
fn ml_ok(src: &str) -> Vec<Stmt> {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "unexpected ml errors: {parsed:#?}"
    );
    parsed.program.statements
}

/// The concatenated diagnostics of a source that must be rejected.
fn ml_errors(src: &str) -> String {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        !parsed.errors.is_empty(),
        "expected errors but got none: {:#?}",
        parsed.program
    );
    parsed
        .errors
        .iter()
        .map(|e| e.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert every listed diagnostic fragment appears among `src`'s errors.
fn rejects(src: &str, expected: &[&str]) {
    let errors = ml_errors(src);
    for fragment in expected {
        assert!(
            errors.contains(fragment),
            "missing {fragment:?} in {errors}"
        );
    }
}

/// The value bound by the single top-level binding in `src`.
fn value(src: &str) -> Expr {
    match ml_ok(src).into_iter().next() {
        Some(Stmt::Let { value, .. } | Stmt::Expr { value, .. }) => value,
        other => panic!("expected a bound value, got {other:?}"),
    }
}

// ─── declaration delimiters ─────────────────────────────────────────────────

#[test]
fn extern_declarations_report_their_missing_delimiters() {
    // A well-formed declaration first, so the diagnostics below are about the
    // delimiter and not about `extern` being unsupported.
    assert_eq!(
        ml_ok("extern puts (s : string) -> int\n").len(),
        1,
        "a well-formed extern must parse"
    );
    rejects(
        "extern puts (s string) -> int\n",
        &["expected ':' in extern parameter"],
    );
    rejects("extern puts (s : string -> int\n", &["expected ')'"]);
}

#[test]
fn effect_operations_report_their_missing_delimiters() {
    let stmts = ml_ok("effect State T\n    get : Unit => T\n    set : T => Unit\n");
    match stmts.into_iter().next() {
        Some(Stmt::Effect {
            name,
            type_params,
            operations,
            ..
        }) => {
            assert_eq!(name, "State");
            assert_eq!(type_params.len(), 1);
            assert_eq!(operations.len(), 2);
        }
        other => panic!("expected an effect declaration, got {other:?}"),
    }
    rejects(
        "effect Log\n    write string => Unit\n",
        &["expected ':' in effect operation"],
    );
    rejects(
        "effect Log\n    write : string Unit\n",
        &["expected '=>' in effect operation"],
    );
}

/// A signature is only ever entered through a lookahead that has already seen
/// the `:`, so the colon is never missing; a line without one is an ordinary
/// item and must be dispatched as such ([FLAVOR-ML-GENERICS]).
#[test]
fn a_signature_is_recognised_only_with_its_colon() {
    assert_eq!(
        ml_ok("pick<T, U> : T -> U -> T\npick a b = a\n").len(),
        1,
        "a signature attaches to its binding rather than standing alone"
    );
    assert_eq!(
        ml_ok("inc : int -> int\ninc x = x + 1\n").len(),
        1,
        "the plain signature form attaches too"
    );
    // `less<T` is a comparison, not a binder, so the generic lookahead must
    // decline it rather than commit to a signature.
    assert!(matches!(
        value("flag = less < 3\n"),
        Expr::Binary { ref op, .. } if op == "<"
    ));
}

#[test]
fn a_bracketed_effect_row_needs_its_closing_bracket() {
    assert_eq!(ml_ok("run : Unit -> int ![State, Log]\nrun = 1\n").len(), 1);
    rejects(
        "run : Unit -> int ![State, Log\nrun = 1\n",
        &["expected ']' to close effect row"],
    );
}

/// A `type` block's field and variant lines each recover independently, so one
/// malformed line does not swallow the rest of the block.
#[test]
fn a_type_block_recovers_line_by_line() {
    // The first field's colon is what makes the block a record, so the missing
    // one has to be on a later line.
    rejects(
        "type Point =\n    x : int\n    y int\n",
        &["expected ':' in type field"],
    );
    // A variant line that is not an identifier at all still terminates.
    let recovered = ml_errors("type Choice =\n    1\n    Real\n");
    assert!(
        !recovered.is_empty(),
        "a non-identifier variant must be reported"
    );
}

// ─── literals and postfix forms ─────────────────────────────────────────────

#[test]
fn list_literals_span_empty_populated_and_trailing_comma() {
    assert_eq!(value("xs = []\n"), Expr::List(vec![]));
    assert_eq!(
        value("xs = [1, 2, 3]\n"),
        Expr::List(vec![Expr::Integer(1), Expr::Integer(2), Expr::Integer(3)])
    );
    assert_eq!(
        value("xs = [1, 2,]\n"),
        Expr::List(vec![Expr::Integer(1), Expr::Integer(2)])
    );
    rejects("xs = [1, 2\n", &["expected ']'"]);
}

/// `[=>]` is the empty map; `[k => v]` the populated one ([FLAVOR-ML-MAP]).
#[test]
fn map_literals_span_empty_and_populated() {
    assert!(matches!(value("m = [=>]\n"), Expr::Map(entries) if entries.is_empty()));
    assert!(
        matches!(value("m = [\"a\" => 1, \"b\" => 2]\n"), Expr::Map(entries) if entries.len() == 2)
    );
}

/// A bracket glued to its receiver indexes it; a bracket after whitespace
/// starts a fresh list argument.
#[test]
fn a_glued_bracket_indexes_its_receiver() {
    assert!(
        matches!(value("first = [10, 20][0]\n"), Expr::Index { .. }),
        "a glued bracket must index"
    );
    rejects("first = [10, 20][0\n", &["expected ']'"]);
}

#[test]
fn floats_bools_and_strings_all_reach_the_ast() {
    assert_eq!(value("x = 1.5\n"), Expr::Float(1.5));
    assert_eq!(value("x = true\n"), Expr::Bool(true));
    assert_eq!(value("x = false\n"), Expr::Bool(false));
    assert_eq!(value("x = \"hi\"\n"), Expr::Str("hi".to_owned()));
}

/// `::` qualification is retained verbatim, and a dangling `::` is reported
/// rather than silently dropped.
#[test]
fn qualified_names_keep_their_written_path() {
    assert!(
        matches!(value("x = Ledger::open\n"), Expr::Path(path) if path.segments.len() == 2),
        "a qualified reference keeps both segments"
    );
    rejects("x = Ledger::\n", &["expected path segment after '::'"]);
}

/// Trailing commas are tolerated in an argument list, matching the literal
/// forms, so reordering arguments over several edits never fails to parse.
#[test]
fn a_call_tolerates_a_trailing_comma_and_reports_a_missing_paren() {
    assert!(matches!(value("x = max(1, 2,)\n"), Expr::Call { .. }));
    rejects("x = max(1, 2\n", &["expected ')'"]);
}

// ─── patterns ───────────────────────────────────────────────────────────────

/// Every literal pattern spelling, including the negative integer the lexer
/// splits into a sign and a magnitude.
#[test]
fn match_covers_every_literal_pattern_spelling() {
    let src = "describe v = match v\n    -5 => \"neg\"\n    0 => \"zero\"\n    \
               true => \"yes\"\n    false => \"no\"\n    \"x\" => \"letter\"\n    _ => \"other\"\n";
    let arms = match ml_ok(src).into_iter().next() {
        Some(Stmt::Function { body, .. }) => match body {
            Expr::Match { arms, .. } => arms,
            other => panic!("expected a match, got {other:?}"),
        },
        other => panic!("expected a function, got {other:?}"),
    };
    assert_eq!(arms.len(), 6);
    assert_eq!(
        arms[0].pattern,
        osprey_ast::Pattern::Literal(Box::new(Expr::Integer(-5)))
    );
}

#[test]
fn a_match_arm_needs_its_fat_arrow() {
    rejects(
        "describe v = match v\n    0 \"zero\"\n",
        &["expected '=>' in match arm"],
    );
}

/// A payload is bound one level deep; nesting has no fall-through spelling, so
/// it is refused with the workaround in the message.
#[test]
fn nested_constructor_patterns_are_refused() {
    let src = "type Tree = Leaf | Node Tree Tree\n\
               depth t = match t\n    Node (Node l r) x => 1\n    _ => 0\n";
    rejects(src, &["nested constructor patterns are not supported"]);
}

/// A lambda has nowhere to put the alternative arms, so a clause head on one is
/// refused rather than half-supported ([FLAVOR-ML-CLAUSES]).
#[test]
fn a_lambda_head_takes_plain_parameters_only() {
    rejects(
        "f = \\0 => 1\n",
        &["a lambda head takes plain parameters; use 'match' to select on a pattern"],
    );
    rejects("f = \\x x + 1\n", &["expected '=>' in lambda"]);
}

// ─── containers and the Result default ──────────────────────────────────────

/// An indented `namespace` owns exactly its block; a bare one owns every
/// declaration that follows it in the file ([MODULES-NAMESPACE]).
#[test]
fn a_namespace_owns_a_block_or_the_rest_of_the_file() {
    let blocked = ml_ok("namespace Ledger\n    balance = 1\n\ntotal = 2\n");
    match blocked.first() {
        Some(Stmt::Namespace {
            body, file_scoped, ..
        }) => {
            assert!(!file_scoped, "an indented namespace is not file-scoped");
            assert_eq!(body.len(), 1, "only the indented declaration is owned");
        }
        other => panic!("expected a namespace, got {other:?}"),
    }
    assert_eq!(blocked.len(), 2, "the dedented declaration stays a sibling");

    let file = ml_ok("namespace Ledger\nbalance = 1\ntotal = 2\n");
    match file.as_slice() {
        [Stmt::Namespace {
            body, file_scoped, ..
        }] => {
            assert!(file_scoped, "a bare namespace is file-scoped");
            assert_eq!(body.len(), 2, "every later declaration is owned");
        }
        other => panic!("expected one file-scoped namespace, got {other:?}"),
    }
}

/// `e ?: d` lowers to the same two-arm boolean `match` the Default flavor's
/// ternary emits — a different node here would break [FLAVOR-IR-EQUIV].
#[test]
fn the_result_default_lowers_to_a_boolean_match() {
    match value("r = risky ?: 0\n") {
        Expr::Match { value, arms } => {
            assert_eq!(*value, Expr::Identifier("risky".to_owned()));
            assert_eq!(arms.len(), 2);
            assert_eq!(
                arms[0].pattern,
                osprey_ast::Pattern::Literal(Box::new(Expr::Bool(true)))
            );
            assert_eq!(arms[0].body, Expr::Identifier("risky".to_owned()));
            assert_eq!(
                arms[1].pattern,
                osprey_ast::Pattern::Literal(Box::new(Expr::Bool(false)))
            );
            assert_eq!(arms[1].body, Expr::Integer(0));
        }
        other => panic!("expected a boolean match, got {other:?}"),
    }
}

// ─── effects ────────────────────────────────────────────────────────────────

/// `resume ()` resumes with unit; a bare `resume` on its own line means the
/// same thing, and an argument resumes with that value ([FLAVOR-ML-EFFECT]).
#[test]
fn resume_spans_its_unit_and_valued_forms() {
    let handler = |arm: &str| {
        format!("effect Ask\n    get : int => int\n\nmain =\n    handle Ask\n        get tag => {arm}\n    in perform Ask.get 1\n")
    };
    for arm in ["resume ()", "resume", "resume (tag * 10)"] {
        assert_eq!(
            ml_ok(&handler(arm)).len(),
            2,
            "the effect and its handler must both parse for arm {arm:?}"
        );
    }
}
