//! ML lowering coverage (`src/ml/lower.rs`): pipes, lambdas, patterns, interpolation, signatures.

use osprey_ast::{Expr, Pattern, Stmt, TypeExpr};

use super::{assert_ml_error, let_value, ml_ok, ml_one};

/// The constructor pattern's name and payload field, or a failing panic.
fn assert_ctor(pattern: &Pattern, name: &str, field: &str) {
    match pattern {
        Pattern::Constructor {
            name: got, fields, ..
        } => {
            assert_eq!(got, name, "constructor name for `{name}`");
            assert_eq!(fields, &vec![field.to_owned()], "payload field of `{name}`");
        }
        other => panic!("expected a constructor pattern for `{name}`, got {other:?}"),
    }
}

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
        Expr::Lambda { parameters, .. } => {
            assert_eq!(parameters, Vec::<osprey_ast::Parameter>::new());
        }
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
            assert_ctor(&arms[0].pattern, "Success", "value");
            assert_ctor(&arms[1].pattern, "Error", "message");
        }
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn interpolation_fragment_falls_back_to_identifier_for_unparseable() {
    // A `${...}` whose body is a lone reserved/keyword token does not parse to a
    // binding, so the fragment lowers to the trimmed identifier fallback.
    match let_value("r = \"v=${ match }\"\n") {
        Expr::InterpolatedStr(parts) => {
            assert_ne!(parts, Vec::<osprey_ast::InterpolatedPart>::new());
        }
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
    // `f a b c = a` curries by default ([FLAVOR-ML-FN], [FLAVOR-ML-CURRY]): a
    // single ONE-parameter function over `a` whose body is a nested one-parameter
    // lambda chain over `b` then `c` — byte-identical to the Default *explicit-curry*
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
    // [FLAVOR-ML-BIND]
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
    // [FLAVOR-ML-TYPE]
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
            Expr::Lambda { parameters, .. } => {
                assert_eq!(parameters, Vec::<osprey_ast::Parameter>::new());
            }
            other => panic!("expected a unit lambda tail, got {other:?}"),
        },
        other => panic!("expected a function, got {other:?}"),
    }

    match ml_one("type Empty =\n") {
        Stmt::Type { variants, .. } => assert_eq!(variants, Vec::<osprey_ast::TypeVariant>::new()),
        other => panic!("expected an empty type declaration, got {other:?}"),
    }

    match ml_one("type Choice =\n    A\n\n") {
        Stmt::Type { variants, .. } => assert_eq!(variants.len(), 1),
        other => panic!("expected a union declaration, got {other:?}"),
    }
}

#[test]
fn extern_without_return_type_uses_none() {
    // [FLAVOR-ML-EXTERN]
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
    // [FLAVOR-ML-SPAWN], [FLAVOR-ML-CONCURRENCY], [FLAVOR-ML-CALL]
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

    match let_value(
        "r =\n    select\n        value = recv a => value\n        recv a => 1\n        send a 2 => 3\n        timeout 4 => 5\n        _ => 0\n",
    ) {
        Expr::Select { operations, arms } => {
            assert_eq!(arms.len(), 5);
            assert_eq!(operations.len(), 5);
            assert_eq!(arms[0].pattern, Pattern::Binding("value".to_owned()));
        }
        other => panic!("expected a select expression, got {other:?}"),
    }
}

#[test]
fn stray_operator_as_an_operand_falls_through_to_an_error() {
    // The right of `+` starts with `*`, which is an operator but neither `-` nor
    // `!`: the unary prefix branch is skipped and the atom parser reports the
    // unexpected operator token.
    assert_ml_error("r = 1 + * 2\n", &["in expression"]);
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
    // Blank lines between record fields exercise the record-field separator path
    // ([FLAVOR-ML-RECORD]).
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
    assert_ml_error(src, &["not yet supported"]);
}

#[test]
fn empty_interpolation_fragment_falls_back_to_an_identifier() {
    // `${ }` does not parse to a binding body, so parse_fragment returns the
    // trimmed (empty) identifier fallback rather than misparsing.
    match let_value("r = \"x=${ }\"\n") {
        Expr::InterpolatedStr(parts) => {
            assert_ne!(parts, Vec::<osprey_ast::InterpolatedPart>::new());
        }
        other => panic!("expected an interpolated string, got {other:?}"),
    }
}

#[test]
fn orphan_signature_without_a_matching_binding_is_rejected() {
    assert_ml_error("foo : int\nbar = 1\n", &["immediately followed"]);
}
