//! ML generics, variance and generic effects ([FLAVOR-ML-GENERICS]).

use osprey_ast::{Expr, Stmt, TypeExpr};
use osprey_syntax::{parse_program_with_flavor, Flavor};

use super::{assert_ml_error, ml_ok, ml_one};

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
    assert_ne!(parsed.program.statements, Vec::<Stmt>::new());
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
    assert_ml_error("type Odd out =\n    supply : int\n", &["after 'out'"]);
    // `in` must be followed by a parameter name.
    assert_ml_error("type Bad in =\n    x : int\n", &["after 'in'"]);
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
            assert_eq!(effects[1].type_args, Vec::<TypeExpr>::new());
        }
        s => panic!("expected function, got {s:?}"),
    }
    // An unclosed row argument list reports an error but recovers.
    let parsed = parse_program_with_flavor("h : Unit -> int ! Stash<int\nh () = 1\n", Flavor::Ml);
    assert_ne!(parsed.errors, Vec::<osprey_syntax::SyntaxError>::new());
}

#[test]
fn construction_site_type_args_parse_in_ml() {
    // `Box<int>(item = 7)` — explicit construction-site type arguments on the
    // inline record form, twinning Default `Box<int> { item: 7 }`
    // ([TYPE-GENERICS-DECL], [FLAVOR-ML-GENERICS]).
    let stmts = ml_ok("type Box T =\n    item : T\npinned = Box<int>(item = 7)\n");
    match &stmts[1] {
        Stmt::Let {
            value: Expr::TypeConstructor {
                name, type_args, ..
            },
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
