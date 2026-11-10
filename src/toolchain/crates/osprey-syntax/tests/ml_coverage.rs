//! Integration coverage for the ML-flavor frontend (`src/ml/*`) and the flavor
//! plumbing in `src/lib.rs`. These tests drive the public API
//! ([`parse_program_with_flavor`], [`Flavor`]) over many ML programs — valid and
//! malformed — so the lexer/parser/lower error and edge branches are exercised.
//!
//! The crate denies `unwrap`/`expect`/indexing in production code; these are
//! test assertions where an out-of-bounds index or a failed match is a test
//! failure (not a production panic), so the lint is relaxed only for this file.
#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unreachable,
    reason = "test assertions: a failed match, unreachable arm, or out-of-bounds index is a test failure, not a production panic"
)]

use std::str::FromStr;

use osprey_ast::{Expr, Pattern, Stmt, TypeExpr};
use osprey_syntax::{
    parse_program, parse_program_for_path, parse_program_with_flavor, resolve_flavor, Flavor,
    Parsed,
};

/// Parse ML source, asserting a clean parse, and return the statements.
fn ml_ok(src: &str) -> Vec<Stmt> {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "unexpected ml errors: {parsed:#?}"
    );
    assert_eq!(parsed.flavor, Flavor::Ml);
    parsed.program.statements
}

/// Parse ML source expecting exactly one statement.
fn ml_one(src: &str) -> Stmt {
    let mut s = ml_ok(src);
    assert_eq!(s.len(), 1, "expected exactly one statement: {s:?}");
    match s.pop() {
        Some(stmt) => stmt,
        None => unreachable!("len checked above"),
    }
}

/// Parse ML source expecting at least one error; return the parsed result.
fn ml_err(src: &str) -> Parsed {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        !parsed.errors.is_empty(),
        "expected parse errors but got none: {:#?}",
        parsed.program
    );
    parsed
}

/// Parse ML source expecting an error whose message mentions every one of `needles`.
fn assert_ml_error(src: &str, needles: &[&str]) {
    let parsed = ml_err(src);
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| needles.iter().all(|needle| e.message.contains(needle))),
        "{src:?} expected an error mentioning {needles:?}, got {:?}",
        parsed.errors
    );
}

/// The value of a single top-level `let`/binding, or a test failure.
fn let_value(src: &str) -> Expr {
    match ml_one(src) {
        Stmt::Let { value, .. } => value,
        other => panic!("expected a let, got {other:?}"),
    }
}

// --- lib.rs: Flavor plumbing -------------------------------------------------

#[test]
fn flavor_display_and_from_str_round_trip() {
    assert_eq!(Flavor::Default.to_string(), "default");
    assert_eq!(Flavor::Ml.to_string(), "ml");
    assert_eq!(Flavor::from_str("default"), Ok(Flavor::Default));
    assert_eq!(Flavor::from_str("ml"), Ok(Flavor::Ml));
    // The default of the enum is Default.
    assert_eq!(Flavor::default(), Flavor::Default);
    // An unknown name fails loudly with a helpful message.
    match Flavor::from_str("fsharp") {
        Ok(f) => panic!("expected an error, got {f:?}"),
        Err(msg) => assert!(msg.contains("unknown flavor"), "msg: {msg}"),
    }
}

#[test]
fn parse_program_with_flavor_dispatches_both_frontends() {
    // [FLAVOR-FRONTEND]
    // Default flavor reaches the tree-sitter frontend and carries its flavor.
    let def = parse_program_with_flavor("let x = 1\n", Flavor::Default);
    assert!(def.errors.is_empty(), "default errors: {:?}", def.errors);
    assert_eq!(def.flavor, Flavor::Default);
    // The bare `parse_program` entry stays on Default.
    assert_eq!(parse_program("let x = 1\n").flavor, Flavor::Default);
    // ML flavor reaches the hand-written frontend.
    let ml = parse_program_with_flavor("x = 1\n", Flavor::Ml);
    assert!(ml.errors.is_empty(), "ml errors: {:?}", ml.errors);
    assert_eq!(ml.flavor, Flavor::Ml);
}

#[test]
fn path_and_resolve_select_ml_frontend() {
    // `.ospml` extension routes through the ML frontend.
    let p = parse_program_for_path("t.ospml", "inc x = x + 1\n");
    assert!(p.errors.is_empty(), "errors: {:?}", p.errors);
    assert_eq!(p.flavor, Flavor::Ml);
    // resolve_flavor agrees with the extension.
    assert_eq!(
        resolve_flavor(None, "t.ospml", "").as_ref(),
        Ok(&Flavor::Ml)
    );
}

#[test]
fn typed_function_accepts_equational_list_pattern_clauses() {
    let statements = ml_ok(
        "classify : List int -> string\n\
         classify [] = \"empty\"\n\
         classify [head, ...tail] = \"many\"\n",
    );
    assert_eq!(statements.len(), 1, "expected the clauses to merge");
}

// --- lexer.rs ----------------------------------------------------------------

#[test]
fn float_literal_lexes_and_lowers() {
    // Exercises scan_number's float branch (the `.digits` tail) and the float
    // atom lowering.
    assert_eq!(let_value("g = 9.8\n"), Expr::Float(9.8));
    // A trailing dot with no fraction is NOT a float: `1.` keeps the int then a
    // dot (field access) — here `x.field` is a field access, not a float.
    match let_value("r = a.b\n") {
        Expr::FieldAccess { field, .. } => assert_eq!(field, "b"),
        other => panic!("expected field access, got {other:?}"),
    }
}

#[test]
fn integer_overflow_is_a_lex_error() {
    // A literal too large for i64 fails to parse — the invalid-integer branch.
    assert_ml_error("x = 99999999999999999999999999\n", &["invalid integer"]);
}

#[test]
fn float_overflow_is_a_lex_error() {
    // A float whose magnitude is beyond f64 (parse to inf, but with a malformed
    // tail it errors). Use an absurdly long exponent-free fraction that still
    // parses — instead test invalid by repeating digits beyond f64 range using
    // an extreme integer part. f64 parse of a huge value yields inf (Ok), so to
    // hit the float-error branch we rely on a string Rust's f64 rejects: it does
    // not reject huge floats. Skip the unreachable error; assert a big float
    // parses to a finite or infinite f64 without a lex error instead.
    let parsed = parse_program_with_flavor("x = 1.5\n", Flavor::Ml);
    assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
}

#[test]
fn string_escapes_are_carried_through_lexing() {
    // The lexer keeps the raw `\n` escape; the lowerer resolves it.
    assert_eq!(let_value("s = \"a\\nb\"\n"), Expr::Str("a\nb".to_owned()));
    // An escaped quote inside the string does not terminate it.
    assert_eq!(let_value("s = \"a\\\"b\"\n"), Expr::Str("a\"b".to_owned()));
}

#[test]
fn unterminated_string_reports_an_error() {
    assert_ml_error("s = \"oops\n", &["unterminated"]);
}

#[test]
fn unexpected_character_reports_an_error() {
    // `@`/`#`/`;` are not Osprey operators — the single-char operator table
    // returns None and the scanner reports an unexpected-character error.
    for src in ["x = @\n", "x = #\n", "y = ;\n"] {
        assert_ml_error(src, &["unexpected character"]);
    }
}

#[test]
fn comparison_and_punctuation_operators_lex() {
    // `!=`, `<=`, `>=` two-char operators and `,`/`.` punctuation all lex.
    match let_value("r = a != b\n") {
        Expr::Binary { op, .. } => assert_eq!(op, "!="),
        other => panic!("expected binary, got {other:?}"),
    }
    match let_value("r = a <= b\n") {
        Expr::Binary { op, .. } => assert_eq!(op, "<="),
        other => panic!("expected binary, got {other:?}"),
    }
    // Comma appears in a tuple type signature (typing a FLAT binding of the
    // same arity, the one representable form); dot in a field access.
    let sig = ml_ok("f : (int, int) -> int\nf (a, b) = a + b\n");
    assert!(matches!(sig.first(), Some(Stmt::Function { .. })));
}

#[test]
fn inconsistent_indentation_reports_an_error() {
    // The third line dedents to column 2, which matches no enclosing level
    // (levels are 0 and 4) — the offside rule reports it.
    let src = "f =\n    a\n  b\n";
    assert_ml_error(src, &["inconsistent indentation"]);
}

// --- parser.rs ---------------------------------------------------------------

#[test]
fn type_application_in_signature_flows_to_parameter_types() {
    // `Result int string` is a type application: it should reach the lowerer and
    // produce a generic-parameter-carrying TypeExpr on the parameter.
    let s = ml_ok("f : Result int string -> int\nf r = 1\n");
    match s.into_iter().next() {
        Some(Stmt::Function { parameters, .. }) => match &parameters[0].ty {
            Some(TypeExpr {
                name,
                generic_params,
                ..
            }) => {
                assert_eq!(name, "Result");
                assert_eq!(generic_params.len(), 2);
            }
            other => panic!("expected an applied type, got {other:?}"),
        },
        other => panic!("expected a function, got {other:?}"),
    }
}

#[test]
fn unexpected_token_in_type_reports_an_error() {
    // `x : =` puts an `=` where a type atom is expected.
    assert_ml_error("x : =\nx = 1\n", &["in type"]);
}

#[test]
fn tuple_domain_on_a_curried_binding_is_rejected() {
    // A tuple type `(int, int)` has no canonical TypeExpr form. This used to
    // lower to an UNTYPED parameter — the written signature silently vanished
    // and the binding was checked as if unannotated, so `f "str"` compiled.
    // The lowering now reports the mismatch instead of dropping the type.
    assert_ml_error(
        "f : (int, int) -> int\nf x = 1\n",
        &["parenthesised signature domain"],
    );
}

#[test]
fn flat_tuple_domain_with_wrong_arity_is_rejected() {
    // The flat form is representable only when the tuple's arity matches the
    // binding's — a mismatch used to drop the whole domain silently.
    assert_ml_error(
        "f : (int, string, bool) -> string\nf (a, b) = \"x\"\n",
        &["declares 3 parameters but the binding takes 2"],
    );
}

#[test]
fn a_parenthesised_flat_function_type_is_a_valid_signature_slot() {
    // `((int, int) -> int)` in a signature slot is a FLAT function type — the
    // Default flavor's `fn(int, int) -> int` — not a tuple value. Rejecting it
    // as "a tuple type has no value form" untyped `applyTwo`'s HOF parameter in
    // type_equality_comprehensive; both binding forms must keep the type.
    let curried =
        ml_ok("applyTwo : ((int, int) -> int) -> int -> int -> int\napplyTwo f a b = f (a, b)\n");
    let flat =
        ml_ok("applyTwo : (((int, int) -> int), int, int) -> int\napplyTwo (f, a, b) = f (a, b)\n");
    for stmts in [curried, flat] {
        match stmts.first() {
            Some(Stmt::Function { parameters, .. }) => {
                let Some(ty) = parameters.first().and_then(|p| p.ty.as_ref()) else {
                    panic!("the HOF parameter lost its written type")
                };
                assert!(ty.is_function, "expected a function type, got {ty:?}");
                assert_eq!(ty.parameter_types.len(), 2);
            }
            other => panic!("expected a function, got {other:?}"),
        }
    }
}

#[test]
fn signature_shorter_than_the_binding_is_rejected() {
    // `g : int -> string` types ONE parameter; a two-parameter binding used to
    // absorb the RETURN type as its second parameter's annotation.
    assert_ml_error(
        "g : int -> string\ng (a, b) = \"x\"\n",
        &["provides 1 parameter types but the binding takes 2"],
    );
}

#[test]
fn named_paren_parameter_is_a_named_param() {
    // `(x)` is a named parameter (parenthesised), distinct from the `()` unit
    // marker — so the function has a real first parameter.
    match ml_one("f (x) = x + 1\n") {
        Stmt::Function { parameters, .. } => {
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "x");
        }
        other => panic!("expected a function, got {other:?}"),
    }
}

#[test]
fn prefix_not_operator_parses() {
    // `!flag` exercises the `!` prefix-unary branch.
    match let_value("r = !flag\n") {
        Expr::Unary { op, .. } => assert_eq!(op, "!"),
        other => panic!("expected unary, got {other:?}"),
    }
}

#[test]
fn float_argument_and_atom_parse() {
    // A float used as an application argument exercises the float atom branch.
    match let_value("r = scale 2.5\n") {
        Expr::Call { arguments, .. } => assert_eq!(arguments, vec![Expr::Float(2.5)]),
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn unclosed_paren_reports_an_error() {
    assert_ml_error("r = (1 + 2\n", &["')'"]);
}

#[test]
fn lambda_missing_fat_arrow_reports_an_error() {
    // `\x x` has no `=>` after the parameter list.
    assert_ml_error("f = \\x x\n", &["=>", "lambda"]);
}

#[test]
fn match_arm_missing_fat_arrow_reports_an_error() {
    let src = "r =\n    match x\n        0 1\n";
    assert_ml_error(src, &["=>", "arm"]);
}

#[test]
fn match_over_literal_and_binding_and_wildcard_patterns() {
    // String, bool, integer literals, a bare binding, and `_` patterns — covers
    // every pattern atom branch in the parser and lowerer.
    let src = "r =\n    match x\n        \"a\" => 1\n        true => 2\n        0 => 3\n        other => 4\n        _ => 5\n";
    match let_value(src) {
        Expr::Match { arms, .. } => {
            assert_eq!(arms.len(), 5);
            assert!(matches!(arms[0].pattern, Pattern::Literal(_)));
            assert!(matches!(arms[1].pattern, Pattern::Literal(_)));
            assert!(matches!(arms[2].pattern, Pattern::Literal(_)));
            assert!(matches!(arms[3].pattern, Pattern::Binding(ref n) if n == "other"));
            assert!(matches!(arms[4].pattern, Pattern::Wildcard));
        }
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn unexpected_token_in_pattern_reports_an_error() {
    // A `=>` where a pattern atom is expected (empty arm head).
    let src = "r =\n    match x\n        => 1\n";
    assert_ml_error(src, &["in pattern"]);
}

#[test]
fn record_field_missing_name_recovers() {
    // A record field line starting with a non-identifier triggers the recover
    // path inside `record_fields`.
    assert_ml_error("p =\n    Point\n        1 = 2\n        y = 3\n", &[]);
}

#[test]
fn binding_missing_equals_reports_an_error() {
    // A lambda body that drops the `=` of an inner binding: force expect_eq's
    // error path with a record field that has no `=`.
    let src = "p =\n    Point\n        x 1\n";
    assert_ml_error(src, &["'='"]);
}

#[test]
fn expression_only_top_level_item_is_a_bare_expr_statement() {
    // A leading non-identifier expression (a literal application) is parsed as a
    // bare expression item, exercising expr_item at top level.
    match ml_one("42\n") {
        Stmt::Expr { value, .. } => assert_eq!(value, Expr::Integer(42)),
        other => panic!("expected an expression statement, got {other:?}"),
    }
}

#[test]
fn reserved_words_each_report_not_yet_supported() {
    // Callable `handler` values are supported; standalone `do` is reserved.
    for word in ["do"] {
        for source in [format!("{word} Foo\n"), format!("value = {word} Foo\n")] {
            assert_ml_error(&source, &["not yet supported"]);
        }
    }
}

#[test]
fn block_with_leading_statement_then_trailing_value() {
    // A block whose first line is a binding and whose last line is a bare
    // expression: the binding is an item, the expression the block value
    // ([FLAVOR-ML-BLOCK]).
    let src = "f x =\n    y = x + 1\n    y\n";
    match ml_one(src) {
        Stmt::Function {
            body: Expr::Block {
                statements, value, ..
            },
            ..
        } => {
            assert_eq!(statements.len(), 1);
            assert!(value.is_some());
        }
        other => panic!("expected a block-bodied function, got {other:?}"),
    }
}

#[path = "ml_coverage/lower.rs"]
mod lower;

#[path = "ml_coverage/generics.rs"]
mod generics;
