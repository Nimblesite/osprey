//! The rewrite: code that leaves no captured request unanswered keeps its
//! direct form, and code that does is written as steps.
//!
//! `direct` rewrites an expression that yields a plain value; `step` rewrites
//! one that yields a step. A function whose summary is non-empty returns a
//! step, so its callers either sequence it (inside a step) or, where every
//! request it could make is answered, run it to its value.

use super::build::{self, done, ident, Fresh, RUN};
use super::escape::Analyzer;
use super::facts::{Callee, Facts, Scope};
use super::table::{nested, Table};
use crate::TypeError;
use osprey_ast::{
    freevars::pattern_bindings, mutate, Expr, MatchArm, NamedArgument, Parameter, Position,
    Program, Stmt,
};
use std::collections::BTreeSet;

/// One rewrite of one program.
pub(super) struct Rewriter<'a> {
    pub(super) analyzer: Analyzer<'a>,
    pub(super) fresh: Fresh,
    /// Every `mut` name: a read of one is snapshotted before a capture.
    pub(super) mutables: BTreeSet<String>,
    errors: Vec<TypeError>,
    /// The nearest source position, for a rejection.
    pub(super) at: Option<Position>,
}

/// Rewrite `program` under the solved `facts`.
pub(super) fn program(
    table: &Table<'_>,
    facts: Facts,
    program: &Program,
) -> Result<Program, Vec<TypeError>> {
    let mut rewriter = Rewriter {
        analyzer: Analyzer::new(table, facts),
        fresh: Fresh::default(),
        mutables: crate::multiplicity::mutable_bindings(program),
        errors: Vec::new(),
        at: None,
    };
    let statements = rewriter.statements(&program.statements, &[]);
    if rewriter.errors.is_empty() {
        Ok(Program {
            statements,
            doc: program.doc.clone(),
        })
    } else {
        Err(rewriter.errors)
    }
}

/// Why a request cannot be captured where it stands.
fn crossing(what: &str) -> String {
    format!(
        "{what} needs its continuation captured, but it runs inside a call that cannot be split: \
a builtin, a stored function value or a callback this pass cannot follow. A `many` continuation \
cannot cross that call; call the function directly or pass it to a function parameter"
    )
}

impl Rewriter<'_> {
    pub(super) fn reject(&mut self, message: String) {
        self.errors.push(TypeError::new(message).with_pos(self.at));
    }

    fn reject_crossing(&mut self, what: &str) {
        self.reject(crossing(what));
    }

    fn statements(&mut self, statements: &[Stmt], namespace: &[String]) -> Vec<Stmt> {
        statements
            .iter()
            .map(|statement| self.declaration(statement, namespace))
            .collect()
    }

    fn declaration(&mut self, statement: &Stmt, namespace: &[String]) -> Stmt {
        let mut out = statement.clone();
        match (&mut out, statement) {
            (Stmt::Namespace { body: slot, .. }, Stmt::Namespace { name, body, .. }) => {
                *slot = self.statements(body, &nested(namespace, [name.label().to_owned()]));
            }
            (Stmt::Module { body: slots, .. }, Stmt::Module { path, body, .. }) => {
                let inner = nested(namespace, path.segments.iter().cloned());
                for (slot, item) in slots.iter_mut().zip(body) {
                    *slot.declaration = self.declaration(&item.declaration, &inner);
                }
            }
            (Stmt::Function { .. }, Stmt::Function { .. }) => {
                self.function(&mut out, statement, namespace);
            }
            (Stmt::Let { value: slot, .. }, Stmt::Let { value, .. }) => {
                *slot = self.bound_value(value, &Scope::top(namespace));
            }
            (
                Stmt::Assignment { value: slot, .. } | Stmt::Expr { value: slot, .. },
                Stmt::Assignment { value, .. } | Stmt::Expr { value, .. },
            ) => *slot = self.direct(value, &Scope::top(namespace)),
            _ => {}
        }
        out
    }

    fn function(&mut self, out: &mut Stmt, source: &Stmt, namespace: &[String]) {
        let (
            Stmt::Function {
                parameters,
                return_type,
                body: slot,
                ..
            },
            Stmt::Function {
                name,
                body,
                position,
                ..
            },
        ) = (out, source)
        else {
            return;
        };
        self.at = *position;
        let table = self.analyzer.table;
        let Some(id) = table.resolve(namespace, name) else {
            *slot = self.direct(body, &Scope::top(namespace));
            return;
        };
        let scope = Scope::of_function(table, id);
        if self.analyzer.facts.returns_step(id) {
            *slot = self.step(body, &scope);
            *return_type = None;
        } else {
            *slot = self.direct(body, &scope);
        }
        self.untype_step_parameters(id, parameters);
    }

    /// Rewrite `expression` where it yields a plain value.
    pub(super) fn direct(&mut self, expression: &Expr, scope: &Scope) -> Expr {
        match expression {
            Expr::Call { .. } => self.direct_call(expression, scope),
            Expr::Handler { .. } => self.direct_handler(expression, scope),
            Expr::Block {
                statements,
                value,
                position,
            } => self.direct_block(statements, value.as_deref(), *position, scope),
            Expr::Match { value, arms } => Expr::Match {
                value: Box::new(self.direct(value, scope)),
                arms: self.arms(arms, scope, false),
            },
            Expr::Lambda { .. } => self.lambda(expression, scope, false),
            Expr::Identifier(_) | Expr::Path(_) => {
                self.plain_use(expression, scope);
                expression.clone()
            }
            Expr::Perform {
                effect,
                operation,
                position,
                ..
            } if self.analyzer.facts.is_captured(effect) => {
                self.at = position.or(self.at);
                self.reject_crossing(&format!("`{effect}.{operation}`"));
                expression.clone()
            }
            Expr::Mask { effect, .. } | Expr::Instance { effect, .. }
                if self.analyzer.facts.is_captured(effect) =>
            {
                self.reject(format!(
                    "`{effect}` is captured by a `many` region, so it cannot be masked or \
instantiated: its requests carry their continuation as a value"
                ));
                expression.clone()
            }
            _ => self.direct_children(expression, scope),
        }
    }

    fn direct_children(&mut self, expression: &Expr, scope: &Scope) -> Expr {
        let mut out = expression.clone();
        mutate::children_mut(&mut out, &mut |child| {
            let rewritten = self.direct(child, scope);
            *child = rewritten;
        });
        out
    }

    /// Match arms, each rewritten as a value or as a step.
    pub(super) fn arms(&mut self, arms: &[MatchArm], scope: &Scope, steps: bool) -> Vec<MatchArm> {
        arms.iter()
            .map(|arm| {
                let inner = scope.binding(pattern_bindings(&arm.pattern));
                MatchArm {
                    pattern: arm.pattern.clone(),
                    body: self.either(&arm.body, &inner, steps),
                }
            })
            .collect()
    }

    fn either(&mut self, expression: &Expr, scope: &Scope, steps: bool) -> Expr {
        if steps {
            self.step(expression, scope)
        } else {
            self.direct(expression, scope)
        }
    }

    /// A lambda whose body yields a value, or a step when `steps`.
    pub(super) fn lambda(&mut self, lambda: &Expr, scope: &Scope, steps: bool) -> Expr {
        let mut out = lambda.clone();
        if let (
            Expr::Lambda {
                body: slot,
                return_type,
                ..
            },
            Expr::Lambda {
                parameters,
                body,
                position,
                ..
            },
        ) = (&mut out, lambda)
        {
            self.at = position.or(self.at);
            let inner = scope.binding(parameters.iter().map(|p| p.name.clone()));
            **slot = self.either(body, &inner, steps);
            if steps {
                *return_type = None;
            }
        }
        out
    }

    fn direct_block(
        &mut self,
        statements: &[Stmt],
        value: Option<&Expr>,
        position: Option<Position>,
        scope: &Scope,
    ) -> Expr {
        let mut scope = scope.clone();
        let mut out = Vec::with_capacity(statements.len());
        for statement in statements {
            let mut rewritten = statement.clone();
            match (&mut rewritten, statement) {
                (Stmt::Let { value: slot, .. }, Stmt::Let { name, value, .. }) => {
                    *slot = self.bound_value(value, &scope);
                    scope = self.analyzer.bound(&scope, name, value);
                }
                (
                    Stmt::Assignment { value: slot, .. } | Stmt::Expr { value: slot, .. },
                    Stmt::Assignment { value, .. } | Stmt::Expr { value, .. },
                ) => *slot = self.direct(value, &scope),
                _ => {}
            }
            out.push(rewritten);
        }
        Expr::Block {
            statements: out,
            value: value.map(|v| Box::new(self.direct(v, &scope))),
            position,
        }
    }

    /// The value of a `let`: a lambda whose calls capture becomes a closure
    /// that returns a step.
    pub(super) fn bound_value(&mut self, value: &Expr, scope: &Scope) -> Expr {
        match self.analyzer.table.lambda(value) {
            Some(id) => self.bound_lambda(id, value, scope),
            None => self.direct(value, scope),
        }
    }

    /// A `let`-bound lambda, rewritten as the function it is: a step-returning
    /// body when its calls capture, untyped parameters where it takes a
    /// capturing callback.
    fn bound_lambda(&mut self, id: usize, lambda: &Expr, scope: &Scope) -> Expr {
        let mut out = lambda.clone();
        let (
            Expr::Lambda {
                parameters: slots,
                return_type,
                body: slot,
                ..
            },
            Expr::Lambda {
                parameters, body, ..
            },
        ) = (&mut out, lambda)
        else {
            return out;
        };
        let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
        let steps = self.analyzer.facts.returns_step(id);
        **slot = self.either(body, &scope.entering(id, &names), steps);
        if steps {
            *return_type = None;
        }
        self.untype_step_parameters(id, slots);
        out
    }

    /// Drop the annotation of every parameter that takes a capturing callback,
    /// so inference re-derives the step type the rewrite supplies.
    fn untype_step_parameters(&mut self, id: usize, parameters: &mut [Parameter]) {
        for (index, parameter) in parameters.iter_mut().enumerate() {
            if self.analyzer.facts.takes_step(id, index) {
                parameter.ty = None;
            }
        }
    }

    /// A step-returning callable used as a plain value escapes this pass.
    fn plain_use(&mut self, name: &Expr, scope: &Scope) {
        let steps = match self.analyzer.callee(name, scope) {
            Callee::Function(id) => self.analyzer.facts.returns_step(id),
            Callee::Alias(alias) => !alias.latent.is_empty(),
            Callee::Param(index) => self.param_steps(index, scope),
            Callee::Other => false,
        };
        if let (true, Expr::Identifier(name)) = (steps, name) {
            self.reject_crossing(&format!("`{name}`"));
        }
    }

    pub(super) fn param_steps(&self, index: usize, scope: &Scope) -> bool {
        scope
            .func
            .is_some_and(|func| self.analyzer.facts.takes_step(func, index))
    }

    fn direct_call(&mut self, call: &Expr, scope: &Scope) -> Expr {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = call
        else {
            return call.clone();
        };
        match self.analyzer.callee(function, scope) {
            Callee::Function(id) => {
                let placed = self.analyzer.placed(id, arguments, named_arguments);
                let values = placed
                    .iter()
                    .map(|(index, argument)| match index {
                        Some(index) if self.analyzer.facts.takes_step(id, *index) => {
                            self.stepping(argument, scope)
                        }
                        _ => self.direct(argument, scope),
                    })
                    .collect();
                let rebuilt = rebuild_call(function, named_arguments, values);
                if self.analyzer.facts.returns_step(id) {
                    self.must_be_closed(call, scope, "this call");
                    build::call(RUN, vec![rebuilt])
                } else {
                    rebuilt
                }
            }
            Callee::Alias(alias) if !alias.latent.is_empty() => {
                self.reject_crossing("this call");
                call.clone()
            }
            Callee::Param(index) if self.param_steps(index, scope) => {
                self.reject_crossing("this call");
                call.clone()
            }
            _ => self.direct_children(call, scope),
        }
    }

    /// A direct context can run a step only when nothing in it is unanswered.
    pub(super) fn must_be_closed(&mut self, expression: &Expr, scope: &Scope, what: &str) {
        if self.analyzer.open(expression, scope) {
            self.reject_crossing(what);
        }
    }

    /// `argument` as the step-returning callable a stepping parameter expects.
    pub(super) fn stepping(&mut self, argument: &Expr, scope: &Scope) -> Expr {
        if matches!(argument, Expr::Lambda { .. }) {
            return self.lambda(argument, scope, true);
        }
        match self.analyzer.callee(argument, scope) {
            Callee::Alias(alias) if alias.latent.is_empty() => self.eta(argument, alias.arity),
            Callee::Alias(_) => argument.clone(),
            Callee::Param(index) if self.param_steps(index, scope) => argument.clone(),
            Callee::Function(id) if self.analyzer.facts.returns_step(id) => {
                if !self.analyzer.facts.summary(id).params.is_empty() {
                    self.reject_crossing("a function that itself takes a capturing callback");
                }
                argument.clone()
            }
            Callee::Function(id) => {
                let arity = self.analyzer.table.func(id).map_or(0, |f| f.params.len());
                self.eta(argument, arity)
            }
            Callee::Param(_) | Callee::Other => {
                self.reject_crossing("a callback passed as this expression");
                argument.clone()
            }
        }
    }

    /// `fn(a..) => Many__Done { value: callable(a..) }`.
    fn eta(&mut self, callable: &Expr, arity: usize) -> Expr {
        let names: Vec<String> = (0..arity).map(|_| self.fresh.name("a")).collect();
        let arguments = names.iter().map(|name| ident(name)).collect();
        let body = done(build::apply(callable.clone(), arguments));
        self.fresh.lambda(names, body)
    }
}

/// `function(values..)`, keeping the names of the original named arguments.
pub(super) fn rebuild_call(function: &Expr, named: &[NamedArgument], values: Vec<Expr>) -> Expr {
    let positional = values.len().saturating_sub(named.len());
    let mut values = values.into_iter();
    let arguments: Vec<Expr> = values.by_ref().take(positional).collect();
    let named_arguments = named
        .iter()
        .zip(values)
        .map(|(argument, value)| NamedArgument {
            name: argument.name.clone(),
            value,
        })
        .collect();
    Expr::Call {
        function: Box::new(function.clone()),
        arguments,
        named_arguments,
    }
}
