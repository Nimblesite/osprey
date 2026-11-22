//! The declarations the pass reads: every effect's operations and every
//! function, with the call resolution the effect proof uses.

use crate::effect_rows::Callables;
use osprey_ast::{
    effect_name, AstNode, Expr, Multiplicity, OperationMode, Parameter, Position, Program, Stmt,
};
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

/// One declared function, or one `let`-bound lambda: both are called by name,
/// so both carry a summary and both may take capturing callbacks.
pub(super) struct Func<'a> {
    pub(super) scope: Vec<String>,
    pub(super) params: Vec<String>,
    pub(super) body: &'a Expr,
    /// A `let`-bound lambda is summarized where it is bound, in its scope.
    pub(super) bound: bool,
}

/// Every effect and function of one program.
pub(super) struct Table<'a> {
    pub(super) effects: BTreeMap<String, Effect>,
    pub(super) funcs: Vec<Func<'a>>,
    ids: HashMap<String, usize>,
    /// `let`-bound lambdas, by the lambda's own position.
    lambdas: HashMap<(u32, u32), usize>,
    /// Lambdas bound at the top of a file, namespace or module, by qualified name.
    globals: HashMap<String, usize>,
    callables: Callables<'a>,
}

impl<'a> Table<'a> {
    pub(super) fn collect(program: &'a Program) -> Self {
        let mut table = Self {
            effects: BTreeMap::new(),
            funcs: Vec::new(),
            ids: HashMap::new(),
            lambdas: HashMap::new(),
            globals: HashMap::new(),
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

    /// The function id of a `let`-bound lambda.
    pub(super) fn lambda(&self, value: &Expr) -> Option<usize> {
        let Expr::Lambda {
            position: Some(at), ..
        } = value
        else {
            return None;
        };
        self.lambdas.get(&(at.line, at.column)).copied()
    }

    /// The top-level `let`-bound lambda `name` reaches from `scope`.
    pub(super) fn global(&self, scope: &[String], name: &str) -> Option<usize> {
        (0..=scope.len()).rev().find_map(|depth| {
            let prefix = scope.get(..depth)?;
            self.globals.get(&qualify(prefix, name)).copied()
        })
    }

    /// Register every `let`-bound lambda under `statement`.
    fn bind_lambdas(&mut self, statement: &'a Stmt, scope: &[String]) {
        let mut found = Vec::new();
        bound_lambdas(AstNode::Statement(statement), &mut found);
        for lambda in found {
            let Expr::Lambda {
                parameters,
                body,
                position: Some(at),
                ..
            } = lambda
            else {
                continue;
            };
            let id = self.push_func(scope, parameters, body, true);
            let _ = self.lambdas.insert((at.line, at.column), id);
        }
        if let Stmt::Let { name, value, .. } = statement {
            if let Some(id) = self.lambda(value) {
                let _ = self.globals.insert(qualify(scope, name), id);
            }
        }
    }

    pub(super) fn effect(&self, mention: &str) -> Option<&Effect> {
        self.effects.get(effect_name::base(mention))
    }

    /// The action parameter and handled effect of function `id` when it is a
    /// handler value: the lone parameter its body handles the invocation of
    /// ([EFFECTS-HANDLER-VALUE]).
    pub(super) fn handler_action(&self, id: usize) -> Option<(usize, String)> {
        let func = self.funcs.get(id)?;
        let [action, extras @ ..] = func.params.as_slice() else {
            return None;
        };
        if !extras.is_empty() {
            return None;
        }
        invokes_action(action, func.body).map(|effect| (0, effect.to_owned()))
    }

    /// Append one function summary — declared or `let`-bound — and return its
    /// id, which the caller registers under whatever name it is called by.
    fn push_func(
        &mut self,
        scope: &[String],
        parameters: &[Parameter],
        body: &'a Expr,
        bound: bool,
    ) -> usize {
        let id = self.funcs.len();
        self.funcs.push(Func {
            scope: scope.to_vec(),
            params: parameters.iter().map(|p| p.name.clone()).collect(),
            body,
            bound,
        });
        id
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
                let id = self.push_func(scope, parameters, body, false);
                let _ = self.ids.insert(qualify(scope, name), id);
                self.bind_lambdas(statement, scope);
            }
            Stmt::Let { .. } | Stmt::Expr { .. } => self.bind_lambdas(statement, scope),
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

/// Whether `body` is a handler region over the invocation of `name`, and
/// the effect it answers: the shape a reusable handler value's body takes
/// ([EFFECTS-HANDLER-VALUE]).
fn invokes_action<'b>(name: &str, body: &'b Expr) -> Option<&'b str> {
    let Expr::Handler {
        effect,
        body: handled,
        ..
    } = body
    else {
        return None;
    };
    matches!(handled.as_ref(), Expr::Call { function, arguments, named_arguments }
        if matches!(function.as_ref(), Expr::Identifier(called) if called == name)
            && arguments.is_empty()
            && named_arguments.is_empty())
    .then(|| effect.as_str())
}

/// The handled effect of a handler value written inline rather than bound to
/// a name the table knows ([EFFECTS-HANDLER-VALUE]).
pub(super) fn handler_value(expression: &Expr) -> Option<String> {
    let Expr::Lambda {
        parameters, body, ..
    } = expression
    else {
        return None;
    };
    let [action, extras @ ..] = parameters.as_slice() else {
        return None;
    };
    if !extras.is_empty() {
        return None;
    }
    invokes_action(&action.name, body).map(str::to_owned)
}

/// Every lambda a `let` binds under `node`, at any depth.
fn bound_lambdas<'a>(node: AstNode<'a>, out: &mut Vec<&'a Expr>) {
    if let AstNode::Statement(Stmt::Let { value, .. }) = node {
        if matches!(value, Expr::Lambda { .. }) {
            out.push(value);
        }
    }
    node.for_each_child(|child| bound_lambdas(child, out));
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
