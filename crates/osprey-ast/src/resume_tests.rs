//! Resume-site analysis: whether an arm resumes at all, and how many times the
//! worst single control path through it does.
//!
//! A sibling file rather than an inline `mod tests`, so `resume.rs` stays under
//! the 500-line ceiling while the two questions it answers keep their coverage.
//! Implements [EFFECTS-RESUME], [MULTI-HANDLE-ONCE].

use crate::resume::{contains_resume, resumes_on_one_path};
use crate::{Expr, HandlerArm, InterpolatedPart, MapEntry, NamedArgument, Stmt};

fn r() -> Expr {
    Expr::Resume(None)
}
fn b(e: Expr) -> Box<Expr> {
    Box::new(e)
}
fn arm(body: Expr) -> crate::MatchArm {
    crate::MatchArm {
        pattern: crate::Pattern::Wildcard,
        body,
    }
}
fn stmt(value: Expr) -> Stmt {
    Stmt::Expr {
        value,
        doc: None,
        position: None,
    }
}
fn field(value: Expr) -> crate::FieldAssignment {
    crate::FieldAssignment {
        name: "f".into(),
        value,
    }
}
fn assert_all_contain(cases: &[Expr]) {
    for e in cases {
        assert!(contains_resume(e), "resume not found in {e:?}");
    }
}

#[test]
fn walks_literal_and_data_container_forms() {
    assert_all_contain(&[
        Expr::InterpolatedStr(vec![crate::InterpolatedPart::Expr(r())]),
        Expr::List(vec![r()], None),
        Expr::Map(vec![crate::MapEntry {
            key: r(),
            value: Expr::Integer(0),
        }]),
        Expr::Object(vec![field(r())]),
        Expr::TypeConstructor {
            name: "C".into(),
            type_args: Vec::new(),
            fields: vec![field(r())],
        },
        Expr::Update {
            record: "rec".into(),
            fields: vec![field(r())],
        },
        Expr::Binary {
            op: "+".into(),
            left: b(Expr::Integer(1)),
            right: b(r()),
        },
        Expr::Pipe {
            left: b(r()),
            right: b(Expr::Identifier("f".into())),
        },
        Expr::Unary {
            op: "-".into(),
            operand: b(r()),
        },
    ]);
}

#[test]
fn walks_call_control_and_concurrency_forms() {
    assert_all_contain(&[
        Expr::Call {
            function: b(Expr::Identifier("f".into())),
            arguments: Vec::new(),
            named_arguments: vec![crate::NamedArgument {
                name: "a".into(),
                value: r(),
            }],
        },
        Expr::MethodCall {
            target: b(r()),
            method: "m".into(),
            arguments: Vec::new(),
            named_arguments: Vec::new(),
        },
        Expr::FieldAccess {
            target: b(r()),
            field: "x".into(),
        },
        Expr::Index {
            target: b(Expr::Identifier("xs".into())),
            index: b(r()),
        },
        Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: b(r()),
            position: None,
        },
        Expr::Spawn(b(r())),
        Expr::Await(b(r())),
        Expr::Recv(b(r())),
        Expr::Yield(Some(b(r()))),
        Expr::Send {
            channel: b(Expr::Integer(0)),
            value: b(r()),
        },
        Expr::Match {
            value: b(r()),
            arms: Vec::new(),
        },
        Expr::Select {
            arms: vec![crate::MatchArm {
                pattern: crate::Pattern::Wildcard,
                body: r(),
            }],
        },
        Expr::Perform {
            effect: "E".into(),
            operation: "o".into(),
            arguments: vec![r()],
            named_arguments: Vec::new(),
            position: None,
        },
    ]);
}

#[test]
fn negatives_and_statement_walks() {
    // Negative cases: leaves without resume, and non-binding statements.
    assert!(!contains_resume(&Expr::Integer(1)));
    assert!(!contains_resume(&Expr::Yield(None)));
    let import_only = Expr::Block {
        statements: vec![crate::Stmt::Import(crate::ImportDecl {
            target: crate::ImportTarget {
                namespace: crate::NamespaceName::Identifier("m".into()),
                path: crate::SymbolPath::default(),
            },
            alias: None,
            selection: crate::ImportSelection::Whole,
            position: None,
        })],
        value: None,
    };
    assert!(!contains_resume(&import_only));
    // Assignment statements inside blocks are walked.
    let assign = Expr::Block {
        statements: vec![crate::Stmt::Assignment {
            name: "x".into(),
            value: r(),
            position: None,
        }],
        value: None,
    };
    assert!(contains_resume(&assign));
}

#[test]
fn finds_resume_through_blocks_but_not_nested_handlers() {
    let resume = Expr::Resume(None);
    assert!(contains_resume(&resume));
    let block = Expr::Block {
        statements: vec![Stmt::Expr {
            value: Expr::Resume(None),
            doc: None,
            position: None,
        }],
        value: None,
    };
    assert!(contains_resume(&block));
    // A nested handler's resume belongs to the nested handler.
    let nested = Expr::Handler {
        stage: crate::Stage::Dynamic,
        effect: "E".into(),
        arms: vec![crate::HandlerArm {
            operation: "op".into(),
            params: Vec::new(),
            body: Expr::Resume(None),
            position: None,
        }],
        body: Box::new(Expr::Integer(1)),
        position: None,
    };
    assert!(!contains_resume(&nested));
}

#[test]
fn branches_take_the_worst_arm_while_sequences_add_up() {
    // Two `resume`s on DIFFERENT branches are one apiece on their own path —
    // the shape `abort_vs_resume.test.osp` relies on staying legal.
    let branched = Expr::Match {
        value: b(Expr::Integer(0)),
        arms: vec![arm(r()), arm(r())],
    };
    assert_eq!(resumes_on_one_path(&branched), 1);
    // The scrutinee is crossed before either branch.
    let scrutinised = Expr::Match {
        value: b(r()),
        arms: vec![arm(r()), arm(Expr::Integer(0))],
    };
    assert_eq!(resumes_on_one_path(&scrutinised), 2);
    // Two in sequence are two on one path.
    let sequential = Expr::Block {
        statements: vec![stmt(r())],
        value: Some(b(r())),
    };
    assert_eq!(resumes_on_one_path(&sequential), 2);
    // A `resume` whose own argument resumes is also two.
    assert_eq!(resumes_on_one_path(&Expr::Resume(Some(b(r())))), 2);
    assert_eq!(resumes_on_one_path(&Expr::Integer(1)), 0);
}

#[test]
fn select_branches_and_lambdas_and_nested_handlers() {
    let selected = Expr::Select {
        arms: vec![arm(r()), arm(r())],
    };
    assert_eq!(resumes_on_one_path(&selected), 1);
    // A lambda body is not on the arm's own control path.
    let lambda = Expr::Lambda {
        parameters: Vec::new(),
        return_type: None,
        body: b(r()),
        position: None,
    };
    assert_eq!(resumes_on_one_path(&lambda), 0);
    // A nested handler's ARMS own their resumes; only its body is crossed.
    let nested = Expr::Handler {
        stage: crate::Stage::Dynamic,
        effect: "E".into(),
        arms: vec![HandlerArm {
            operation: "op".into(),
            params: Vec::new(),
            body: r(),
            position: None,
        }],
        body: b(r()),
        position: None,
    };
    assert_eq!(resumes_on_one_path(&nested), 1);
}

#[test]
fn every_sequential_position_is_crossed() {
    let cases = [
        Expr::InterpolatedStr(vec![
            InterpolatedPart::Text("t".into()),
            InterpolatedPart::Expr(r()),
        ]),
        Expr::List(vec![r()], None),
        Expr::Map(vec![MapEntry {
            key: r(),
            value: Expr::Integer(0),
        }]),
        Expr::Object(vec![field(r())]),
        Expr::TypeConstructor {
            name: "C".into(),
            type_args: Vec::new(),
            fields: vec![field(r())],
        },
        Expr::Update {
            record: "r".into(),
            fields: vec![field(r())],
        },
        Expr::Binary {
            op: "+".into(),
            left: b(Expr::Integer(1)),
            right: b(r()),
        },
        Expr::Pipe {
            left: b(r()),
            right: b(Expr::Identifier("f".into())),
        },
        Expr::Unary {
            op: "-".into(),
            operand: b(r()),
        },
        Expr::Call {
            function: b(Expr::Identifier("f".into())),
            arguments: vec![r()],
            named_arguments: Vec::new(),
        },
        Expr::MethodCall {
            target: b(r()),
            method: "m".into(),
            arguments: Vec::new(),
            named_arguments: Vec::new(),
        },
        Expr::FieldAccess {
            target: b(r()),
            field: "x".into(),
        },
        Expr::Index {
            target: b(Expr::Identifier("xs".into())),
            index: b(r()),
        },
        Expr::Spawn(b(r())),
        Expr::Await(b(r())),
        Expr::Recv(b(r())),
        Expr::Yield(Some(b(r()))),
        Expr::Send {
            channel: b(Expr::Integer(0)),
            value: b(r()),
        },
        Expr::Perform {
            effect: "E".into(),
            operation: "op".into(),
            arguments: vec![r()],
            named_arguments: Vec::new(),
            position: None,
        },
    ];
    for case in &cases {
        assert_eq!(resumes_on_one_path(case), 1, "not crossed: {case:?}");
    }
    // Named arguments are crossed too, and a value-less `yield` crosses nothing.
    let named = Expr::Call {
        function: b(Expr::Identifier("f".into())),
        arguments: Vec::new(),
        named_arguments: vec![NamedArgument {
            name: "a".into(),
            value: r(),
        }],
    };
    assert_eq!(resumes_on_one_path(&named), 1);
    assert_eq!(resumes_on_one_path(&Expr::Yield(None)), 0);
    assert_eq!(resumes_on_one_path(&Expr::Identifier("x".into())), 0);
}
