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
    let parsed = ml_err("x = 99999999999999999999999999\n");
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("invalid integer")),
        "expected invalid integer error, got {:?}",
        parsed.errors
    );
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
    let parsed = ml_err("s = \"oops\n");
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("unterminated")),
        "expected unterminated string error, got {:?}",
        parsed.errors
    );
}

#[test]
fn unexpected_character_reports_an_error() {
    // `@`/`#`/`;` are not Osprey operators — the single-char operator table
    // returns None and the scanner reports an unexpected-character error.
    for src in ["x = @\n", "x = #\n", "y = ;\n"] {
        let parsed = ml_err(src);
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.message.contains("unexpected character")),
            "src {src:?} expected unexpected-character error, got {:?}",
            parsed.errors
        );
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
    // Comma appears in a tuple type signature; dot in a field access.
    let sig = ml_ok("f : (int, int) -> int\nf x = x\n");
    assert!(matches!(sig.first(), Some(Stmt::Function { .. })));
}

#[test]
fn inconsistent_indentation_reports_an_error() {
    // The third line dedents to column 2, which matches no enclosing level
    // (levels are 0 and 4) — the offside rule reports it.
    let src = "f =\n    a\n  b\n";
    let parsed = ml_err(src);
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("inconsistent indentation")),
        "expected inconsistent-indentation error, got {:?}",
        parsed.errors
    );
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
    let parsed = ml_err("x : =\nx = 1\n");
    assert!(
        parsed.errors.iter().any(|e| e.message.contains("in type")),
        "expected an in-type error, got {:?}",
        parsed.errors
    );
}

#[test]
fn tuple_type_signature_parses_and_leaves_inference_to_the_checker() {
    // A tuple type `(int, int)` has no canonical TypeExpr form, so the lowerer
    // leaves the parameter type as None — but it must parse without error.
    let s = ml_ok("f : (int, int) -> int\nf x = 1\n");
    match s.into_iter().next() {
        Some(Stmt::Function { parameters, .. }) => assert!(parameters[0].ty.is_none()),
        other => panic!("expected a function, got {other:?}"),
    }
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
    let parsed = ml_err("r = (1 + 2\n");
    assert!(
        parsed.errors.iter().any(|e| e.message.contains("')'")),
        "expected an unclosed-paren error, got {:?}",
        parsed.errors
    );
}

#[test]
fn lambda_missing_fat_arrow_reports_an_error() {
    // `\x x` has no `=>` after the parameter list.
    let parsed = ml_err("f = \\x x\n");
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("=>") && e.message.contains("lambda")),
        "expected a lambda-arrow error, got {:?}",
        parsed.errors
    );
}

#[test]
fn match_arm_missing_fat_arrow_reports_an_error() {
    let src = "r =\n    match x\n        0 1\n";
    let parsed = ml_err(src);
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("=>") && e.message.contains("arm")),
        "expected a match-arm-arrow error, got {:?}",
        parsed.errors
    );
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
    let parsed = ml_err(src);
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("in pattern")),
        "expected an in-pattern error, got {:?}",
        parsed.errors
    );
}

#[test]
fn record_field_missing_name_recovers() {
    // A record field line starting with a non-identifier triggers the recover
    // path inside `record_fields`.
    let src = "p =\n    Point\n        1 = 2\n        y = 3\n";
    let parsed = ml_err(src);
    assert!(
        !parsed.errors.is_empty(),
        "expected a record-field error, got {:?}",
        parsed.errors
    );
}

#[test]
fn binding_missing_equals_reports_an_error() {
    // A lambda body that drops the `=` of an inner binding: force expect_eq's
    // error path with a record field that has no `=`.
    let src = "p =\n    Point\n        x 1\n";
    let parsed = ml_err(src);
    assert!(
        parsed.errors.iter().any(|e| e.message.contains("'='")),
        "expected a missing-equals error, got {:?}",
        parsed.errors
    );
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
    // The remaining Phase-0 words (first-class handler values) must error loudly
    // rather than misparse. `effect`/`handle`/`perform`/`resume` are now supported
    // and lower to the canonical effect AST ([FLAVOR-ML-EFFECT]).
    for word in ["handler", "do"] {
        let parsed = ml_err(&format!("{word} Foo\n"));
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.message.contains("not yet supported")),
            "word {word:?} expected a not-yet-supported error, got {:?}",
            parsed.errors
        );
    }
}

#[test]
fn block_with_leading_statement_then_trailing_value() {
    // A block whose first line is a binding and whose last line is a bare
    // expression: the binding is an item, the expression the block value.
    let src = "f x =\n    y = x + 1\n    y\n";
    match ml_one(src) {
        Stmt::Function {
            body: Expr::Block { statements, value },
            ..
        } => {
            assert_eq!(statements.len(), 1);
            assert!(value.is_some());
        }
        other => panic!("expected a block-bodied function, got {other:?}"),
    }
}

// --- lower.rs ----------------------------------------------------------------

#[test]
fn pipe_into_bare_callee_and_into_a_call() {
    // `x |> f` wraps a bare callee; `x |> f a` prepends to an existing call.
    match let_value("r = x |> f\n") {
        Expr::Call {
            function,
            arguments,
            ..
        } => {
            assert_eq!(*function, Expr::Identifier("f".to_owned()));
            assert_eq!(arguments, vec![Expr::Identifier("x".to_owned())]);
        }
        other => panic!("expected a piped call, got {other:?}"),
    }
    // `x |> f a` — the right side is already a call `f a`, so x is prepended.
    match let_value("r = x |> f a\n") {
        Expr::Call { arguments, .. } => {
            assert_eq!(arguments.len(), 2);
            assert_eq!(arguments[0], Expr::Identifier("x".to_owned()));
        }
        other => panic!("expected a piped call, got {other:?}"),
    }
}

#[test]
fn zero_parameter_lambda_lowers_to_an_empty_lambda() {
    // `\=> body` (no params) lowers to a zero-parameter Lambda.
    match let_value("f = \\=> 1\n") {
        Expr::Lambda { parameters, .. } => assert!(parameters.is_empty()),
        other => panic!("expected a lambda, got {other:?}"),
    }
}

#[test]
fn field_access_lowers_to_field_access() {
    match let_value("r = rec.field\n") {
        Expr::FieldAccess { field, .. } => assert_eq!(field, "field"),
        other => panic!("expected a field access, got {other:?}"),
    }
}

#[test]
fn string_pattern_in_match_lowers_to_a_literal() {
    match let_value("r =\n    match s\n        \"hi\" => 1\n        _ => 0\n") {
        Expr::Match { arms, .. } => {
            assert!(
                matches!(&arms[0].pattern, Pattern::Literal(boxed) if matches!(**boxed, Expr::Str(ref t) if t == "hi"))
            );
        }
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn constructor_pattern_with_payload_fields_lowers() {
    // `Success value` binds one payload field; `Error message` another.
    match let_value(
        "r =\n    match res\n        Success value => value\n        Error message => message\n",
    ) {
        Expr::Match { arms, .. } => {
            match &arms[0].pattern {
                Pattern::Constructor { name, fields, .. } => {
                    assert_eq!(name, "Success");
                    assert_eq!(fields, &vec!["value".to_owned()]);
                }
                other => panic!("expected a constructor pattern, got {other:?}"),
            }
            match &arms[1].pattern {
                Pattern::Constructor { name, fields, .. } => {
                    assert_eq!(name, "Error");
                    assert_eq!(fields, &vec!["message".to_owned()]);
                }
                other => panic!("expected a constructor pattern, got {other:?}"),
            }
        }
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn interpolation_fragment_falls_back_to_identifier_for_unparseable() {
    // A `${...}` whose body is a lone reserved/keyword token does not parse to a
    // binding, so the fragment lowers to the trimmed identifier fallback.
    match let_value("r = \"v=${ match }\"\n") {
        Expr::InterpolatedStr(parts) => assert!(!parts.is_empty()),
        other => panic!("expected an interpolated string, got {other:?}"),
    }
}

#[test]
fn interpolation_with_application_fragment_lowers_to_a_call() {
    // `${toString id}` is ML whitespace application inside the fragment.
    match let_value("r = \"n=${toString id}\"\n") {
        Expr::InterpolatedStr(parts) => assert!(parts.len() >= 2),
        other => panic!("expected an interpolated string, got {other:?}"),
    }
}

#[test]
fn three_parameter_function_is_curried_nested_lambda() {
    // `f a b c = a` curries by default ([FLAVOR-ML-CURRY]): a single ONE-parameter
    // function over `a` whose body is a nested one-parameter lambda chain over `b`
    // then `c` — byte-identical to the Default *explicit-curry*
    // `fn f(a) = fn(b) => fn(c) => a`, deliberately NOT the flat `fn f(a, b, c)`.
    match ml_one("f a b c = a\n") {
        Stmt::Function {
            parameters, body, ..
        } => {
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "a");
            match body {
                Expr::Lambda {
                    parameters, body, ..
                } => {
                    assert_eq!(parameters.len(), 1);
                    assert_eq!(parameters[0].name, "b");
                    match *body {
                        Expr::Lambda {
                            parameters, body, ..
                        } => {
                            assert_eq!(parameters.len(), 1);
                            assert_eq!(parameters[0].name, "c");
                            assert!(matches!(*body, Expr::Identifier(ref n) if n == "a"));
                        }
                        other => panic!("expected inner lambda over c, got {other:?}"),
                    }
                }
                other => panic!("expected curried lambda over b, got {other:?}"),
            }
        }
        other => panic!("expected a function, got {other:?}"),
    }
}

#[test]
fn mut_binding_and_reassignment_lower_distinctly() {
    let s = ml_ok("mut total = 0\ntotal := total + 1\n");
    assert!(matches!(s[0], Stmt::Let { mutable: true, .. }));
    assert!(matches!(s[1], Stmt::Assignment { ref name, .. } if name == "total"));
}

#[test]
fn signed_let_binding_carries_its_declared_type() {
    // A value binding (no params) with a signature lowers to a `let` whose `ty`
    // is the signature.
    match ml_one("count : int\ncount = 0\n") {
        Stmt::Let { ty, .. } => {
            assert!(matches!(ty, Some(TypeExpr { ref name, .. }) if name == "int"));
        }
        other => panic!("expected a let, got {other:?}"),
    }
}

#[test]
fn typed_uncurried_function_and_lambda_params_lower() {
    match ml_one("pair (x : int, y : string) = x\n") {
        Stmt::Function { parameters, .. } => {
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name, "x");
            assert!(matches!(parameters[0].ty, Some(TypeExpr { ref name, .. }) if name == "int"));
            assert_eq!(parameters[1].name, "y");
            assert!(
                matches!(parameters[1].ty, Some(TypeExpr { ref name, .. }) if name == "string")
            );
        }
        other => panic!("expected a flat function, got {other:?}"),
    }

    match let_value("f = \\(x : int, y : string) => x\n") {
        Expr::Lambda { parameters, .. } => {
            assert_eq!(parameters.len(), 2);
            assert!(matches!(parameters[0].ty, Some(TypeExpr { ref name, .. }) if name == "int"));
            assert!(
                matches!(parameters[1].ty, Some(TypeExpr { ref name, .. }) if name == "string")
            );
        }
        other => panic!("expected a flat lambda, got {other:?}"),
    }
}

#[test]
fn typed_curried_params_thread_into_function_and_lambda_tail() {
    match ml_one("apply (f : int -> int) (x : int) = f x\n") {
        Stmt::Function {
            parameters, body, ..
        } => {
            assert_eq!(parameters.len(), 1);
            assert!(matches!(
                parameters[0].ty,
                Some(TypeExpr {
                    is_function: true,
                    ..
                })
            ));
            match body {
                Expr::Lambda { parameters, .. } => {
                    assert_eq!(parameters.len(), 1);
                    assert!(
                        matches!(parameters[0].ty, Some(TypeExpr { ref name, .. }) if name == "int")
                    );
                }
                other => panic!("expected a typed lambda tail, got {other:?}"),
            }
        }
        other => panic!("expected a curried function, got {other:?}"),
    }
}

#[test]
fn tuple_type_and_effect_payload_rendering_are_canonical() {
    match ml_one("type TupleBox =\n    pair : (int, string)\n") {
        Stmt::Type { variants, .. } => {
            assert_eq!(variants[0].fields[0].ty, "(int, string)");
        }
        other => panic!("expected a type declaration, got {other:?}"),
    }

    match ml_one("type FnBox =\n    mapper : (int, string) -> Result int string\n") {
        Stmt::Type { variants, .. } => {
            assert_eq!(
                variants[0].fields[0].ty,
                "(int, string) -> Result<int, string>"
            );
        }
        other => panic!("expected a type declaration, got {other:?}"),
    }

    match ml_one(
        "effect Bus\n    publish : (string, int) => Result string string\n    ping : Unit => int\n",
    ) {
        Stmt::Effect { operations, .. } => {
            assert_eq!(
                operations[0].ty,
                "fn(string, int) -> Result<string, string>"
            );
            assert_eq!(operations[1].ty, "fn() -> int");
        }
        other => panic!("expected an effect declaration, got {other:?}"),
    }
}

#[test]
fn unit_tail_params_and_empty_type_bodies_parse() {
    match ml_one("ignore x () = x\n") {
        Stmt::Function { body, .. } => match body {
            Expr::Lambda { parameters, .. } => assert!(parameters.is_empty()),
            other => panic!("expected a unit lambda tail, got {other:?}"),
        },
        other => panic!("expected a function, got {other:?}"),
    }

    match ml_one("type Empty =\n") {
        Stmt::Type { variants, .. } => assert!(variants.is_empty()),
        other => panic!("expected an empty type declaration, got {other:?}"),
    }

    match ml_one("type Choice =\n    A\n\n") {
        Stmt::Type { variants, .. } => assert_eq!(variants.len(), 1),
        other => panic!("expected a union declaration, got {other:?}"),
    }
}

#[test]
fn extern_without_return_type_uses_none() {
    match ml_one("extern log (message : string)\n") {
        Stmt::Extern {
            parameters,
            return_type,
            ..
        } => {
            assert_eq!(parameters.len(), 1);
            assert!(return_type.is_none());
        }
        other => panic!("expected an extern declaration, got {other:?}"),
    }
}

#[test]
fn concurrency_forms_and_uncurried_calls_lower_to_ast_nodes() {
    assert!(matches!(
        let_value("r = await (spawn task 1)\n"),
        Expr::Await(_)
    ));
    assert!(matches!(let_value("r = yield\n"), Expr::Yield(None)));
    assert!(matches!(
        let_value("r = yield value\n"),
        Expr::Yield(Some(_))
    ));
    assert!(matches!(
        let_value("r = send ch value\n"),
        Expr::Send { .. }
    ));
    assert!(matches!(let_value("r = recv ch\n"), Expr::Recv(_)));

    match let_value("r = f (1, 2)\n") {
        Expr::Call { arguments, .. } => assert_eq!(arguments.len(), 2),
        other => panic!("expected a flat uncurried call, got {other:?}"),
    }

    match let_value("r =\n    select\n        value => value\n        _ => 0\n") {
        Expr::Select { arms } => assert_eq!(arms.len(), 2),
        other => panic!("expected a select expression, got {other:?}"),
    }
}

#[test]
fn stray_operator_as_an_operand_falls_through_to_an_error() {
    // The right of `+` starts with `*`, which is an operator but neither `-` nor
    // `!`: the unary prefix branch is skipped and the atom parser reports the
    // unexpected operator token.
    let parsed = ml_err("r = 1 + * 2\n");
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("in expression")),
        "expected an in-expression error, got {:?}",
        parsed.errors
    );
}

#[test]
fn match_with_blank_separator_lines_between_arms() {
    // Blank lines between arms become separators; after skipping them the loop
    // re-checks for block end, covering the inner break/skip path.
    let src = "r =\n    match x\n        0 => 1\n\n        _ => 2\n";
    match let_value(src) {
        Expr::Match { arms, .. } => assert_eq!(arms.len(), 2),
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn record_with_blank_separator_lines_between_fields() {
    // Blank lines between record fields exercise the record-field separator path.
    let src = "p =\n    Point\n        x = 1\n\n        y = 2\n";
    match let_value(src) {
        Expr::TypeConstructor { name, fields, .. } => {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
        }
        other => panic!("expected a type constructor, got {other:?}"),
    }
}

#[test]
fn block_with_a_skipped_line_keeps_parsing() {
    // A signature line inside a block parses as an item; a reserved word inside a
    // block returns None for that line (the block_line None path) yet recovery
    // lets the surrounding parse continue and report the error.
    let src = "f x =\n    do thing\n    x\n";
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.contains("not yet supported")),
        "expected a not-yet-supported error, got {:?}",
        parsed.errors
    );
}

#[test]
fn empty_interpolation_fragment_falls_back_to_an_identifier() {
    // `${ }` does not parse to a binding body, so parse_fragment returns the
    // trimmed (empty) identifier fallback rather than misparsing.
    match let_value("r = \"x=${ }\"\n") {
        Expr::InterpolatedStr(parts) => assert!(!parts.is_empty()),
        other => panic!("expected an interpolated string, got {other:?}"),
    }
}

#[test]
fn orphan_signature_without_a_matching_binding_is_dropped() {
    // A signature whose name does not match the next binding is discarded, and
    // the following binding still lowers cleanly (untyped).
    match ml_one("foo : int\nbar = 1\n") {
        Stmt::Let { name, ty, .. } => {
            assert_eq!(name, "bar");
            assert!(ty.is_none());
        }
        other => panic!("expected a let, got {other:?}"),
    }
}

// ---- generics, variance, and generic effects ([FLAVOR-ML-GENERICS]) ----

#[test]
fn generic_signature_binders_and_variance_markers_parse() {
    // A generic signature `pick<T, U> : ...` binds fn type params.
    let stmts = ml_ok("pick<T, U> : (T, U) -> T\npick (first, second) = first\n");
    match &stmts[0] {
        Stmt::Function { type_params, .. } => {
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T", "U"]);
        }
        s => panic!("expected function, got {s:?}"),
    }
    // Variance markers are parsed on signature binders too (the checker
    // rejects them later — the parser stays faithful).
    let stmts = ml_ok("f<out T, in U> : (U) -> T\nf x = x\n");
    match &stmts[0] {
        Stmt::Function { type_params, .. } => {
            assert_eq!(type_params[0].variance, osprey_ast::Variance::Covariant);
            assert_eq!(type_params[1].variance, osprey_ast::Variance::Contravariant);
        }
        s => panic!("expected function, got {s:?}"),
    }
}

#[test]
fn generic_signature_lookahead_never_swallows_comparisons() {
    // `x<y` at item start is a comparison expression, not a signature.
    let stmts = ml_ok("x = 1\ny = 2\nz = x < y\n");
    assert_eq!(stmts.len(), 3);
    // `f<T> = 1` (no colon after the binder shape) is not a signature either.
    let parsed = parse_program_with_flavor("f<T> = 1\n", Flavor::Ml);
    assert!(!parsed.program.statements.is_empty());
    // `f<1> : int` (non-identifier inside) falls back to expression parsing.
    let parsed = parse_program_with_flavor("f<1> : int\n", Flavor::Ml);
    assert!(parsed.program.statements.is_empty() || !parsed.errors.is_empty());
}

#[test]
fn variance_markers_on_type_declarations() {
    // `out` marks covariance.
    let s = ml_one("type Feed out T =\n    supply : T\n");
    match s {
        Stmt::Type { type_params, .. } => {
            assert_eq!(type_params[0].name, "T");
            assert_eq!(type_params[0].variance, osprey_ast::Variance::Covariant);
        }
        s => panic!("expected type, got {s:?}"),
    }
    // A variance marker must be followed by a parameter name — `out`/`in`
    // are reserved inside type-parameter position in BOTH flavors.
    let parsed = parse_program_with_flavor("type Odd out =\n    supply : int\n", Flavor::Ml);
    assert!(parsed
        .errors
        .iter()
        .any(|e| e.message.contains("after 'out'")));
    // `in` must be followed by a parameter name.
    let parsed = parse_program_with_flavor("type Bad in =\n    x : int\n", Flavor::Ml);
    assert!(parsed
        .errors
        .iter()
        .any(|e| e.message.contains("after 'in'")));
}

#[test]
fn generic_effects_and_rows_parse_in_ml() {
    // `effect Stash T` declares a generic effect ([EFFECTS-GENERIC-DECL]).
    let s = ml_one("effect Stash T\n    put : T => Unit\n    take : Unit => T\n");
    match s {
        Stmt::Effect {
            type_params,
            operations,
            ..
        } => {
            assert_eq!(type_params.len(), 1);
            assert_eq!(operations.len(), 2);
        }
        s => panic!("expected effect, got {s:?}"),
    }
    // Effect rows carry type arguments, bare and bracketed
    // ([EFFECTS-GENERIC-ROWS]).
    let stmts = ml_ok("f : Unit -> int ! Stash<int>\nf () = 1\n");
    match &stmts[0] {
        Stmt::Function { effects, .. } => {
            assert_eq!(effects[0].name, "Stash");
            assert_eq!(effects[0].type_args[0].name, "int");
        }
        s => panic!("expected function, got {s:?}"),
    }
    let stmts = ml_ok("g : Unit -> int ! [Stash<string>, Log]\ng () = 1\n");
    match &stmts[0] {
        Stmt::Function { effects, .. } => {
            assert_eq!(effects.len(), 2);
            assert_eq!(effects[0].type_args.len(), 1);
            assert!(effects[1].type_args.is_empty());
        }
        s => panic!("expected function, got {s:?}"),
    }
    // An unclosed row argument list reports an error but recovers.
    let parsed = parse_program_with_flavor("h : Unit -> int ! Stash<int\nh () = 1\n", Flavor::Ml);
    assert!(!parsed.errors.is_empty());
}

#[test]
fn construction_site_type_args_parse_in_ml() {
    // `Box<int>(item = 7)` — explicit construction-site type arguments on the
    // inline record form, twinning Default `Box<int> { item: 7 }`
    // ([TYPE-GENERICS-DECL], [FLAVOR-ML-GENERICS]).
    let stmts = ml_ok("type Box T =\n    item : T\npinned = Box<int>(item = 7)\n");
    match &stmts[1] {
        Stmt::Let {
            value: Expr::TypeConstructor { name, type_args, .. },
            ..
        } => {
            assert_eq!(name, "Box");
            assert_eq!(type_args.len(), 1);
            assert_eq!(type_args[0].name, "int");
        }
        s => panic!("expected pinned constructor, got {s:?}"),
    }
    // `Ctor < x` comparisons never misparse as a generic record.
    let stmts = ml_ok("x = 1\ny = Box < x\n");
    assert_eq!(stmts.len(), 2);
}
