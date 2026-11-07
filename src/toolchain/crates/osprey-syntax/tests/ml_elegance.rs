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
    ml_program(src).statements
}

/// A source that must parse cleanly, as its program.
fn ml_program(src: &str) -> osprey_ast::Program {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(parsed.errors.is_empty(), "unexpected errors: {parsed:#?}");
    parsed.program
}

/// Assert `src` is rejected with a diagnostic containing `expected`.
fn assert_rejected(src: &str, expected: &str) {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        !parsed.errors.is_empty(),
        "expected errors but got none: {:#?}",
        parsed.program
    );
    let errors = parsed
        .errors
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        errors.contains(expected),
        "expected {expected:?} from {src:?}, got: {errors}"
    );
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

/// Assert the variants `src` declares, spelled `(name, field names)`.
fn assert_variants(src: &str, expected: &[(&str, &[&str])]) {
    let want: Vec<(String, Vec<String>)> = expected
        .iter()
        .map(|(name, fields)| {
            (
                (*name).to_owned(),
                fields.iter().map(|f| (*f).to_owned()).collect(),
            )
        })
        .collect();
    assert_eq!(variants(src), want, "variants of {src:?}");
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

/// The body under a curried function's lambda wrapper, unwrapped if present.
fn under_lambda(body: Expr) -> Expr {
    match body {
        Expr::Lambda { body, .. } => *body,
        other => other,
    }
}

/// The scrutinee and arms of the match a clause-set body must hold.
fn match_parts(body: Expr) -> (Expr, Vec<osprey_ast::MatchArm>) {
    match under_lambda(body) {
        Expr::Match { value, arms } => (*value, arms),
        other => panic!("expected a match body, got {other:?}"),
    }
}

/// The first match expression's arms in a source that must parse cleanly.
fn match_arms_of(src: &str) -> Vec<osprey_ast::MatchArm> {
    let found = ml_program(src).statements.iter().find_map(|s| match s {
        Stmt::Function {
            body: Expr::Match { arms, .. },
            ..
        } => Some(arms.clone()),
        _ => None,
    });
    match found {
        Some(arms) => arms,
        None => panic!("expected a match expression in the program"),
    }
}

/// How many of `stmts` are functions named `name`.
fn count_named<'a>(stmts: impl Iterator<Item = &'a Stmt>, name: &str) -> usize {
    stmts
        .filter(|s| matches!(s, Stmt::Function { name: n, .. } if n == name))
        .count()
}

/// A constructor pattern naming `name` over positional binders, as ML lowers it.
fn ctor(name: &str, binders: &[&str]) -> Pattern {
    Pattern::Constructor {
        name: name.to_owned(),
        fields: vec![],
        sub_patterns: binders
            .iter()
            .map(|b| Pattern::Binding((*b).to_owned()))
            .collect(),
    }
}

// ─── [FLAVOR-ML-UNION-INLINE] ───────────────────────────────────────────────

/// Payload-free variants; positional payloads keeping their generated slot names
/// beside named ones; a parenthesised type atom in payload position read as a
/// positional slot, not a named-field list — the two `(` forms are told apart by
/// the `field :` shape.
/// Each variant name with the slot names its payload keeps.
type VariantSlots<'a> = &'a [(&'a str, &'a [&'a str])];

#[test]
fn inline_union_variants_keep_their_slot_names() {
    let cases: &[(&str, VariantSlots<'_>)] = &[
        (
            "type Colour = Red | Green | Blue\n",
            &[("Red", &[]), ("Green", &[]), ("Blue", &[])],
        ),
        (
            "type Shape = Dot | Circle float | Rect(w : float, h : float)\n",
            &[("Dot", &[]), ("Circle", &["0"]), ("Rect", &["w", "h"])],
        ),
        (
            "type Holder = Wrap (List int) int\n",
            &[("Wrap", &["0", "1"])],
        ),
    ];
    for (src, expected) in cases {
        assert_variants(src, expected);
    }
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

// ─── [TYPE-UNION-POSITIONAL] ────────────────────────────────────────────────

/// A saturated application of a positionally-declared constructor folds into
/// the construction node, with arguments landing in slot order
/// ([FLAVOR-ML-CTOR-POSITIONAL]). A nullary positional variant needs no
/// application at all, and an unsaturated one is left as written so the checker
/// reports the arity against the call the author actually spelled.
#[test]
fn positional_applications_fold_only_when_saturated() {
    let (_, body) = function_body("type Pair = Both int int\nmk a b = Both a b\n", "mk");
    match under_lambda(body) {
        Expr::TypeConstructor { name, fields, .. } => {
            assert_eq!(name, "Both");
            assert_eq!(
                fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                vec!["0".to_owned(), "1".to_owned()]
            );
        }
        other => panic!("expected a positional construction, got {other:?}"),
    }
    let (_, body) = function_body("type Pair = Both int int\nhalf a = Both a\n", "half");
    let inner = under_lambda(body);
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
    let (value, arms) = match_parts(body);
    assert_eq!(value, Expr::Identifier(params[0].clone()));
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].pattern, ctor("Leaf", &[]));
    // ML's only constructor destructure is positional, so its binders lower into
    // `sub_patterns` — the same node the Default twin `Node(l, r)` emits.
    // Lowering them into `fields` made each binder resolve to the payload slot
    // that happened to share its spelling ([TYPE-UNION-POSITIONAL]).
    assert_eq!(arms[1].pattern, ctor("Node", &["l", "r"]));
}

/// The merged parameter takes the first plain binder any clause spells in the
/// selected column — so `make 0` / `make d` matches on `d`, not a generated
/// name.
#[test]
fn clause_set_borrows_the_first_plain_binder_as_the_parameter() {
    let (params, body) = function_body("make 0 = 1\nmake d = d\n", "make");
    assert_eq!(params, vec!["d".to_owned()]);
    let (value, arms) = match_parts(body);
    assert_eq!(value, Expr::Identifier("d".to_owned()));
    assert_eq!(
        arms[0].pattern,
        Pattern::Literal(Box::new(Expr::Integer(0)))
    );
    assert_eq!(arms[1].pattern, Pattern::Binding("d".to_owned()));
}

/// Every clause selecting or ignoring the column leaves no binder to borrow, so
/// the scrutinee takes the generated name — unspellable, so it cannot capture.
#[test]
fn clause_set_generates_a_scrutinee_when_no_clause_binds_it() {
    let (params, body) = function_body("sign 0 = 0\nsign _ = 1\n", "sign");
    assert_eq!(params, vec![osprey_ast::clause_param_name(0)]);
    let (_, arms) = match_parts(body);
    assert_eq!(arms[1].pattern, Pattern::Wildcard);
}

/// List patterns are valid match patterns and the binding-head lookahead
/// already accepts bracketed atoms, so they must also parse as equation clauses.
#[test]
fn list_patterns_are_accepted_in_clause_heads() {
    let src = "size [] = 0\n\
               size [_, ...tail] = 1 + size tail\n";
    let (params, body) = function_body(src, "size");
    assert_eq!(params.len(), 1);
    let (_, arms) = match_parts(body);
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[0].pattern, Pattern::List { .. }));
    assert!(matches!(arms[1].pattern, Pattern::List { .. }));
}

/// A clause that spells a non-selected column differently keeps its own
/// vocabulary: the arm body opens with `itsName = mergedName`.
#[test]
fn clause_set_renames_divergent_columns_in_the_arm_body() {
    let src = "pick 0 fallback = fallback\npick n other = n + other\n";
    let (params, body) = function_body(src, "pick");
    // Two columns curry ([FLAVOR-ML-CURRY]), so the match sits under a lambda.
    assert_eq!(params, vec!["n".to_owned()]);
    assert!(
        matches!(body, Expr::Lambda { .. }),
        "expected a curried lambda, got {body:?}"
    );
    let (_, arms) = match_parts(body);
    let renamed = arms.iter().any(|arm| {
        let Expr::Block {
            statements,
            value: Some(body),
            position: Some(_),
        } = &arm.body
        else {
            panic!("clause arm lost its source position: {:?}", arm.body);
        };
        assert_eq!(statements.as_slice(), []);
        matches!(body.as_ref(), Expr::Block { statements, .. }
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

/// A run ends at a gap or at a differing arity: same-name bindings separated by
/// another declaration are never adjacent, and arity is part of what makes
/// clauses one set, so neither merges into a ragged match.
#[test]
fn a_gap_or_a_differing_arity_ends_a_run() {
    let cases: &[(&str, usize)] = &[
        ("f 0 = 1\ng x = x\nf n = n\n", 2),
        ("f 0 y = y\nf 1 = 2\nf x y = x\n", 3),
    ];
    for (src, runs) in cases {
        let stmts = ml_ok(src);
        assert_eq!(
            count_named(stmts.iter(), "f"),
            *runs,
            "each head is its own run: {src}"
        );
    }
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

/// `(a, b)` lowers to the canonical positional row — the same
/// [`Pattern::Structural`] both flavors share ([PATTERN-TUPLE],
/// [FLAVOR-ML-TUPLE]).
#[test]
fn a_tuple_pattern_lowers_to_the_positional_row() {
    let src = "type Pair = Both(int, string)\nfst p = match p\n    (a, _) => a\n";
    let arms = match_arms_of(src);
    assert!(
        matches!(
            &arms[0].pattern,
            Pattern::Structural { fields, open: false }
                if fields
                    .iter()
                    .map(|(f, b)| (f.as_str(), b.as_str()))
                    .eq([("0", "a"), ("1", "")])
        ),
        "expected the positional row, got {:?}",
        arms[0].pattern
    );
}

/// `{ heading, .. }` opens the row; `{ heading }` stays closed
/// ([PATTERN-STRUCTURAL]).
#[test]
fn structural_patterns_parse_closed_and_open() {
    let src = "title page = match page\n    { heading, .. } => heading\n    { name } => name\n    _ => \"untitled\"\n";
    let arms = match_arms_of(src);
    assert!(matches!(
        &arms[0].pattern,
        Pattern::Structural { fields, open: true }
            if fields.iter().map(|(f, _)| f.as_str()).eq(["heading"])
    ));
    assert!(matches!(
        &arms[1].pattern,
        Pattern::Structural { fields, open: false }
            if fields.iter().map(|(f, _)| f.as_str()).eq(["name"])
    ));
}

/// Clause merging reaches inside the declaration containers, so a clause set in
/// a module body collapses exactly as a top-level one does.
#[test]
fn clause_sets_merge_inside_a_module_body() {
    let src = "module Trees\n    type Tree = Leaf | Node Tree Tree\n\
               \n    check Leaf = 0\n    check (Node l r) = 1\n";
    let body = match ml_program(src).statements.into_iter().next() {
        Some(Stmt::Module { body, .. }) => body,
        other => panic!("expected a module, got {other:?}"),
    };
    assert_eq!(
        count_named(body.iter().map(|item| item.declaration.as_ref()), "check"),
        1,
        "the clause set must collapse: {body:#?}"
    );
}

// ─── normative restrictions ─────────────────────────────────────────────────

/// Every restriction the elegance surface imposes reports the diagnostic that
/// names it: an uppercase variant head, a closed named payload list, no clause
/// after an irrefutable arm, one selected column per clause set, no
/// or-patterns, and no constructor in a tuple slot ([PATTERN-TUPLE]).
#[test]
fn rejected_surfaces_report_their_diagnostic() {
    for (src, expected) in [
        (
            "type Bad = Good | oops\n",
            "must start with an uppercase letter",
        ),
        ("type Bad = Rect(w : float\n", "expected ')'"),
        // An irrefutable arm consumes every value, so accepting a later clause
        // would silently make source order change program meaning.
        (
            "choose 0 = 0\nchoose n = n\nchoose _ = 2\n",
            "unreachable clause",
        ),
        ("choose n = n\nchoose other = other\n", "unreachable clause"),
        ("f 0 y = y\nf x 0 = x\n", "selects on columns 0 and 1"),
        // `|` separates union variants; it is meaningless in pattern position.
        (
            "type Colour = Red | Green\nname c = match c\n    Red | Green => \"warm\"\n",
            "or-patterns are not supported",
        ),
        // `( p )` groups exactly one pattern ([FLAVOR-ML-PATTERN-GROUP]); a comma
        // list is a tuple pattern whose slots each bind a name or `_`.
        (
            "type Pair = Both int int\nfst p = match p\n    (Both a, b) => a\n",
            "a tuple pattern slot binds a name or `_`",
        ),
    ] {
        assert_rejected(src, expected);
    }
}
