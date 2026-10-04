//! Existing ML lowering assertions.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a panic"
)]
use crate::ml::parse_ml;
use crate::test_support::ml_one_stmt;
use osprey_ast::{Expr, Stmt, Variance};

// ---------- [TESTING-DOC] expression-statement documentation ----------

#[test]
fn extern_lowers_to_canonical_extern_stmt() {
    // `extern name (p : T) (q : U) -> R` lowers to the SAME `Stmt::Extern`
    // the Default `extern fn name(p: T, q: U) -> R` emits — typed parameters
    // in order plus a return type ([FLAVOR-ML-EXTERN], [FLAVOR-IR-EQUIV]).
    let s = ml_one_stmt("extern sqlite3_open (filename : string) (ppDb : Ptr) -> int\n");
    assert!(
        matches!(s, Stmt::Extern { .. }),
        "expected extern, got {s:?}"
    );
    if let Stmt::Extern {
        name,
        parameters,
        return_type,
        ..
    } = s
    {
        assert_eq!(name, "sqlite3_open");
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name, "filename");
        assert_eq!(parameters[0].ty.name, "string");
        assert_eq!(parameters[1].name, "ppDb");
        assert_eq!(parameters[1].ty.name, "Ptr");
        assert_eq!(return_type.map(|t| t.name), Some("int".to_owned()));
    }
}

#[test]
fn reserved_do_word_reports_a_clear_error() {
    // Callable handlers are supported; a standalone `do` remains reserved.
    let parsed = parse_ml("do work ()\n");
    assert!(parsed
        .errors
        .iter()
        .any(|e| e.message.contains("not yet supported")));
}

#[test]
fn effect_decl_lowers_to_effect_stmt() {
    // `effect Trace` + `mark : string => Unit` lowers to the SAME
    // `Stmt::Effect` the Default `effect Trace { mark: fn(string) -> Unit }`
    // emits — one operation rendered as `fn(string) -> Unit`, with empty
    // parameters and a blank return type ([FLAVOR-ML-EFFECT], [FLAVOR-IR-EQUIV]).
    let s = ml_one_stmt("effect Trace\n    mark : string => Unit\n");
    assert!(
        matches!(s, Stmt::Effect { .. }),
        "expected effect, got {s:?}"
    );
    if let Stmt::Effect {
        name, operations, ..
    } = s
    {
        assert_eq!(name, "Trace");
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].name, "mark");
        assert_eq!(operations[0].ty, "fn(string) -> Unit");
        assert_eq!(
            operations[0].parameters,
            Vec::<osprey_ast::Parameter>::new()
        );
        assert_eq!(operations[0].return_type, "");
    }
}

#[test]
fn multi_arg_effect_op_renders_flat_payload() {
    // A multi-argument op `exec : (Ptr, string) => int` must lower to the
    // FLAT `fn(Ptr, string) -> int` the Default flavor emits — NOT the
    // parenthesised `fn((Ptr, string)) -> int` — so inference recovers each
    // argument type and the IR is byte-identical ([FLAVOR-ML-EFFECT],
    // [FLAVOR-IR-EQUIV]).
    let s = ml_one_stmt("effect Database\n    exec : (Ptr, string) => int\n");
    if let Stmt::Effect { operations, .. } = s {
        assert_eq!(operations[0].ty, "fn(Ptr, string) -> int");
    } else {
        panic!("expected effect, got {s:?}");
    }
}

#[test]
fn signature_effect_row_threads_into_function() {
    // `traced : Unit -> int ! Trace` puts `Trace` in the function's effect row,
    // byte-identical to the Default `fn traced() -> int !Trace`.
    let s =
        ml_one_stmt("traced : Unit -> int ! Trace\ntraced () =\n    perform Trace.mark \"one\"\n");
    assert!(
        matches!(s, Stmt::Function { .. }),
        "expected function, got {s:?}"
    );
    if let Stmt::Function { effects, .. } = s {
        let names: Vec<&str> = effects.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Trace"]);
        assert_eq!(effects[0].type_args, Vec::<osprey_ast::TypeExpr>::new());
    }
}

#[test]
fn generic_signature_and_effect_row_args_thread_into_function() {
    // `tick<T> : Unit -> int ! State<int>` — the signature's type-param
    // binder and the row's type arguments both land on the canonical
    // `Stmt::Function`, byte-identical to the Default
    // `fn tick<T>() -> int !State<int>`. Implements [FLAVOR-ML-GENERICS],
    // [EFFECTS-GENERIC-ROWS].
    let s =
        ml_one_stmt("tick<T> : Unit -> int ! State<int>\ntick () =\n    perform State.get ()\n");
    if let Stmt::Function {
        type_params,
        effects,
        ..
    } = s
    {
        assert_eq!(type_params.len(), 1);
        assert_eq!(type_params[0].name, "T");
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].name, "State");
        assert_eq!(effects[0].type_args.len(), 1);
        assert_eq!(effects[0].type_args[0].name, "int");
    } else {
        panic!("expected function, got {s:?}");
    }
}

#[test]
fn variance_markers_lower_onto_type_and_effect_params() {
    // `type Source out T =` / `type Sink in T =` — variance markers lower
    // to the canonical `TypeParam` variance the Default `type Source<out T>`
    // carries. Implements [TYPE-VARIANCE-DECL].
    let s = ml_one_stmt("type Source out T =\n    produce : T\n");
    if let Stmt::Type { type_params, .. } = s {
        assert_eq!(type_params.len(), 1);
        assert_eq!(type_params[0].name, "T");
        assert_eq!(type_params[0].variance, Variance::Covariant);
    } else {
        panic!("expected type, got {s:?}");
    }
    let s = ml_one_stmt("type Sink in T =\n    accept : T -> Unit\n");
    if let Stmt::Type { type_params, .. } = s {
        assert_eq!(type_params[0].variance, Variance::Contravariant);
    } else {
        panic!("expected type, got {s:?}");
    }
    // `effect State T` — a generic effect declaration.
    // Implements [EFFECTS-GENERIC-DECL].
    let s = ml_one_stmt("effect State T\n    get : Unit => T\n    set : T => Unit\n");
    if let Stmt::Effect {
        type_params,
        operations,
        ..
    } = s
    {
        assert_eq!(type_params.len(), 1);
        assert_eq!(type_params[0].name, "T");
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0].ty, "fn() -> T");
        assert_eq!(operations[1].ty, "fn(T) -> Unit");
    } else {
        panic!("expected effect, got {s:?}");
    }
}

#[test]
fn perform_lowers_to_perform_expr() {
    // `perform Trace.mark "one"` lowers to the SAME `Expr::Perform` the Default
    // `perform Trace.mark("one")` emits ([FLAVOR-ML-EFFECT]).
    let s = ml_one_stmt("r = perform Trace.mark \"one\"\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Perform { .. },
                ..
            }
        ),
        "expected perform, got {s:?}"
    );
    if let Stmt::Let {
        value:
            Expr::Perform {
                effect,
                operation,
                arguments,
                named_arguments,
                ..
            },
        ..
    } = s
    {
        assert_eq!(effect, "Trace");
        assert_eq!(operation, "mark");
        assert_eq!(arguments, vec![Expr::Str("one".to_owned())]);
        assert_eq!(named_arguments, Vec::<osprey_ast::NamedArgument>::new());
    }
}

#[test]
fn handle_lowers_to_handler_expr() {
    // `handle Trace` + a `mark label => …` arm over the rest of its block
    // lowers to the SAME `Expr::Handler` the Default braced form emits
    // ([FLAVOR-ML-EFFECT], [EFFECTS-HANDLE-REST]).
    let src =
        "r =\n    handle Trace\n        mark label =>\n            resume ()\n    traced ()\n";
    let s = ml_one_stmt(src);
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Handler { .. },
                ..
            }
        ),
        "expected handler, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Handler {
            effect, arms, body, ..
        },
        ..
    } = s
    {
        assert_eq!(effect, "Trace");
        assert_eq!(arms.len(), 1);
        assert_eq!(arms[0].operation, "mark");
        assert_eq!(arms[0].params, vec!["label".to_owned()]);
        assert!(
            matches!(*body, Expr::Call { .. }),
            "handle body should be the call, got {body:?}"
        );
    }
}

#[test]
fn resume_lowers_with_and_without_argument() {
    // `resume ()` → `Resume(None)`; `resume seed` → `Resume(Some(seed))`,
    // byte-identical to the Default `resume()` / `resume(seed)`
    // ([FLAVOR-ML-EFFECT]). Bare `resume` denotes the owned continuation
    // VALUE and never silently invokes it ([EFFECTS-CONTINUATION-OWNERSHIP]).
    let bare = ml_one_stmt("r = resume ()\n");
    assert!(
        matches!(
            bare,
            Stmt::Let {
                value: Expr::Resume(None),
                ..
            }
        ),
        "expected bare resume, got {bare:?}"
    );
    let valued = ml_one_stmt("r = resume seed\n");
    assert!(
        matches!(
            valued,
            Stmt::Let {
                value: Expr::Resume(Some(_)),
                ..
            }
        ),
        "expected valued resume, got {valued:?}"
    );
    if let Stmt::Let {
        value: Expr::Resume(Some(inner)),
        ..
    } = valued
    {
        assert_eq!(*inner, Expr::Identifier("seed".to_owned()));
    }
}
