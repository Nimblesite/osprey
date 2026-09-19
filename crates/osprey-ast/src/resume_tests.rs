//! Local continuation-use path counts. Implements [MULTI-HANDLE-ONCE].

use crate::resume::resumes_on_one_path;
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
/// Every expression form that holds a sub-expression in a **container**
/// position, each built around exactly one `resume()`. The containment walk and
/// the one-path count must both cross all of them, so the list is stated once
/// here instead of once per test, where the two copies could drift apart.
fn container_forms() -> Vec<Expr> {
    vec![
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
            record: "rec".into(),
            fields: vec![field(r())],
        },
        Expr::Binary {
            position: None,
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
    ]
}

/// The call, field and concurrency forms — still one sequential path each, so
/// they too are crossed exactly once. Positional and named argument lists are
/// both listed, because they are separate positions in the walk.
fn call_forms() -> Vec<Expr> {
    vec![
        Expr::Call {
            function: b(Expr::Identifier("f".into())),
            arguments: vec![r()],
            named_arguments: Vec::new(),
        },
        Expr::Call {
            function: b(Expr::Identifier("f".into())),
            arguments: Vec::new(),
            named_arguments: vec![NamedArgument {
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
            operation: "o".into(),
            arguments: vec![r()],
            named_arguments: Vec::new(),
            position: None,
        },
    ]
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
        return_clause: None,
        position: None,
    };
    assert_eq!(resumes_on_one_path(&nested), 1);
}

#[test]
fn every_sequential_position_is_crossed() {
    for case in container_forms().iter().chain(&call_forms()) {
        assert_eq!(resumes_on_one_path(case), 1, "not crossed: {case:?}");
    }
    // A value-less `yield` and a bare identifier cross nothing.
    assert_eq!(resumes_on_one_path(&Expr::Yield(None)), 0);
    assert_eq!(resumes_on_one_path(&Expr::Identifier("x".into())), 0);
}
