//! Existing ML lowering assertions.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a panic"
)]
use crate::test_support::{ml_one_stmt, ml_stmts};
use osprey_ast::{Expr, InterpolatedPart, Stmt};

// ---------- [TESTING-DOC] expression-statement documentation ----------

#[test]
fn lambda_is_curried_and_pipe_desugars_to_call() {
    // `\x y => x + y` curries by default ([FLAVOR-ML-CURRY]): a one-parameter
    // `Expr::Lambda` over `x` whose body is a one-parameter lambda over `y` —
    // byte-identical to the Default explicit-curry `fn(x) => fn(y) => x + y`,
    // not a single two-parameter lambda.
    let s = ml_one_stmt("f = \\x y => x + y\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Lambda { .. },
                ..
            }
        ),
        "expected lambda, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Lambda {
            parameters, body, ..
        },
        ..
    } = s
    {
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "x");
        assert!(
            matches!(&*body, Expr::Lambda { parameters, .. }
                    if parameters.len() == 1 && parameters[0].name == "y"),
            "expected curried inner lambda over y, got {body:?}"
        );
        if let Expr::Lambda { body: inner, .. } = *body {
            assert!(matches!(*inner, Expr::Binary { ref op, .. } if op == "+"));
        }
    }
    // `x |> f` becomes `f(x)` — no Pipe node survives, matching Default
    // [BUILTIN-ITER-PIPE].
    let piped = ml_one_stmt("r = x |> f\n");
    assert!(
        matches!(
            piped,
            Stmt::Let {
                value: Expr::Call { .. },
                ..
            }
        ),
        "expected piped call, got {piped:?}"
    );
    if let Stmt::Let {
        value: Expr::Call {
            function,
            arguments,
            ..
        },
        ..
    } = piped
    {
        assert_eq!(*function, Expr::Identifier("f".to_owned()));
        assert_eq!(arguments, vec![Expr::Identifier("x".to_owned())]);
    }
}

#[test]
fn record_block_lowers_to_type_constructor() {
    let src = "p =\n    Point\n        x = 1\n        y = 2\n";
    let s = ml_one_stmt(src);
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::TypeConstructor { .. },
                ..
            }
        ),
        "expected type constructor, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::TypeConstructor { name, fields, .. },
        ..
    } = s
    {
        assert_eq!(name, "Point");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
    }
}

#[test]
fn inline_record_lowers_to_type_constructor() {
    // `Ok(value = "x")` in expression position is an inline record literal —
    // it lowers to the SAME `Expr::TypeConstructor` the layout form and the
    // Default `Ok { value: "x" }` produce ([FLAVOR-ML-RECORD]).
    let s = ml_one_stmt("r = Ok(value = \"x\")\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::TypeConstructor { .. },
                ..
            }
        ),
        "expected type constructor, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::TypeConstructor { name, fields, .. },
        ..
    } = s
    {
        assert_eq!(name, "Ok");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "value");
        assert_eq!(fields[0].value, Expr::Str("x".to_owned()));
    }
}

#[test]
fn lowercase_inline_record_lowers_to_update() {
    // `receiver(field = v)` with a LOWERCASE head is a non-destructive record
    // update; it lowers to the SAME `Expr::Update` the Default
    // `receiver { field: v }` produces ([FLAVOR-ML-RECORD]).
    let s = ml_one_stmt("p2 = point1(x = 30)\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Update { .. },
                ..
            }
        ),
        "expected record update, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Update { record, fields },
        ..
    } = s
    {
        assert_eq!(record, "point1");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].value, Expr::Integer(30));
    }
}

#[test]
fn map_literal_lowers_to_canonical_map() {
    // `["a" => 1, "b" => 2]` lowers to the SAME `Expr::Map` the Default
    // `{ "a": 1, "b": 2 }` produces ([FLAVOR-ML-MAP]).
    let s = ml_one_stmt("m = [\"a\" => 1, \"b\" => 2]\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Map(_),
                ..
            }
        ),
        "expected map, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Map(entries),
        ..
    } = s
    {
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, Expr::Str("a".to_owned()));
        assert_eq!(entries[0].value, Expr::Integer(1));
        assert_eq!(entries[1].key, Expr::Str("b".to_owned()));
    }
    // `[=>]` is the explicit empty-map form.
    assert!(matches!(
        ml_one_stmt("m = [=>]\n"),
        Stmt::Let { value: Expr::Map(ref e), .. } if e.is_empty()
    ));
}

#[test]
fn generic_type_annotation_lowers_to_generic_params() {
    // `empty : List<string>` flows the angle-bracketed generic argument into
    // `TypeExpr.generic_params`, byte-identical to the Default annotation.
    let s = ml_stmts("empty : List<string>\nempty = []\n");
    let first = s.first();
    assert!(
        matches!(first, Some(Stmt::Let { ty: Some(_), .. })),
        "expected typed let, got {first:?}"
    );
    if let Some(Stmt::Let { ty: Some(ty), .. }) = first {
        assert_eq!(ty.name, "List");
        assert_eq!(ty.generic_params.len(), 1);
        assert_eq!(ty.generic_params[0].name, "string");
    }
}

#[test]
fn fn_typed_field_renders_with_parenthesised_arg() {
    // A function-typed record field renders as `(int) -> bool` — the spelling
    // the type checker accepts — not the bare `int -> bool` ([FLAVOR-ML-TYPE]).
    let s = ml_one_stmt("type Checker =\n    check : (int) -> bool\n");
    assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
    if let Stmt::Type { variants, .. } = s {
        assert_eq!(variants[0].fields[0].ty, "(int) -> bool");
    }
}

#[test]
fn spawn_inline_expr_lowers_to_spawn() {
    // `spawn f x` lowers to `Expr::Spawn` wrapping the call, byte-identical
    // to the Default `spawn f(x)` ([FLAVOR-ML-SPAWN]).
    let s = ml_one_stmt("r = spawn task 1\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Spawn(_),
                ..
            }
        ),
        "expected spawn, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Spawn(inner),
        ..
    } = s
    {
        assert!(
            matches!(*inner, Expr::Call { .. }),
            "spawn body should be the call, got {inner:?}"
        );
    }
}

#[test]
fn spawn_block_lowers_to_spawn_block() {
    // `spawn` + an indented block lowers to `Expr::Spawn` wrapping the block.
    let s = ml_one_stmt("r = spawn\n    x = 1\n    task x\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Spawn(_),
                ..
            }
        ),
        "expected spawn, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Spawn(inner),
        ..
    } = s
    {
        assert!(
            matches!(*inner, Expr::Block { .. }),
            "spawn block body should be a Block, got {inner:?}"
        );
    }
}

#[test]
fn interpolation_parses_fragment_as_ml_application() {
    // `${toString id}` is ML whitespace application inside the fragment.
    let s = ml_one_stmt("r = \"n=${toString id}\"\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::InterpolatedStr(_),
                ..
            }
        ),
        "expected interpolated string, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::InterpolatedStr(parts),
        ..
    } = s
    {
        assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "n="));
        assert!(matches!(
            parts[1],
            InterpolatedPart::Expr(Expr::Call { .. })
        ));
    }
}

#[test]
fn block_body_with_statements_keeps_block_with_trailing_value() {
    let src = "f x =\n    y = x + 1\n    y + 2\n";
    let s = ml_one_stmt(src);
    assert!(
        matches!(
            s,
            Stmt::Function {
                body: Expr::Block { .. },
                ..
            }
        ),
        "expected block body, got {s:?}"
    );
    if let Stmt::Function {
        body: Expr::Block {
            statements, value, ..
        },
        ..
    } = s
    {
        assert_eq!(statements.len(), 1);
        assert!(value.is_some());
    }
}

#[test]
fn name_binding_is_an_immutable_let_at_top_level_and_in_a_block() {
    // The cross-flavor guarantee `name = expr` must satisfy: it lowers to the
    // SAME node a Default `let name = expr` does — `Stmt::Let { mutable:
    // false }` — both at the top level and inside a layout block. This is the
    // structural precondition for byte-identical IR with the Default twin
    // ([FLAVOR-CURRY], [FLAVOR-IR-EQUIV]); only `mut`+`:=` produces an
    // `Assignment`, never a bare `=`.
    assert!(matches!(
        ml_one_stmt("answer = 41 + 1\n"),
        Stmt::Let { mutable: false, .. }
    ));
    // Same binding, this time the first statement of a function block.
    let s = ml_one_stmt("main () =\n    answer = 41 + 1\n    answer\n");
    assert!(
        matches!(
            s,
            Stmt::Function {
                body: Expr::Block { .. },
                ..
            }
        ),
        "expected function with block body, got {s:?}"
    );
    if let Stmt::Function {
        body: Expr::Block { statements, .. },
        ..
    } = s
    {
        assert!(
            matches!(statements.first(), Some(Stmt::Let { mutable: false, name, .. }) if name == "answer"),
            "block-local `name = expr` must be an immutable Let, got {statements:?}"
        );
    }
}

#[test]
fn union_type_lowers_to_canonical_type_stmt() {
    // The ML layout union must lower to the SAME `Stmt::Type` the Default
    // `type Outcome = Ok { value: string } | Err { message: string }` emits:
    // two payload-carrying variants with `validation_func: None` and each
    // field `constraint: None` ([FLAVOR-ML-TYPE], [FLAVOR-IR-EQUIV]).
    let src = "type Outcome =\n    Ok\n        value : string\n    Err\n        message : string\n";
    let s = ml_one_stmt(src);
    assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
    if let Stmt::Type {
        name,
        type_params,
        variants,
        validation_func,
        ..
    } = s
    {
        assert_eq!(name, "Outcome");
        assert_eq!(type_params, Vec::<osprey_ast::TypeParam>::new());
        assert!(validation_func.is_none());
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name, "Ok");
        assert_eq!(variants[0].fields.len(), 1);
        assert_eq!(variants[0].fields[0].name, "value");
        assert_eq!(variants[0].fields[0].ty, "string");
        assert!(variants[0].fields[0].constraint.is_none());
        assert_eq!(variants[1].name, "Err");
        assert_eq!(variants[1].fields[0].name, "message");
    }
}

#[test]
fn enum_type_lowers_to_fieldless_variants() {
    let s = ml_one_stmt("type Status =\n    Active\n    Inactive\n");
    assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
    if let Stmt::Type { variants, .. } = s {
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name, "Active");
        assert_eq!(variants[0].fields, Vec::<osprey_ast::TypeField>::new());
        assert_eq!(variants[1].name, "Inactive");
        assert_eq!(variants[1].fields, Vec::<osprey_ast::TypeField>::new());
    }
}

#[test]
fn record_type_lowers_to_single_variant_named_after_type() {
    // A lowercase first field marks the record form; its lone variant takes
    // the type's own name, exactly as Default's `type Point = { x, y }` does.
    let s = ml_one_stmt("type Point =\n    x : int\n    y : int\n");
    assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
    if let Stmt::Type { name, variants, .. } = s {
        assert_eq!(name, "Point");
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].name, "Point");
        assert_eq!(variants[0].fields.len(), 2);
        assert_eq!(variants[0].fields[0].name, "x");
        assert_eq!(variants[0].fields[0].ty, "int");
    }
}
