//! The syntax the pass writes: calls to the generated helpers, closures and
//! bindings, each with a position no source node has.

use crate::prelude::{PRELUDE_LINE_BASE, PRELUDE_UNIT_STRIDE};
use osprey_ast::{Expr, FieldAssignment, Parameter, Position, Stmt};

/// The line the generated helper declarations start at, past every prelude unit.
pub(super) const HELPER_LINE_BASE: u32 = PRELUDE_LINE_BASE + 8 * PRELUDE_UNIT_STRIDE;
/// The line written nodes start at, past the helper declarations.
const NODE_LINE_BASE: u32 = PRELUDE_LINE_BASE + 9 * PRELUDE_UNIT_STRIDE;

/// The step type's finished constructor.
pub(super) const DONE: &str = "Many__Done";
pub(super) const BIND: &str = "__manyBind";
pub(super) const FINALLY: &str = "__manyFinally";
pub(super) const REINSTALL: &str = "__manyReinstall";
pub(super) const RUN: &str = "__manyRun";
pub(super) const UNIT: &str = "__manyUnit";

/// The helper that makes `effect.operation`'s request.
pub(super) fn request_name(effect: &str, operation: &str) -> String {
    format!("__manyPerform_{effect}_{operation}")
}

/// The helper that answers `effect`'s requests.
pub(super) fn handle_name(effect: &str) -> String {
    format!("__manyHandle_{effect}")
}

/// Fresh names and positions for written nodes.
#[derive(Debug, Default)]
pub(super) struct Fresh {
    next: u32,
}

impl Fresh {
    fn bump(&mut self) -> u32 {
        self.next = self.next.saturating_add(1);
        self.next
    }

    pub(super) fn position(&mut self) -> Position {
        Position {
            line: NODE_LINE_BASE.saturating_add(self.bump()),
            column: 0,
        }
    }

    pub(super) fn name(&mut self, stem: &str) -> String {
        format!("__many_{stem}{}", self.bump())
    }

    pub(super) fn lambda(&mut self, params: Vec<String>, body: Expr) -> Expr {
        Expr::Lambda {
            parameters: params.into_iter().map(parameter).collect(),
            return_type: None,
            body: Box::new(body),
            position: Some(self.position()),
        }
    }

    /// `__manyBind(step, fn(binder) => rest)`.
    pub(super) fn bind(&mut self, step: Expr, binder: String, rest: Expr) -> Expr {
        let next = self.lambda(vec![binder], rest);
        call(BIND, vec![step, next])
    }

    /// `{ let name = value; rest }`.
    pub(super) fn let_in(&mut self, name: String, value: Expr, rest: Expr) -> Expr {
        let binding = Stmt::Let {
            name,
            mutable: false,
            ty: None,
            value,
            doc: None,
            position: Some(self.position()),
        };
        block(vec![binding], rest)
    }
}

fn parameter(name: String) -> Parameter {
    Parameter {
        name,
        ty: None,
        inline_constraint: false,
    }
}

pub(super) fn ident(name: &str) -> Expr {
    Expr::Identifier(name.to_owned())
}

pub(super) fn apply(function: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: Vec::new(),
    }
}

pub(super) fn call(name: &str, arguments: Vec<Expr>) -> Expr {
    apply(ident(name), arguments)
}

/// A finished step carrying `value`.
pub(super) fn done(value: Expr) -> Expr {
    Expr::TypeConstructor {
        name: DONE.to_owned(),
        type_args: Vec::new(),
        fields: vec![FieldAssignment {
            name: "value".to_owned(),
            value,
        }],
    }
}

/// `statements` followed by `tail`, flattened when `tail` is itself a block.
pub(super) fn block(mut statements: Vec<Stmt>, tail: Expr) -> Expr {
    match tail {
        Expr::Block {
            statements: more,
            value,
            position: None,
        } => {
            statements.extend(more);
            Expr::Block {
                statements,
                value,
                position: None,
            }
        }
        other => Expr::Block {
            statements,
            value: Some(Box::new(other)),
            position: None,
        },
    }
}
