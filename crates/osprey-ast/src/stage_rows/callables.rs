//! What a call site proves about the helper it calls [STAGE-STATIC-MONOTONE].
//!
//! A helper that calls one of its parameters has an unknown row of its own,
//! because the callable arrives from its caller. A call that hands it a named
//! function or an inline lambda at every parameter it calls says exactly what
//! it calls — the argument, whose requirements the caller already counts where
//! it names or writes it — as a GPU kernel's call site does ([GPU-KERNEL-PURE]).
//! Such a call inherits everything the helper requires except that unknown.
//! Every other use of the helper, and any name a local binding could shadow,
//! stays unknown.

use super::{function_bodies, invokes_parameter};
use crate::{walk_program, AstNode, AstVisitor, Expr, NamedArgument, Parameter, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// The requirement of a callable whose effects no call site settles.
pub(crate) const UNKNOWN: &str = "<unknown>";

/// The program's helpers that call parameters, and the functions a call site
/// may hand them by name.
#[derive(Default)]
pub(crate) struct Callables {
    /// Functions declared once, shadowed by no binder and closed over nothing
    /// a binder introduces: a reference to one names exactly its body.
    known: BTreeSet<String>,
    /// Such functions whose only unknown is calling these parameters.
    parametric: BTreeMap<String, BTreeSet<usize>>,
    /// Functions no call site settles: they reach an unknown otherwise.
    opaque: BTreeSet<String>,
}

impl Callables {
    pub(crate) fn of(program: &Program) -> Callables {
        let mut scope = Scope::default();
        walk_program(program, &mut scope);
        let bodies = function_bodies(program);
        let mut callables = scope.classify(&bodies);
        callables.settle(&bodies);
        callables
    }

    /// Every function whose own row is unknown.
    pub(crate) fn unknown(&self) -> impl Iterator<Item = &String> {
        self.opaque.iter().chain(self.parametric.keys())
    }

    /// Whether `function(arguments)` hands a named function or an inline
    /// lambda to every parameter its callee calls.
    pub(crate) fn resolves(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named: &[NamedArgument],
    ) -> bool {
        let Expr::Identifier(callee) = function else {
            return false;
        };
        named.is_empty()
            && self.parametric.get(callee).is_some_and(|invoked| {
                invoked.iter().all(|index| {
                    arguments
                        .get(*index)
                        .is_some_and(|argument| self.callable(argument))
                })
            })
    }

    fn callable(&self, argument: &Expr) -> bool {
        match argument {
            Expr::Lambda { .. } => true,
            Expr::Identifier(name) => self.known.contains(name),
            _ => false,
        }
    }

    fn is_unknown(&self, name: &str) -> bool {
        self.opaque.contains(name) || self.parametric.contains_key(name)
    }

    /// Grow the opaque set to a fixed point: a body is opaque when it names an
    /// unknown function anywhere but as the callee of a resolving call.
    fn settle(&mut self, bodies: &BTreeMap<String, Expr>) {
        loop {
            let reached: Vec<String> = bodies
                .iter()
                .filter(|(name, body)| !self.opaque.contains(*name) && self.reaches_unknown(body))
                .map(|(name, _)| name.clone())
                .collect();
            if reached.is_empty() {
                return;
            }
            self.parametric.retain(|name, _| !reached.contains(name));
            self.opaque.extend(reached);
        }
    }

    fn reaches_unknown(&self, body: &Expr) -> bool {
        let mut pending = vec![AstNode::Expression(body)];
        while let Some(node) = pending.pop() {
            match node {
                AstNode::Expression(Expr::Call {
                    function,
                    arguments,
                    named_arguments,
                }) if self.resolves(function, arguments, named_arguments) => {
                    pending.extend(arguments.iter().map(AstNode::Expression));
                }
                AstNode::Expression(Expr::Identifier(name)) if self.is_unknown(name) => {
                    return true
                }
                _ => node.for_each_child(|child| pending.push(child)),
            }
        }
        false
    }
}

/// Every binder in the program, and each callable's declarations.
#[derive(Default)]
struct Scope {
    /// Names a parameter, `let`, lambda, handler arm or pattern binds.
    binders: BTreeSet<String>,
    /// How many `fn` statements declare each name.
    declared: BTreeMap<String, usize>,
    /// The parameters of each `fn` and `let`-bound lambda, in order.
    parameters: BTreeMap<String, Vec<String>>,
}

impl AstVisitor for Scope {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Function {
                name, parameters, ..
            } => {
                let count = self.declared.entry(name.clone()).or_default();
                *count = count.saturating_add(1);
                self.declare(name, parameters);
            }
            Stmt::Let { name, value, .. } => {
                let _ = self.binders.insert(name.clone());
                if let Expr::Lambda { parameters, .. } = value {
                    self.declare(name, parameters);
                }
            }
            _ => {}
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Lambda { parameters, .. } => {
                self.binders
                    .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            }
            Expr::Handler { arms, .. } => {
                self.binders
                    .extend(arms.iter().flat_map(|arm| arm.params.iter().cloned()));
            }
            Expr::Match { arms, .. } | Expr::Select { arms, .. } => {
                self.binders.extend(
                    arms.iter()
                        .flat_map(|arm| crate::freevars::pattern_bindings(&arm.pattern)),
                );
            }
            _ => {}
        }
    }
}

impl Scope {
    fn declare(&mut self, name: &str, parameters: &[Parameter]) {
        let parameters: Vec<String> = parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();
        self.binders.extend(parameters.iter().cloned());
        let _ = self.parameters.insert(name.to_owned(), parameters);
    }

    /// A name declared by exactly one `fn` and bound by nothing else.
    fn unique(&self, name: &str) -> bool {
        self.declared.get(name) == Some(&1) && !self.binders.contains(name)
    }

    /// Known functions, and each helper's called parameters: resolvable when
    /// the helper is unique, opaque from the start otherwise.
    fn classify(&self, bodies: &BTreeMap<String, Expr>) -> Callables {
        let mut callables = Callables::default();
        for (name, parameters) in &self.parameters {
            if let Some(body) = bodies.get(name) {
                self.admit(&mut callables, name, body, parameters);
            }
        }
        callables
    }

    /// Record `name` as known when a reference to it names exactly `body`,
    /// and as parametric or opaque when `body` calls a parameter.
    fn admit(&self, callables: &mut Callables, name: &str, body: &Expr, parameters: &[String]) {
        let unique = self.unique(name);
        if unique && self.closed(body, parameters) {
            let _ = callables.known.insert(name.to_owned());
        }
        let invoked = invoked_indices(body, parameters);
        if invoked.is_empty() {
            return;
        }
        if unique {
            let _ = callables.parametric.insert(name.to_owned(), invoked);
        } else {
            let _ = callables.opaque.insert(name.to_owned());
        }
    }

    /// Whether `body` reads nothing a binder elsewhere could supply.
    fn closed(&self, body: &Expr, parameters: &[String]) -> bool {
        let mut free = BTreeSet::new();
        crate::freevars::free_idents(body, &mut free);
        free.iter()
            .all(|name| parameters.contains(name) || !self.binders.contains(name))
    }
}

/// The positions of the parameters `body` calls.
fn invoked_indices(body: &Expr, parameters: &[String]) -> BTreeSet<usize> {
    parameters
        .iter()
        .enumerate()
        .filter(|(_, parameter)| invokes_parameter(body, &BTreeSet::from([(*parameter).clone()])))
        .map(|(index, _)| index)
        .collect()
}
