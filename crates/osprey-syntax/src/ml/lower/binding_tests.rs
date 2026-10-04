//! Existing ML lowering assertions.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a panic"
)]
use crate::ml::parse_ml;
use crate::test_support::{
    assert_doc_pair, assert_summary, ml_one_stmt, ml_program, ml_stmts, stmt_doc,
};
use osprey_ast::{Expr, Pattern, Stmt};

// ---------- [TESTING-DOC] expression-statement documentation ----------

#[test]
fn an_ml_block_doc_lowers_onto_the_expression_statement_it_precedes() {
    // `test "name" case` is an application expression, so documenting a
    // case requires the doc to attach to the expression statement
    // ([TESTING-DOC], [DOC-SIGIL-ML]).
    let s = ml_one_stmt("(** Documents the call. *)\nprintLine \"hi\"\n");
    assert!(matches!(s, Stmt::Expr { .. }), "still an expr stmt: {s:?}");
    assert_summary(&s, Some("Documents the call."));
}

#[test]
fn an_undocumented_ml_expression_statement_carries_no_doc() {
    let s = ml_one_stmt("printLine \"hi\"\n");
    assert!(stmt_doc(&s).is_none(), "no doc invented: {s:?}");
}

#[test]
fn an_ml_doc_is_consumed_by_the_first_statement_and_not_the_next() {
    let all = ml_stmts("(** First. *)\nprintLine \"a\"\nprintLine \"b\"\n");
    assert_eq!(all.len(), 2);
    assert_summary(&all[0], Some("First."));
    assert_eq!(
        stmt_doc(&all[1]).map(|d| d.summary.as_str()),
        None,
        "the doc does not leak forward"
    );
}

#[test]
fn a_stray_ml_inner_doc_is_reported_not_silently_swallowed() {
    // A `//!` documents the scope it OPENS. One written anywhere else has
    // no scope to document, and dropping it quietly would read exactly
    // like a doc that was never written ([DOC-SIGIL-INNER]).
    let trailing = parse_ml("x = 1\n//! nothing encloses me.\n");
    assert!(
        trailing
            .errors
            .iter()
            .any(|e| e.message.contains("`//!` documents the enclosing")),
        "a trailing //! must be reported, got {:?}",
        trailing.errors
    );
    // ...and it must not have been hoisted into the file's own doc.
    assert!(
        trailing.program.doc.is_none(),
        "a stray //! must never become the file doc"
    );
}

#[test]
fn ml_inner_docs_attach_to_file_namespace_and_module() {
    // [FLAVOR-LOWER-CONTRACT]: an inner `//!` doc must lower to the same
    // place in the canonical AST under BOTH flavors. The ML flavor already
    // lexes `//` line comments, so `//!` is its inner sigil too — the outer
    // sigils differ (`///` vs `(** *)`), the inner one does not.
    // Implements [DOC-SIGIL-INNER].
    let file = ml_program("//! The whole file.\nx = 1\n");
    assert_eq!(
        file.doc.as_ref().map(|d| d.summary.as_str()),
        Some("The whole file."),
        "ML file-level //! must reach Program.doc"
    );
    assert_eq!(
        file.doc.as_ref().map(osprey_ast::DocComment::scope),
        Some(osprey_ast::DocScope::Inner)
    );
    // The binding after it keeps its own (empty) doc slot.
    assert_summary(&file.statements[0], None);

    match ml_one_stmt("(** From outside. *)\nmodule M\n    //! From inside.\n    export x = 1\n") {
        Stmt::Module { doc, inner_doc, .. } => {
            assert_eq!(doc.map(|d| d.summary), Some("From outside.".to_owned()));
            assert_eq!(
                inner_doc.map(|d| d.summary),
                Some("From inside.".to_owned())
            );
        }
        s => panic!("expected module, got {s:?}"),
    }
}

#[test]
fn an_ml_binding_after_a_documented_statement_keeps_its_own_doc() {
    let all = ml_stmts("(** Runs it. *)\nprintLine \"hi\"\n(** Adds. *)\nadd a b = a + b\n");
    assert_doc_pair(&all, "Runs it.", "Adds.");
}

#[test]
fn an_ml_doc_with_sections_lowers_every_recognised_field() {
    let s = ml_one_stmt(
            "(** Summary line.\n\n    # Parameters\n    - left: the first addend\n\n    # Since\n    0.3 *)\nprintLine \"hi\"\n",
        );
    let doc = stmt_doc(&s).expect("doc attached");
    assert_eq!(doc.summary, "Summary line.");
    assert_eq!(
        doc.params,
        vec![("left".to_owned(), "the first addend".to_owned())]
    );
    assert_eq!(doc.since.as_deref(), Some("0.3"));
}

#[test]
fn value_binding_lowers_to_let() {
    let s = ml_one_stmt("answer = 42\n");
    assert!(matches!(s, Stmt::Let { .. }), "expected let, got {s:?}");
    if let Stmt::Let {
        name,
        mutable,
        value,
        ..
    } = s
    {
        assert_eq!(name, "answer");
        assert!(!mutable);
        assert_eq!(value, Expr::Integer(42));
    }
}

#[test]
fn mut_and_assignment_lower_distinctly() {
    let s = ml_stmts("mut requests = 0\nrequests := requests + 1\n");
    assert!(matches!(s[0], Stmt::Let { mutable: true, .. }));
    assert!(matches!(s[1], Stmt::Assignment { ref name, .. } if name == "requests"));
}

#[test]
fn multi_param_function_is_curried_nested_lambda() {
    // `add x y = x + y` curries by default ([FLAVOR-ML-CURRY]): a ONE-parameter
    // `Stmt::Function` over `x` whose body is a one-parameter `Expr::Lambda`
    // over `y` — byte-identical to the Default *explicit-curry*
    // `fn add(x) = fn(y) => x + y`, deliberately NOT the multi-parameter
    // `fn add(x, y)`.
    let s = ml_one_stmt("add x y = x + y\n");
    assert!(
        matches!(s, Stmt::Function { .. }),
        "expected function, got {s:?}"
    );
    if let Stmt::Function {
        name,
        parameters,
        body,
        ..
    } = s
    {
        assert_eq!(name, "add");
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "x");
        assert!(
            matches!(&body, Expr::Lambda { parameters, .. }
                    if parameters.len() == 1 && parameters[0].name == "y"),
            "expected curried lambda over y, got {body:?}"
        );
        if let Expr::Lambda { body: inner, .. } = body {
            assert!(matches!(*inner, Expr::Binary { ref op, .. } if op == "+"));
        }
    }
}

#[test]
fn single_param_function_has_no_extra_lambda() {
    let s = ml_one_stmt("inc x = x + 1\n");
    assert!(
        matches!(s, Stmt::Function { .. }),
        "expected function, got {s:?}"
    );
    if let Stmt::Function {
        parameters, body, ..
    } = s
    {
        assert_eq!(parameters.len(), 1);
        assert!(matches!(body, Expr::Binary { .. }));
    }
}

#[test]
fn unit_function_has_zero_parameters() {
    // `f () = body` is a zero-parameter function, like the Default `fn f()`.
    let s = ml_one_stmt("greet () = 1\n");
    assert!(
        matches!(s, Stmt::Function { .. }),
        "expected function, got {s:?}"
    );
    if let Stmt::Function { parameters, .. } = s {
        assert_eq!(parameters, Vec::<osprey_ast::Parameter>::new());
    }
}

#[test]
fn whitespace_application_is_curried_nested_call() {
    // A user-defined `add` curries by default ([FLAVOR-ML-CURRY]): the
    // whitespace call `add 1 2` is nested one-argument calls
    // `Call(Call(add, [1]), [2])` — byte-identical to the Default explicit-curry
    // `add(1)(2)`, NOT a flat `add(1, 2)`. (An UNBOUND head is treated as a
    // multi-argument builtin and folds to a flat saturated call instead.)
    let value = ml_stmts("add a b = a + b\nr = add 1 2\n")
        .into_iter()
        .find_map(|st| match st {
            Stmt::Let { name, value, .. } if name == "r" => Some(value),
            _ => None,
        });
    match value {
        Some(Expr::Call {
            function,
            arguments,
            ..
        }) => {
            // The outer call applies the inner `(add 1)` to the single argument `2`.
            assert_eq!(arguments, vec![Expr::Integer(2)]);
            assert!(
                matches!(&*function, Expr::Call { arguments, .. }
                        if arguments == &vec![Expr::Integer(1)]),
                "expected inner call add(1), got {function:?}"
            );
            if let Expr::Call {
                function: inner, ..
            } = *function
            {
                assert_eq!(*inner, Expr::Identifier("add".to_owned()));
            }
        }
        other => panic!("expected nested curried call for r, got {other:?}"),
    }
}

#[test]
fn application_binds_tighter_than_operators() {
    // `add 1 2 == 3` ⇒ (add 1 2) == 3.
    let s = ml_one_stmt("r = add 1 2 == 3\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Binary { .. },
                ..
            }
        ),
        "expected comparison, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Binary {
            op, left, right, ..
        },
        ..
    } = s
    {
        assert_eq!(op, "==");
        assert!(matches!(*left, Expr::Call { .. }));
        assert_eq!(*right, Expr::Integer(3));
    }
}

#[test]
fn unit_application_is_zero_arg_call() {
    let s = ml_one_stmt("r = make ()\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Call { .. },
                ..
            }
        ),
        "expected zero-arg call, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Call {
            function,
            arguments,
            ..
        },
        ..
    } = s
    {
        assert_eq!(arguments, Vec::<Expr>::new());
        assert_eq!(*function, Expr::Identifier("make".to_owned()));
    }
}

#[test]
fn match_lowers_constructor_and_wildcard_arms() {
    let s = ml_one_stmt("r =\n    match x\n        Success value => value\n        _ => 0\n");
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Match { .. },
                ..
            }
        ),
        "expected match, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Match { arms, .. },
        ..
    } = s
    {
        assert_eq!(arms.len(), 2);
        let p0 = &arms[0].pattern;
        assert!(
            matches!(p0, Pattern::Constructor { .. }),
            "expected constructor pattern, got {p0:?}"
        );
        if let Pattern::Constructor { name, fields, .. } = p0 {
            assert_eq!(name, "Success");
            assert_eq!(fields, &vec!["value".to_owned()]);
        }
        assert!(matches!(arms[1].pattern, Pattern::Wildcard));
    }
}

#[test]
fn list_patterns_lower_to_canonical_list_pattern() {
    // `[]`, `[x]`, `[a, b]`, `[head, ...tail]`, `[_, b, ...rest]` lower to the
    // SAME `Pattern::List { elements, rest }` the Default flavor emits
    // ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]).
    let src = "r =\n    match xs\n        [] => 0\n        [head, ...tail] => 1\n        [_, b, ...rest] => 2\n";
    let s = ml_one_stmt(src);
    assert!(
        matches!(
            s,
            Stmt::Let {
                value: Expr::Match { .. },
                ..
            }
        ),
        "expected match, got {s:?}"
    );
    if let Stmt::Let {
        value: Expr::Match { arms, .. },
        ..
    } = s
    {
        assert!(
            matches!(&arms[0].pattern, Pattern::List { elements, rest } if elements.is_empty() && rest.is_none())
        );
        let p1 = &arms[1].pattern;
        assert!(
            matches!(p1, Pattern::List { .. }),
            "expected list pattern, got {p1:?}"
        );
        if let Pattern::List { elements, rest } = p1 {
            assert_eq!(elements, &vec![Pattern::Binding("head".to_owned())]);
            assert_eq!(rest, &Some("tail".to_owned()));
        }
        let p2 = &arms[2].pattern;
        assert!(
            matches!(p2, Pattern::List { .. }),
            "expected list pattern, got {p2:?}"
        );
        if let Pattern::List { elements, rest } = p2 {
            assert_eq!(elements.len(), 2);
            assert!(matches!(elements[0], Pattern::Wildcard));
            assert_eq!(elements[1], Pattern::Binding("b".to_owned()));
            assert_eq!(rest, &Some("rest".to_owned()));
        }
    }
}
