//! Integration coverage for the plan-0019 ML elegance surface: inline unions
//! ([FLAVOR-ML-UNION-INLINE]), positional payloads and their saturated
//! construction ([TYPE-UNION-POSITIONAL]), and equational clause sets
//! ([FLAVOR-ML-CLAUSES]) — including every normative restriction each of them
//! rejects.
#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test assertions: a failed match or out-of-bounds index is a test failure, not a production panic"
)]

use osprey_ast::{Expr, Pattern, Stmt};
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

/// The variants of the single `type` statement in `src`, as
/// `(name, field names)` pairs.
fn variants(src: &str) -> Vec<(String, Vec<String>)> {
    match ml_ok(src).into_iter().next() {
        Some(Stmt::Type { variants, .. }) => variants
            .into_iter()
            .map(|v| (v.name, v.fields.into_iter().map(|f| f.name).collect()))
            .collect(),
        other => panic!("expected a type declaration, got {other:?}"),
    }
}

/// The body of the single function named `name`.
fn function_body(src: &str, name: &str) -> (Vec<String>, Expr) {
    let found = ml_ok(src).into_iter().find_map(|stmt| match stmt {
        Stmt::Function {
            name: n,
            parameters,
            body,
            ..
        } if n == name => Some((parameters.into_iter().map(|p| p.name).collect(), body)),
        _ => None,
    });
    match found {
        Some(pair) => pair,
        None => panic!("no function named {name} in {src}"),
    }
}

// ─── [FLAVOR-ML-UNION-INLINE] ───────────────────────────────────────────────

#[test]
fn inline_union_declares_payload_free_variants() {
    assert_eq!(
        variants("type Colour = Red | Green | Blue\n"),
        vec![
            ("Red".to_owned(), vec![]),
            ("Green".to_owned(), vec![]),
            ("Blue".to_owned(), vec![]),
        ]
    );
}

/// A positional payload gets generated slot names; a named payload keeps the
/// spelled ones. Both spellings coexist in one declaration.
#[test]
fn inline_union_mixes_positional_and_named_payloads() {
    assert_eq!(
        variants("type Shape = Dot | Circle float | Rect(w : float, h : float)\n"),
        vec![
            ("Dot".to_owned(), vec![]),
            ("Circle".to_owned(), vec!["0".to_owned()]),
            ("Rect".to_owned(), vec!["w".to_owned(), "h".to_owned()]),
        ]
    );
}

/// A parenthesised type atom in payload position is a positional slot, not a
/// named-field list — the two `(` forms are told apart by the `field :` shape.
#[test]
fn inline_union_takes_parenthesised_positional_atoms() {
    assert_eq!(
        variants("type Holder = Wrap (List int) int\n"),
        vec![("Wrap".to_owned(), vec!["0".to_owned(), "1".to_owned()])]
    );
}

/// A lowercase right-hand side is a manifest alias, never a one-variant union.
#[test]
fn lowercase_right_hand_side_stays_an_alias() {
    match ml_ok("type UserId = int\n").into_iter().next() {
        Some(Stmt::Type {
            alias, variants, ..
        }) => {
            assert!(variants.is_empty(), "alias must not declare variants");
            assert_eq!(alias.map(|t| t.name), Some("int".to_owned()));
        }
        other => panic!("expected an alias, got {other:?}"),
    }
}

#[test]
fn inline_variant_must_be_uppercase() {
    assert!(ml_errors("type Bad = Good | oops\n").contains("must start with an uppercase letter"));
}

#[test]
fn inline_named_payload_wants_a_closing_paren() {
    assert!(ml_errors("type Bad = Rect(w : float\n").contains("expected ')'"));
}

// ─── [TYPE-UNION-POSITIONAL] ────────────────────────────────────────────────

/// A saturated application of a positionally-declared constructor folds into
/// the construction node, with arguments landing in slot order.
#[test]
fn saturated_positional_application_becomes_a_construction() {
    let (_, body) = function_body("type Pair = Both int int\nmk a b = Both a b\n", "mk");
    let inner = match body {
        Expr::Lambda { body, .. } => *body,
        other => other,
    };
    match inner {
        Expr::TypeConstructor { name, fields, .. } => {
            assert_eq!(name, "Both");
            assert_eq!(
                fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                vec!["0".to_owned(), "1".to_owned()]
            );
        }
        other => panic!("expected a positional construction, got {other:?}"),
    }
}

/// A nullary positional variant needs no application at all, and an
/// unsaturated one is left as written so the checker reports the arity against
/// the call the author actually spelled.
#[test]
fn unsaturated_positional_application_is_left_alone() {
    let (_, body) = function_body("type Pair = Both int int\nhalf a = Both a\n", "half");
    let inner = match body {
        Expr::Lambda { body, .. } => *body,
        other => other,
    };
    assert!(
        matches!(inner, Expr::Call { .. }),
        "an unsaturated constructor must stay a call: {inner:?}"
    );
}

// ─── [FLAVOR-ML-CLAUSES] ────────────────────────────────────────────────────

/// The canonical clause set: consecutive same-name, same-arity clauses collapse
/// into one function whose body matches the single refutable column, with arms
/// in clause order.
#[test]
fn clause_set_collapses_into_a_match() {
    let src = "type Tree = Leaf | Node Tree Tree\n\
               check Leaf = 0\n\
               check (Node l r) = 1 + check l + check r\n";
    let (params, body) = function_body(src, "check");
    assert_eq!(params.len(), 1);
    match body {
        Expr::Match { value, arms } => {
            assert_eq!(*value, Expr::Identifier(params[0].clone()));
            assert_eq!(arms.len(), 2);
            assert_eq!(
                arms[0].pattern,
                Pattern::Constructor {
                    name: "Leaf".to_owned(),
                    fields: vec![],
                    sub_patterns: vec![],
                }
            );
            match &arms[1].pattern {
                Pattern::Constructor { name, fields, .. } => {
                    assert_eq!(name, "Node");
                    assert_eq!(fields, &["l".to_owned(), "r".to_owned()]);
                }
                other => panic!("expected a Node pattern, got {other:?}"),
            }
        }
        other => panic!("expected a match body, got {other:?}"),
    }
}

/// The merged parameter takes the first plain binder any clause spells in the
/// selected column — so `make 0` / `make d` matches on `d`, not a generated
/// name.
#[test]
fn clause_set_borrows_the_first_plain_binder_as_the_parameter() {
    let (params, body) = function_body("make 0 = 1\nmake d = d\n", "make");
    assert_eq!(params, vec!["d".to_owned()]);
    match body {
        Expr::Match { value, arms } => {
            assert_eq!(*value, Expr::Identifier("d".to_owned()));
            assert_eq!(
                arms[0].pattern,
                Pattern::Literal(Box::new(Expr::Integer(0)))
            );
            assert_eq!(arms[1].pattern, Pattern::Binding("d".to_owned()));
        }
        other => panic!("expected a match body, got {other:?}"),
    }
}

/// Every clause selecting or ignoring the column leaves no binder to borrow, so
/// the scrutinee takes the generated name — unspellable, so it cannot capture.
#[test]
fn clause_set_generates_a_scrutinee_when_no_clause_binds_it() {
    let (params, body) = function_body("sign 0 = 0\nsign _ = 1\n", "sign");
    assert_eq!(params, vec![osprey_ast::clause_param_name(0)]);
    match body {
        Expr::Match { arms, .. } => {
            assert_eq!(arms[1].pattern, Pattern::Wildcard);
        }
        other => panic!("expected a match body, got {other:?}"),
    }
}

/// A clause that spells a non-selected column differently keeps its own
/// vocabulary: the arm body opens with `itsName = mergedName`.
#[test]
fn clause_set_renames_divergent_columns_in_the_arm_body() {
    let src = "pick 0 fallback = fallback\npick n other = n + other\n";
    let (params, body) = function_body(src, "pick");
    // Two columns curry ([FLAVOR-ML-CURRY]), so the match sits under a lambda.
    assert_eq!(params, vec!["n".to_owned()]);
    let arms = match body {
        Expr::Lambda { body, .. } => match *body {
            Expr::Match { arms, .. } => arms,
            other => panic!("expected a match under the lambda, got {other:?}"),
        },
        other => panic!("expected a curried lambda, got {other:?}"),
    };
    let renamed = arms.iter().any(|arm| {
        matches!(&arm.body, Expr::Block { statements, .. }
            if statements.iter().any(|s| matches!(s, Stmt::Let { name, .. } if name == "other")))
    });
    assert!(renamed, "expected an `other = fallback` rename: {arms:#?}");
}

/// A single clause with no refutable column is not a clause set at all — it
/// stays exactly the binding it was written as.
#[test]
fn a_plain_binding_is_not_rewritten() {
    let (params, body) = function_body("inc x = x + 1\n", "inc");
    assert_eq!(params, vec!["x".to_owned()]);
    assert!(
        !matches!(body, Expr::Match { .. }),
        "a plain binding must not gain a match: {body:?}"
    );
}

/// Same-name bindings separated by another declaration are not adjacent, so
/// they are never merged.
#[test]
fn non_adjacent_clauses_do_not_merge() {
    let src = "f 0 = 1\ng x = x\nf n = n\n";
    let matches = ml_ok(src)
        .into_iter()
        .filter(|s| matches!(s, Stmt::Function { name, .. } if name == "f"))
        .count();
    assert_eq!(matches, 2, "a gap must leave both clauses standing");
}

#[test]
fn a_clause_set_may_select_on_only_one_column() {
    assert!(ml_errors("f 0 y = y\nf x 0 = x\n").contains("selects on columns 0 and 1"));
}

/// Arity is part of what makes clauses one set, so a differing head ends the
/// run rather than merging into a ragged match.
#[test]
fn a_differing_arity_starts_a_new_run() {
    let src = "f 0 y = y\nf 1 = 2\nf x y = x\n";
    let fs = ml_ok(src)
        .into_iter()
        .filter(|s| matches!(s, Stmt::Function { name, .. } if name == "f"))
        .count();
    assert_eq!(fs, 3, "each arity is its own run: {src}");
}

/// A `mut` binding is a cell, never a case, so it never joins a run.
#[test]
fn a_mutable_binding_is_never_a_clause() {
    let stmts = ml_ok("mut total = 0\ntotal = 1\n");
    assert!(
        matches!(stmts.first(), Some(Stmt::Let { mutable: true, .. })),
        "expected a mutable let: {stmts:#?}"
    );
}

/// `|` separates union variants; it is meaningless in pattern position.
#[test]
fn or_patterns_are_rejected() {
    let src = "type Colour = Red | Green\nname c = match c\n    Red | Green => \"warm\"\n";
    assert!(ml_errors(src).contains("or-patterns are not supported"));
}

/// `(` groups exactly one pattern — Osprey has no tuple patterns.
#[test]
fn a_grouped_pattern_holds_one_pattern() {
    let src = "type Pair = Both int int\nfst p = match p\n    (Both a, b) => a\n";
    assert!(ml_errors(src).contains("no tuple patterns"));
}

/// Clause merging reaches inside the declaration containers, so a clause set in
/// a module body collapses exactly as a top-level one does.
#[test]
fn clause_sets_merge_inside_a_module_body() {
    let src = "module Trees\n    type Tree = Leaf | Node Tree Tree\n\
               \n    check Leaf = 0\n    check (Node l r) = 1\n";
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(parsed.errors.is_empty(), "unexpected errors: {parsed:#?}");
    let body = match parsed.program.statements.into_iter().next() {
        Some(Stmt::Module { body, .. }) => body,
        other => panic!("expected a module, got {other:?}"),
    };
    let checks = body
        .iter()
        .filter(|item| {
            matches!(item.declaration.as_ref(), Stmt::Function { name, .. } if name == "check")
        })
        .count();
    assert_eq!(checks, 1, "the clause set must collapse: {body:#?}");
}
