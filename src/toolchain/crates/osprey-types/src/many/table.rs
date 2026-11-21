//! The declarations the pass reads: every effect's operations and every
//! function, with the call resolution the effect proof uses.

use crate::effect_rows::Callables;
use osprey_ast::{effect_name, Expr, Multiplicity, OperationMode, Position, Program, Stmt};
use std::collections::{BTreeMap, HashMap};

/// One declared operation, with its parameter and result types as source text.
pub(super) struct Op {
    pub(super) name: String,
    pub(super) params: Vec<String>,
    pub(super) result: String,
    pub(super) control: bool,
}

/// One declared effect.
pub(super) struct Effect {
    pub(super) ops: Vec<Op>,
    pub(super) generic: bool,
    pub(super) many: bool,
    pub(super) control: bool,
    pub(super) position: Option<Position>,
}

/// One declared function.
pub(super) struct Func<'a> {
    pub(super) scope: Vec<String>,
    pub(super) params: Vec<String>,
    pub(super) body: &'a Expr,
}

/// Every effect and function of one program.
pub(super) struct Table<'a> {
    pub(super) effects: BTreeMap<String, Effect>,
    pub(super) funcs: Vec<Func<'a>>,
    ids: HashMap<String, usize>,
    callables: Callables<'a>,
}

impl<'a> Table<'a> {
    pub(super) fn collect(program: &'a Program) -> Self {
        let mut table = Self {
            effects: BTreeMap::new(),
            funcs: Vec::new(),
            ids: HashMap::new(),
            callables: Callables::collect(program),
        };
        table.statements(&program.statements, &[]);
        table
    }

    /// The function `name` reaches from `scope`, as the effect proof resolves it.
    pub(super) fn resolve(&self, scope: &[String], name: &str) -> Option<usize> {
        let qualified = self.callables.resolve(scope, name)?;
        self.ids.get(qualified).copied()
    }

    pub(super) fn func(&self, id: usize) -> Option<&Func<'a>> {
        self.funcs.get(id)
    }

    pub(super) fn effect(&self, mention: &str) -> Option<&Effect> {
        self.effects.get(effect_name::base(mention))
    }

    fn statements(&mut self, statements: &'a [Stmt], scope: &[String]) {
        for statement in statements {
            self.statement(statement, scope);
        }
    }

    fn statement(&mut self, statement: &'a Stmt, scope: &[String]) {
        match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => {
                let _ = self.ids.insert(qualify(scope, name), self.funcs.len());
                self.funcs.push(Func {
                    scope: scope.to_vec(),
                    params: parameters.iter().map(|p| p.name.clone()).collect(),
                    body,
                });
            }
            Stmt::Effect { .. } => {
                if let Some((name, effect)) = declared_effect(statement) {
                    let _ = self.effects.insert(name, effect);
                }
            }
            Stmt::Namespace { name, body, .. } => {
                self.statements(body, &nested(scope, [name.label().to_owned()]));
            }
            Stmt::Module { path, body, .. } => {
                let inner = nested(scope, path.segments.iter().cloned());
                for item in body {
                    self.statement(&item.declaration, &inner);
                }
            }
            _ => {}
        }
    }
}

/// `scope` extended by `more`, the way a namespace or module nests names.
pub(super) fn nested(scope: &[String], more: impl IntoIterator<Item = String>) -> Vec<String> {
    scope.iter().cloned().chain(more).collect()
}

fn qualify(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}

fn declared_effect(statement: &Stmt) -> Option<(String, Effect)> {
    let Stmt::Effect {
        name,
        type_params,
        operations,
        position,
        ..
    } = statement
    else {
        return None;
    };
    let ops: Vec<Op> = operations
        .iter()
        .map(|op| {
            let (params, result) = signature(&op.ty);
            Op {
                name: op.name.clone(),
                params,
                result,
                control: op.mode == OperationMode::Control,
            }
        })
        .collect();
    let effect = Effect {
        generic: !type_params.is_empty(),
        many: operations
            .iter()
            .any(|op| op.multiplicity() == Multiplicity::Many),
        control: ops.iter().any(|op| op.control),
        ops,
        position: *position,
    };
    Some((name.clone(), effect))
}

/// An operation's written `fn(A, B) -> R` as its parameter and result types.
fn signature(written: &str) -> (Vec<String>, String) {
    match crate::convert::type_name_to_type(written, &HashMap::new()) {
        crate::Type::Fun { params, ret } => (
            params.iter().map(ToString::to_string).collect(),
            ret.to_string(),
        ),
        other => (Vec::new(), other.to_string()),
    }
}

/// Whether any effect in `statements`, at any nesting depth, declares `many`.
pub(super) fn declares_many(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Effect { .. } => declared_effect(statement).is_some_and(|(_, effect)| effect.many),
        Stmt::Namespace { body, .. } => declares_many(body),
        Stmt::Module { body, .. } => body
            .iter()
            .any(|item| declares_many(std::slice::from_ref(item.declaration.as_ref()))),
        _ => false,
    })
}
