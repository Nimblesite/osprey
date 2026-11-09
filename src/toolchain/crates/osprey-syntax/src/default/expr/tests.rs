//! Expression lowering tests.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]

use crate::parse_tree;
use crate::test_support::find_kind;
use crate::test_support::stmts;
use osprey_ast::SelectOperation;
use osprey_ast::{Expr, InterpolatedPart, Pattern, Stmt};

fn let_value(src: &str) -> Expr {
    match stmts(src).into_iter().next() {
        Some(Stmt::Let { value, .. }) => value,
        other => panic!("expected let, got {other:?}"),
    }
}

#[test]
fn unquote_resolves_all_escapes() {
    // Every recognised escape (\n \r \t \e \0 \" \\), plus an unknown escape
    // (\q) that is kept verbatim.
    match let_value("let s = \"\\n\\r\\t\\e\\0\\\"\\\\\\q\"\n") {
        Expr::Str(s) => assert_eq!(s, "\n\r\t\u{1b}\0\"\\\\q"),
        other => panic!("expected string, got {other:?}"),
    }
}

/// A negated literal is a compile-time constant, so it folds to the literal
/// and keeps the plain `int`/`float` type; a negated *variable* stays a
/// checked `Unary` because `-i64::MIN` really can overflow.
/// Implements [ARITH-NEG-LITERAL].
#[test]
fn negated_literals_fold_but_negated_values_stay_checked() {
    assert!(matches!(let_value("let r = -1\n"), Expr::Integer(-1)));
    assert!(matches!(let_value("let r = -0\n"), Expr::Integer(0)));
    match let_value("let r = -2.5\n") {
        Expr::Float(f) => assert!((f + 2.5).abs() < f64::EPSILON, "got {f}"),
        other => panic!("expected folded float, got {other:?}"),
    }
    // The one value whose negation overflows stays an unfolded, checked
    // `Unary` rather than panicking or silently wrapping in the folder.
    assert!(matches!(
        let_value("let r = -(-9223372036854775808)\n"),
        Expr::Unary { .. }
    ));
    assert!(matches!(let_value("let r = -x\n"), Expr::Unary { .. }));
}

#[test]
fn lowers_unary_and_block_recovery() {
    // unary_expression arm.
    assert!(matches!(let_value("let r = -x\n"), Expr::Unary { .. }));
    // A function-body block whose trailing value is emitted as an
    // expression_statement: lower_block recovers it as the block value.
    match stmts("fn f() = {\n  print(1)\n  42\n}\n")
        .into_iter()
        .next()
    {
        Some(Stmt::Function {
            body: Expr::Block {
                statements, value, ..
            },
            ..
        }) => {
            assert_eq!(statements.len(), 1);
            assert!(matches!(value.as_deref(), Some(Expr::Integer(42))));
        }
        other => panic!("expected function with block body, got {other:?}"),
    }
}

#[test]
fn interpolation_keeps_trailing_text_and_inner_expr() {
    // `${1 + 2}` followed by trailing text exercises the tail-text push.
    match let_value("let s = \"v ${1 + 2} end\"\n") {
        Expr::InterpolatedStr(parts) => {
            assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "v "));
            assert!(matches!(
                parts[1],
                InterpolatedPart::Expr(Expr::Binary { .. })
            ));
            assert!(matches!(parts[2], InterpolatedPart::Text(ref t) if t == " end"));
        }
        other => panic!("expected interpolated, got {other:?}"),
    }
    // An interpolation that ends exactly at `}` leaves no trailing text, so
    // the final empty-text branch is taken (no extra Text part is pushed).
    match let_value("let s = \"${1}\"\n") {
        Expr::InterpolatedStr(parts) => {
            assert_eq!(parts.len(), 1);
            assert!(matches!(parts[0], InterpolatedPart::Expr(_)));
        }
        other => panic!("expected interpolated, got {other:?}"),
    }
}

#[test]
fn a_brace_head_is_an_update_or_a_construction_by_its_spelling() {
    // [TYPE-RECORD-UPDATE] Both flavors lower `p { x: 1 }` to the SAME
    // `Expr::Update` ([FLAVOR-IR-EQUIV]); a capitalised, qualified or
    // type-applied head constructs.
    assert!(matches!(
        let_value("let q = p { x: 1 }\n"),
        Expr::Update { record, .. } if record == "p"
    ));
    for source in [
        "let q = Point { x: 1 }\n",
        "let q = Ids::id { x: 1 }\n",
        "let q = Box<int> { held: 1 }\n",
    ] {
        assert!(
            matches!(let_value(source), Expr::TypeConstructor { .. }),
            "{source}"
        );
    }
}

#[test]
fn spawn_and_yield_lower() {
    assert!(matches!(let_value("let f = spawn g()\n"), Expr::Spawn(_)));
    assert!(matches!(let_value("let y = yield\n"), Expr::Yield(None)));
}

#[test]
fn lowers_common_expression_forms() {
    // Pipe desugars `x |> f(2)` to a call with x prepended
    // [BUILTIN-ITER-PIPE].
    match let_value("let r = x |> f(2)\n") {
        Expr::Call { arguments, .. } => assert_eq!(arguments.len(), 2),
        other => panic!("expected call, got {other:?}"),
    }
    // [BUILTIN-STRING-UFCS] Preserve the receiver until type checking can
    // select its callable field or the free-function fallback.
    assert_eq!(
        let_value("let r = o.m(1)\n"),
        Expr::MethodCall {
            target: Box::new(Expr::Identifier("o".into())),
            method: "m".into(),
            arguments: vec![Expr::Integer(1)],
            named_arguments: vec![],
        }
    );
    // Plain field access and indexing.
    assert!(matches!(
        let_value("let r = o.field\n"),
        Expr::FieldAccess { .. }
    ));
    assert!(matches!(let_value("let r = xs[0]\n"), Expr::Index { .. }));
    // Named-argument call.
    match let_value("let r = mk(a: 1, b: 2)\n") {
        Expr::Call {
            named_arguments, ..
        } => assert_eq!(named_arguments.len(), 2),
        other => panic!("expected call, got {other:?}"),
    }
    // Ternary desugars to a boolean match; Elvis desugars to an exhaustive
    // Success/Error match [PATTERN-RESULT-DEFAULT].
    assert!(matches!(
        let_value("let r = c ? 1 : 2\n"),
        Expr::Match { .. }
    ));
    assert!(matches!(let_value("let r = c ?: 2\n"), Expr::Match { .. }));
    // Object literal and lambda.
    assert!(matches!(
        let_value("let r = { a: 1, b: 2 }\n"),
        Expr::Object(_)
    ));
    assert!(matches!(
        let_value("let r = |x| => x\n"),
        Expr::Lambda { .. }
    ));
}

#[test]
fn lowers_match_select_handler_perform() {
    // match with a structural-field pattern body.
    assert!(matches!(
        let_value("let r = match x { Ok { value } => value  _ => 0 }\n"),
        Expr::Match { .. }
    ));
    // select expression (reuses arm lowering).
    match let_value("let r = select { m = recv(a) => m  recv(a) => 1  send(a, 2) => 3  timeout(4) => 5  _ => 6 }\n") {
        Expr::Select { operations, arms } => {
            assert!(matches!(
                operations.as_slice(),
                [
                    SelectOperation::Recv(_),
                    SelectOperation::Recv(_),
                    SelectOperation::Send { .. },
                    SelectOperation::Timeout(_),
                    SelectOperation::Default
                ]
            ));
            assert_eq!(arms[0].pattern, Pattern::Binding("m".to_owned()));
            assert_eq!(arms[1].pattern, Pattern::Wildcard);
        }
        other => panic!("expected a select, got {other:?}"),
    }
    // handler with params + perform inside its body.
    // The handler over the rest of its block is the block's VALUE, and a
    // block with nothing before it IS that value — the same node ML's
    // layout form lowers to ([FLAVOR-IR-EQUIV]).
    let handled =
        let_value("let r = {\n  handle Log { info m => m }\n  perform Log.info(x: 1)\n}\n");
    match handled {
        Expr::Handler { effect, arms, .. } => {
            assert_eq!(effect, "Log");
            assert_eq!(arms[0].operation, "info");
            assert_eq!(arms[0].params, vec!["m"]);
        }
        other => panic!("expected handler, got {other:?}"),
    }
}

#[test]
fn parse_fragment_falls_back_to_identifier() {
    // A fragment that does not parse to a `let` value returns the trimmed text.
    assert_eq!(
        super::parse_fragment(" fn "),
        Expr::Identifier("fn".to_string())
    );
}

#[test]
fn lowers_calls_literals_block_and_constructor() {
    // send/recv/await call forms.
    assert!(matches!(
        let_value("let r = send(ch, 1)\n"),
        Expr::Send { .. }
    ));
    assert!(matches!(let_value("let r = recv(ch)\n"), Expr::Recv(_)));
    assert!(matches!(let_value("let r = await(f)\n"), Expr::Await(_)));
    // type constructor carrying field assignments.
    match let_value("let r = Point { x: 1, y: 2 }\n") {
        Expr::TypeConstructor { name, fields, .. } => {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
        }
        other => panic!("expected constructor, got {other:?}"),
    }
    // List and string-key map literals [TYPE-MAP-LITERAL].
    assert!(matches!(
        let_value("let r = [1, 2, 3]\n"),
        Expr::List(v, position) if v.len() == 3 && position.is_some()
    ));
    assert!(matches!(let_value("let r = { \"a\": 1, \"b\": 2 }\n"), Expr::Map(m) if m.len() == 2));
    // block with a statement plus a trailing value expression.
    match let_value("let r = {\n  let a = 1\n  a\n}\n") {
        Expr::Block {
            statements, value, ..
        } => {
            assert_eq!(statements.len(), 1);
            assert!(value.is_some());
        }
        other => panic!("expected block, got {other:?}"),
    }
    // structural ternary `cond { f } ? then : else` -> block binding fields.
    assert!(matches!(
        let_value("let r = rec { a } ? a : 0\n"),
        Expr::Block { .. }
    ));
    // `if c { a } else { b }` reuses the ternary's boolean match, and an
    // `else if` nests into the `false` arm ([GRAMMAR-IF-ELSE]).
    match let_value("let r = if c { 1 } else if d { 2 } else { 3 }\n") {
        Expr::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert_eq!(arms[0].body, Expr::Integer(1));
            assert!(matches!(arms[1].body, Expr::Match { .. }));
        }
        other => panic!("expected a boolean match, got {other:?}"),
    }
}

/// Horizontal space between a callee and its argument list is legal, and
/// binds exactly as tightly as the unspaced spelling. Requiring
/// `token.immediate('(')` to keep a following `(a, b)` tuple-pattern arm
/// parseable silently rejected `print(id (1))` — ordinary source the
/// language has always accepted — so the same-line test moved into the
/// scanner instead ([PATTERN-TUPLE], src/toolchain/tree-sitter-osprey/src/scanner.c).
#[test]
fn a_call_accepts_horizontal_space_before_its_argument_list() {
    for src in ["let r = id(1)\n", "let r = id (1)\n", "let r = id  (1)\n"] {
        match let_value(src) {
            Expr::Call {
                function,
                arguments,
                ..
            } => {
                assert!(
                    matches!(*function, Expr::Identifier(ref n) if n == "id"),
                    "{src}"
                );
                assert_eq!(arguments, vec![Expr::Integer(1)], "{src}");
            }
            other => panic!("expected a call for {src:?}, got {other:?}"),
        }
    }
}

/// The spaced call still binds at `PREC.member`, so `1 + id (2)` is
/// `1 + id(2)` — never `(1 + id)(2)`. Resolving this with a GLR fork could
/// not preserve it: both readings contain exactly one spaced call, so their
/// dynamic precedences tie and the resolver chose the wrong one.
#[test]
fn a_spaced_call_binds_tighter_than_an_operator() {
    match let_value("let r = 1 + id (2)\n") {
        Expr::Binary { left, right, .. } => {
            assert!(matches!(*left, Expr::Integer(1)));
            assert!(matches!(*right, Expr::Call { .. }), "got {right:?}");
        }
        other => panic!("expected `1 + id(2)`, got {other:?}"),
    }
}

/// Indexing is STRICTER than calling: the `[` must touch its target, so
/// `xs[0]` indexes and `xs [0]` does not. The asymmetry is deliberate and
/// this test is why it cannot simply be unified — list-arm matches are
/// written on a SINGLE line, so extending the call's same-line rule to `[`
/// reads the `0  [head` in
///
///     match xs { [] => 0  [head, ...tail] => … }
///
/// as an index and breaks `src/toolchain/benchmarks/cases/listops/listops.osp`. Tuple
/// arms have no single-line usage, which is what lets `(` be looser.
/// [TYPE-LIST-PATTERNS]
#[test]
fn an_index_bracket_must_touch_its_target_unlike_a_call() {
    const SPACED: &str = "let r = xs [0]\n";
    match let_value("let r = xs[0]\n") {
        Expr::Index { target, index } => {
            assert!(matches!(*target, Expr::Identifier(ref n) if n == "xs"));
            assert!(matches!(*index, Expr::Integer(0)));
        }
        other => panic!("expected an index, got {other:?}"),
    }
    // Spaced: NOT an index. The `[0]` can no longer become a silently
    // discarded statement either, so the line is rejected outright
    // [LEX-STATEMENT-BREAK]. BOTH halves are asserted: rejection alone
    // would still hold if the `[` had wrongly bound as an index and some
    // later rule complained, which is the very confusion under test.
    assert!(
        !crate::parse_program(SPACED).errors.is_empty(),
        "a spaced `[` must not split the statement"
    );
    let Some(tree) = parse_tree(SPACED) else {
        panic!("tree-sitter produced no tree for {SPACED:?}");
    };
    // An index binds inside `call_expression` (the postfix chain), so its
    // absence is what proves the `[` did not attach to `xs`.
    assert!(
        find_kind(tree.root_node(), "call_expression").is_none(),
        "a spaced `[` must not bind as an index; tree was {}",
        tree.root_node().to_sexp()
    );
    // The single-line list-arm match the strict rule exists to protect.
    match let_value("let r = match xs { [] => 0  [head, ...tail] => head }\n") {
        Expr::Match { arms, .. } => assert_eq!(arms.len(), 2, "got {arms:?}"),
        other => panic!("expected a match, got {other:?}"),
    }
}

/// The boundary the scanner draws: a `(` that opens the NEXT line is the
/// following arm's tuple pattern, never an argument list extending the
/// previous arm's body. Both arms must survive, and the tuple arm binds its
/// slots positionally.
#[test]
fn a_newline_paren_opens_a_tuple_arm_not_a_call() {
    let src = "let r = match v {\n    { held, .. } => render (held)\n    (n, s) => n\n}\n";
    match let_value(src) {
        Expr::Match { arms, .. } => {
            assert_eq!(arms.len(), 2, "got {arms:?}");
            // The first arm's body kept its spaced call ...
            let Expr::Block {
                statements,
                value: Some(body),
                position,
            } = &arms[0].body
            else {
                panic!("arm 0 lost its source anchor: {:?}", arms[0].body);
            };
            assert_eq!(statements.as_slice(), []);
            assert_eq!(*position, Some(osprey_ast::Position { line: 2, column: 4 }));
            assert!(
                matches!(body.as_ref(), Expr::Call { .. }),
                "arm 0 body: {:?}",
                arms[0].body
            );
            // ... and did not swallow `(n, s)` as a second argument list.
            match arms[1].pattern {
                Pattern::Structural { ref fields, open } => {
                    assert!(!open);
                    assert_eq!(
                        fields,
                        &[
                            (String::from("0"), String::from("n")),
                            (String::from("1"), String::from("s")),
                        ]
                    );
                }
                ref other => panic!("expected a tuple pattern, got {other:?}"),
            }
        }
        other => panic!("expected a match, got {other:?}"),
    }
}

#[test]
fn defensive_expr_fallbacks() {
    // Direct calls on a `line_comment` (or a missing field) hit the
    // best-effort `Expr::Bool(false)` arms that well-formed trees never reach.
    let src = "// c\nlet x = 1\n";
    let tree = parse_tree(src).unwrap();
    let lw = super::Lowerer::new(src.as_bytes());
    let comment = find_kind(tree.root_node(), "line_comment").unwrap();
    let let_node = find_kind(tree.root_node(), "let_declaration").unwrap();

    assert_eq!(lw.lower_expr(comment), Expr::Bool(false)); // `_` arm
    assert_eq!(lw.lower_literal(comment), Expr::Bool(false)); // no named child
    assert_eq!(lw.lower_literal_node(comment), Expr::Bool(false)); // `_` arm
    assert_eq!(lw.lower_inner_expr(comment), Expr::Bool(false)); // no named child
    assert_eq!(lw.lower_expr_field(let_node, "nope"), Expr::Bool(false)); // missing field
}
