//! Whether an expression performs an explicit `resume` — the property that
//! splits handler-arm semantics: a resuming arm's value is the handler's
//! ANSWER, a non-resuming arm's value substitutes for the operation's RESULT.
//! Shared by the type checker (arm typing) and codegen (arm emission).
//! Implements [EFFECTS-RESUME].

use crate::{Expr, Stmt};

/// True when `e` contains a `resume` belonging to the ENCLOSING handler arm —
/// a nested handler's body owns its own `resume`s, so they don't count.
#[must_use]
pub fn contains_resume(e: &Expr) -> bool {
    match e {
        Expr::Resume(_) => true,
        Expr::InterpolatedStr(parts) => parts
            .iter()
            .any(|p| matches!(p, crate::InterpolatedPart::Expr(inner) if contains_resume(inner))),
        Expr::List(xs) => xs.iter().any(contains_resume),
        Expr::Map(entries) => entries
            .iter()
            .any(|entry| contains_resume(&entry.key) || contains_resume(&entry.value)),
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => fields.iter().any(|f| contains_resume(&f.value)),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            contains_resume(left) || contains_resume(right)
        }
        Expr::Unary { operand, .. } => contains_resume(operand),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            contains_resume(function)
                || arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            contains_resume(target)
                || arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        Expr::FieldAccess { target, .. } => contains_resume(target),
        Expr::Index { target, index } => contains_resume(target) || contains_resume(index),
        Expr::Lambda { body, .. } | Expr::Spawn(body) | Expr::Await(body) | Expr::Recv(body) => {
            contains_resume(body)
        }
        Expr::Yield(Some(value)) => contains_resume(value),
        Expr::Send { channel, value } => contains_resume(channel) || contains_resume(value),
        Expr::Match { value, arms } => {
            contains_resume(value) || arms.iter().any(|arm| contains_resume(&arm.body))
        }
        Expr::Block { statements, value } => {
            statements.iter().any(stmt_contains_resume)
                || value.as_deref().is_some_and(contains_resume)
        }
        Expr::Select { arms } => arms.iter().any(|arm| contains_resume(&arm.body)),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            arguments.iter().any(contains_resume)
                || named_arguments.iter().any(|n| contains_resume(&n.value))
        }
        // A nested handler owns its own `resume`; do not mark the outer handler
        // as a resuming region because of it.
        Expr::Handler { body, .. } => contains_resume(body),
        _ => false,
    }
}

fn stmt_contains_resume(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            contains_resume(value)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Expr {
        Expr::Resume(None)
    }
    fn b(e: Expr) -> Box<Expr> {
        Box::new(e)
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
            Expr::List(vec![r()]),
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
                position: None,
            }],
            value: None,
        };
        assert!(contains_resume(&block));
        // A nested handler's resume belongs to the nested handler.
        let nested = Expr::Handler {
            effect: "E".into(),
            arms: vec![crate::HandlerArm {
                operation: "op".into(),
                params: Vec::new(),
                body: Expr::Resume(None),
            }],
            body: Box::new(Expr::Integer(1)),
            position: None,
        };
        assert!(!contains_resume(&nested));
    }
}
