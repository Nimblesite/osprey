//! Which continuation-capturing requests an expression can leave unanswered.
//!
//! An effect is *captured* when one of its operations is `many`, or when one
//! of its handlers sits between such a request and the handler that answers
//! it: a `control` handler over a captured body, or an arm that itself makes a
//! captured request. Every other effect keeps its direct call.
//!
//! A function's summary is the set of captured effects it can leave
//! unanswered, plus the function-valued parameters whose own requests pass
//! through it. Both are solved to a fixed point over the whole program.

use super::facts::{Alias, Callee, Esc, Facts, Scope};
use super::table::{nested, Table};
use osprey_ast::{
    effect_name, freevars::pattern_bindings, AstNode, Expr, NamedArgument, Program, Stmt,
};
use std::collections::{BTreeMap, BTreeSet};

/// The escape analysis over one program.
pub(super) struct Analyzer<'a> {
    pub(super) table: &'a Table<'a>,
    pub(super) facts: Facts,
    changed: bool,
}

/// Solve the captured effects, stepping parameters and function summaries.
pub(super) fn solve(table: &Table<'_>, program: &Program) -> Facts {
    let captured = table
        .effects
        .iter()
        .filter(|(_, effect)| effect.many)
        .map(|(name, _)| name.clone())
        .collect();
    let facts = Facts {
        captured,
        stepping: BTreeSet::new(),
        summaries: vec![Esc::default(); table.funcs.len()],
    };
    let mut analyzer = Analyzer::new(table, facts);
    loop {
        analyzer.changed = false;
        for id in 0..table.funcs.len() {
            analyzer.summarize(id);
        }
        analyzer.top_level(&program.statements, &[]);
        if !analyzer.changed {
            return analyzer.facts;
        }
    }
}

impl<'a> Analyzer<'a> {
    pub(super) fn new(table: &'a Table<'a>, facts: Facts) -> Self {
        Self {
            table,
            facts,
            changed: false,
        }
    }

    fn summarize(&mut self, id: usize) {
        let table = self.table;
        let Some(func) = table.func(id).filter(|func| !func.bound) else {
            return;
        };
        let esc = self.esc(func.body, &Scope::of_function(table, id));
        self.record(id, esc);
    }

    fn record(&mut self, id: usize, esc: Esc) {
        if let Some(slot) = self.facts.summaries.get_mut(id) {
            if *slot != esc {
                *slot = esc;
                self.changed = true;
            }
        }
    }

    fn top_level(&mut self, statements: &[Stmt], namespace: &[String]) {
        for statement in statements {
            match statement {
                Stmt::Namespace { name, body, .. } => {
                    self.top_level(body, &nested(namespace, [name.label().to_owned()]));
                }
                Stmt::Module { path, body, .. } => {
                    let inner = nested(namespace, path.segments.iter().cloned());
                    for item in body {
                        self.top_level(std::slice::from_ref(item.declaration.as_ref()), &inner);
                    }
                }
                Stmt::Let { name, value, .. } => {
                    let scope = Scope::top(namespace);
                    let _ = self.esc(value, &scope);
                    let _ = self.bound(&scope, name, value);
                }
                Stmt::Expr { value, .. } => {
                    let _ = self.esc(value, &Scope::top(namespace));
                }
                _ => {}
            }
        }
    }

    fn capture(&mut self, effect: &str) {
        self.changed |= self.facts.captured.insert(effect.to_owned());
    }

    fn step_param(&mut self, func: usize, index: usize) {
        self.changed |= self.facts.stepping.insert((func, index));
    }

    /// What evaluating `expression` can leave unanswered.
    pub(super) fn esc(&mut self, expression: &Expr, scope: &Scope) -> Esc {
        match expression {
            Expr::Lambda { .. } => Esc::default(),
            Expr::Perform { effect, .. } => {
                let mut out = self.children(expression, scope);
                if self.facts.is_captured(effect) {
                    let _ = out.fixed.insert(effect_name::base(effect).to_owned());
                }
                out
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.call(function, arguments, named_arguments, scope),
            Expr::Handler { .. } => self.handler(expression, scope),
            Expr::Resume(_) => {
                let mut out = self.children(expression, scope);
                out.absorb(scope.resume.clone());
                out
            }
            Expr::Block {
                statements, value, ..
            } => self.block(statements, value.as_deref(), scope),
            Expr::Match { value, arms } => {
                let mut out = self.esc(value, scope);
                for arm in arms {
                    let inner = scope.binding(pattern_bindings(&arm.pattern));
                    out.absorb(self.esc(&arm.body, &inner));
                }
                out
            }
            _ => self.children(expression, scope),
        }
    }

    fn children(&mut self, expression: &Expr, scope: &Scope) -> Esc {
        let mut kids = Vec::new();
        AstNode::Expression(expression).for_each_child(|child| kids.push(child));
        let mut out = Esc::default();
        for kid in kids {
            if let AstNode::Expression(child) = kid {
                out.absorb(self.esc(child, scope));
            }
        }
        out
    }

    fn block(&mut self, statements: &[Stmt], value: Option<&Expr>, scope: &Scope) -> Esc {
        let mut scope = scope.clone();
        let mut out = Esc::default();
        for statement in statements {
            out.absorb(self.statement(statement, &mut scope));
        }
        if let Some(value) = value {
            out.absorb(self.esc(value, &scope));
        }
        out
    }

    fn statement(&mut self, statement: &Stmt, scope: &mut Scope) -> Esc {
        match statement {
            Stmt::Let { name, value, .. } => {
                let out = self.esc(value, scope);
                *scope = self.bound(scope, name, value);
                out
            }
            Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => self.esc(value, scope),
            _ => Esc::default(),
        }
    }

    /// `scope` after `let name = value`.
    pub(super) fn bound(&mut self, scope: &Scope, name: &str, value: &Expr) -> Scope {
        match self.lambda_alias(value, scope) {
            Some(alias) => scope.with_alias(name, alias),
            None => scope.binding([name.to_owned()]),
        }
    }

    pub(super) fn lambda_alias(&mut self, value: &Expr, scope: &Scope) -> Option<Alias> {
        let Expr::Lambda {
            parameters, body, ..
        } = value
        else {
            return None;
        };
        let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
        let id = self.table.lambda(value);
        let inner = match id {
            Some(id) => scope.entering(id, &names),
            None => scope.binding(names),
        };
        let latent = self.esc(body, &inner);
        if let Some(id) = id {
            self.record(id, latent.clone());
        }
        Some(Alias {
            id,
            latent,
            arity: parameters.len(),
        })
    }

    /// What `function` names from `scope`.
    pub(super) fn callee(&self, function: &Expr, scope: &Scope) -> Callee {
        let name = match function {
            Expr::Identifier(name) => name.clone(),
            Expr::Path(path) => path.to_string(),
            Expr::TypeApply { function, .. } => return self.callee(function, scope),
            _ => return Callee::Other,
        };
        if let Some(alias) = scope.aliases.get(&name) {
            return alias
                .id
                .map_or_else(|| Callee::Alias(alias.clone()), Callee::Function);
        }
        if let Some(index) = scope.params.get(&name) {
            return Callee::Param(*index);
        }
        if scope.hidden.contains(&name) {
            return Callee::Other;
        }
        self.table
            .resolve(&scope.namespace, &name)
            .or_else(|| self.table.global(&scope.namespace, &name))
            .map_or(Callee::Other, Callee::Function)
    }

    /// Each argument of a call to function `id`, with the parameter it fills.
    pub(super) fn placed<'e>(
        &self,
        id: usize,
        arguments: &'e [Expr],
        named: &'e [NamedArgument],
    ) -> Vec<(Option<usize>, &'e Expr)> {
        let params = self.table.func(id).map(|func| func.params.as_slice());
        let position = |name: &str| params.and_then(|all| all.iter().position(|p| p == name));
        let positional = arguments.iter().enumerate().map(|(i, arg)| (Some(i), arg));
        let by_name = named.iter().map(|arg| (position(&arg.name), &arg.value));
        positional.chain(by_name).collect()
    }

    fn call(
        &mut self,
        function: &Expr,
        arguments: &[Expr],
        named: &[NamedArgument],
        scope: &Scope,
    ) -> Esc {
        let mut out = Esc::default();
        for argument in arguments.iter().chain(named.iter().map(|arg| &arg.value)) {
            out.absorb(self.esc(argument, scope));
        }
        match self.callee(function, scope) {
            Callee::Function(id) => {
                let placed = self.placed(id, arguments, named);
                out.absorb(self.applied(id, &placed, scope));
            }
            Callee::Alias(alias) => out.absorb(alias.latent),
            Callee::Param(index) => out.absorb(self.param_call(index, scope)),
            Callee::Other => {
                out.absorb(self.esc(function, scope));
                if let Some((_, callback)) = self.list_walk(function, arguments, scope) {
                    out.absorb(self.latent(callback, scope).unwrap_or_default());
                }
            }
        }
        out
    }

    /// The step form of a list builtin call, with its callback argument, when
    /// `function` names the builtin itself rather than a local or a function.
    pub(super) fn list_walk<'e>(
        &self,
        function: &Expr,
        arguments: &'e [Expr],
        scope: &Scope,
    ) -> Option<(&'static str, &'e Expr)> {
        let Expr::Identifier(name) = function else {
            return None;
        };
        if !matches!(self.callee(function, scope), Callee::Other) || scope.hidden.contains(name) {
            return None;
        }
        let (helper, index) = super::build::list_walk(name)?;
        arguments.get(index).map(|callback| (helper, callback))
    }

    fn param_call(&self, index: usize, scope: &Scope) -> Esc {
        match scope.func {
            Some(func) if self.facts.takes_step(func, index) => Esc::param(index),
            _ => Esc::default(),
        }
    }

    /// What calling function `id` with these arguments leaves unanswered,
    /// beyond evaluating the arguments themselves.
    pub(super) fn applied(
        &mut self,
        id: usize,
        placed: &[(Option<usize>, &Expr)],
        scope: &Scope,
    ) -> Esc {
        let summary = self.facts.summary(id);
        let mut out = Esc {
            fixed: summary.fixed.clone(),
            params: BTreeMap::new(),
        };
        for (index, argument) in placed {
            let (Some(index), Some(latent)) = (*index, self.latent(argument, scope)) else {
                continue;
            };
            if !latent.is_empty() {
                self.step_param(id, index);
            }
            if self.facts.takes_step(id, index) {
                self.forward(argument, scope);
            }
            if let Some(hidden) = summary.params.get(&index) {
                out.absorb(latent.behind(hidden));
            }
        }
        out
    }

    /// A parameter handed on to a stepping parameter is itself stepping.
    fn forward(&mut self, argument: &Expr, scope: &Scope) {
        if let (Callee::Param(index), Some(func)) = (self.callee(argument, scope), scope.func) {
            self.step_param(func, index);
        }
    }

    /// What calling the callable `argument` later leaves unanswered, when
    /// `argument` is a callable this pass can follow.
    pub(super) fn latent(&mut self, argument: &Expr, scope: &Scope) -> Option<Esc> {
        if let Some(alias) = self.lambda_alias(argument, scope) {
            return Some(alias.latent);
        }
        match self.callee(argument, scope) {
            Callee::Alias(alias) => Some(alias.latent),
            Callee::Param(index) => Some(self.param_call(index, scope)),
            Callee::Function(id) => Some(Esc {
                fixed: self.facts.summary(id).fixed,
                params: BTreeMap::new(),
            }),
            Callee::Other => None,
        }
    }

    /// What the handled body leaves unanswered past this handler.
    pub(super) fn through(&mut self, handler: &Expr, scope: &Scope) -> Esc {
        let Expr::Handler { effect, body, .. } = handler else {
            return Esc::default();
        };
        let body_esc = self.esc(body, scope);
        if self.facts.is_captured(effect) {
            body_esc.answered(effect_name::base(effect))
        } else {
            body_esc
        }
    }

    fn handler(&mut self, handler: &Expr, scope: &Scope) -> Esc {
        let Expr::Handler {
            effect,
            arms,
            return_clause,
            finally,
            body,
            ..
        } = handler
        else {
            return Esc::default();
        };
        let through = self.through(handler, scope);
        let captured = self.facts.is_captured(effect);
        let mut arm_esc = Esc::default();
        for arm in arms {
            let inner = scope.in_arm(arm, through.clone(), captured);
            arm_esc.absorb(self.esc(&arm.body, &inner));
        }
        let controls = self.table.effect(effect).is_some_and(|e| e.control);
        let between = (controls && self.open(body, scope)) || !arm_esc.is_empty();
        if between && self.table.effect(effect).is_some() {
            self.capture(effect_name::base(effect));
        }
        let mut out = through;
        out.absorb(arm_esc);
        for clause in return_clause.iter().chain(finally.iter()) {
            out.absorb(self.clause(clause, scope));
        }
        out
    }

    fn clause(&mut self, clause: &Expr, scope: &Scope) -> Esc {
        match clause {
            Expr::Lambda {
                parameters, body, ..
            } => self.esc(
                body,
                &scope.binding(parameters.iter().map(|p| p.name.clone())),
            ),
            other => self.esc(other, scope),
        }
    }
}

impl Analyzer<'_> {
    /// Whether rewriting `expression` must produce a step: it leaves a
    /// captured request unanswered, or it resumes a captured arm.
    pub(super) fn open(&mut self, expression: &Expr, scope: &Scope) -> bool {
        !self.esc(expression, scope).is_empty()
            || (scope.resume_captured && mentions_resume(expression))
    }
}

/// Whether `expression` resumes the arm it is written in. A nested handler's
/// arms resume their own continuation, and a lambda body resumes nothing.
fn mentions_resume(expression: &Expr) -> bool {
    match expression {
        Expr::Resume(_) => true,
        Expr::Lambda { .. } => false,
        Expr::Handler {
            body,
            return_clause,
            finally,
            ..
        } => std::iter::once(body.as_ref())
            .chain(return_clause.as_deref())
            .chain(finally.as_deref())
            .any(mentions_resume),
        _ => {
            let mut found = false;
            AstNode::Expression(expression).for_each_child(|child| {
                found |= match child {
                    AstNode::Expression(inner) => mentions_resume(inner),
                    AstNode::Statement(statement) => statement_resumes(statement),
                };
            });
            found
        }
    }
}

fn statement_resumes(statement: &Stmt) -> bool {
    match statement {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            mentions_resume(value)
        }
        _ => false,
    }
}
