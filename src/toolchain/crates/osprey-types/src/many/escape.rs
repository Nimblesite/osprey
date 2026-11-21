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

use super::table::{nested, Table};
use osprey_ast::{
    effect_name, freevars::pattern_bindings, AstNode, Expr, HandlerArm, NamedArgument, Program,
    Stmt,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The captured requests an expression can leave unanswered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Esc {
    /// Captured effects requested here and answered by no handler here.
    pub(super) fixed: BTreeSet<String>,
    /// Parameters of the enclosing function whose requests pass through, each
    /// with the effects a handler here already answers.
    pub(super) params: BTreeMap<usize, BTreeSet<String>>,
}

impl Esc {
    pub(super) fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.params.is_empty()
    }

    fn param(index: usize) -> Self {
        Self {
            fixed: BTreeSet::new(),
            params: BTreeMap::from([(index, BTreeSet::new())]),
        }
    }

    pub(super) fn absorb(&mut self, other: Self) {
        self.fixed.extend(other.fixed);
        for (index, answered) in other.params {
            let merged = match self.params.get(&index) {
                Some(mine) => mine.intersection(&answered).cloned().collect(),
                None => answered,
            };
            let _ = self.params.insert(index, merged);
        }
    }

    /// What is left once a handler for `effect` encloses this.
    fn answered(mut self, effect: &str) -> Self {
        let _ = self.fixed.remove(effect);
        for hidden in self.params.values_mut() {
            let _ = hidden.insert(effect.to_owned());
        }
        self
    }

    /// This set as it leaves a callee that already answers `hidden`.
    fn behind(mut self, hidden: &BTreeSet<String>) -> Self {
        self.fixed.retain(|effect| !hidden.contains(effect));
        for own in self.params.values_mut() {
            own.extend(hidden.iter().cloned());
        }
        self
    }
}

/// A `let`-bound lambda: what calling it leaves unanswered, and its arity.
#[derive(Clone, Debug)]
pub(super) struct Alias {
    pub(super) latent: Esc,
    pub(super) arity: usize,
}

/// The names visible at one point of one function body.
#[derive(Clone, Debug, Default)]
pub(super) struct Scope {
    pub(super) func: Option<usize>,
    pub(super) namespace: Vec<String>,
    params: HashMap<String, usize>,
    aliases: HashMap<String, Alias>,
    hidden: HashSet<String>,
    /// What `resume(..)` in the enclosing arm can leave unanswered.
    resume: Esc,
    /// Whether the enclosing arm answers a captured effect, so its `resume`
    /// is a closure call.
    resume_captured: bool,
    /// The closure a rewritten arm's `resume` calls.
    pub(super) resume_name: Option<String>,
}

impl Scope {
    pub(super) fn of_function(table: &Table<'_>, id: usize) -> Self {
        let Some(func) = table.func(id) else {
            return Self::default();
        };
        Self {
            func: Some(id),
            namespace: func.scope.clone(),
            params: func.params.iter().cloned().zip(0..).collect(),
            ..Self::default()
        }
    }

    pub(super) fn top(namespace: &[String]) -> Self {
        Self {
            namespace: namespace.to_vec(),
            ..Self::default()
        }
    }

    /// This scope with `names` bound to plain local values.
    pub(super) fn binding(&self, names: impl IntoIterator<Item = String>) -> Self {
        let mut inner = self.clone();
        for name in names {
            let _ = inner.params.remove(&name);
            let _ = inner.aliases.remove(&name);
            let _ = inner.hidden.insert(name);
        }
        inner
    }

    fn with_alias(&self, name: &str, alias: Alias) -> Self {
        let mut inner = self.binding([name.to_owned()]);
        let _ = inner.aliases.insert(name.to_owned(), alias);
        inner
    }

    /// The scope of one handler arm's body.
    pub(super) fn in_arm(&self, arm: &HandlerArm, resume: Esc, captured: bool) -> Self {
        let mut inner = self.binding(arm.params.iter().cloned());
        inner.resume = resume;
        inner.resume_captured = captured;
        inner.resume_name = None;
        inner
    }
}

/// What a call's callee expression names.
#[derive(Debug)]
pub(super) enum Callee {
    Function(usize),
    Alias(Alias),
    Param(usize),
    Other,
}

/// The solved program-wide facts.
#[derive(Clone, Debug, Default)]
pub(super) struct Facts {
    /// Effects whose requests carry their continuation.
    pub(super) captured: BTreeSet<String>,
    /// `(function, parameter)` pairs that receive continuation-capturing callables.
    stepping: BTreeSet<(usize, usize)>,
    summaries: Vec<Esc>,
}

impl Facts {
    pub(super) fn is_captured(&self, mention: &str) -> bool {
        self.captured.contains(effect_name::base(mention))
    }

    pub(super) fn summary(&self, id: usize) -> Esc {
        self.summaries.get(id).cloned().unwrap_or_default()
    }

    /// Whether calling function `id` yields a step rather than a plain value.
    pub(super) fn returns_step(&self, id: usize) -> bool {
        self.summaries.get(id).is_some_and(|esc| !esc.is_empty())
    }

    /// Whether parameter `index` of function `id` holds a step-returning callable.
    pub(super) fn takes_step(&self, id: usize, index: usize) -> bool {
        self.stepping.contains(&(id, index))
    }
}

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
        let Some(func) = table.func(id) else { return };
        let esc = self.esc(func.body, &Scope::of_function(table, id));
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
                Stmt::Let { value, .. } | Stmt::Expr { value, .. } => {
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
        let inner = scope.binding(parameters.iter().map(|p| p.name.clone()));
        Some(Alias {
            latent: self.esc(body, &inner),
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
            return Callee::Alias(alias.clone());
        }
        if let Some(index) = scope.params.get(&name) {
            return Callee::Param(*index);
        }
        if scope.hidden.contains(&name) {
            return Callee::Other;
        }
        self.table
            .resolve(&scope.namespace, &name)
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
            Callee::Other => out.absorb(self.esc(function, scope)),
        }
        out
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
            } => self.esc(body, &scope.binding(parameters.iter().map(|p| p.name.clone()))),
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
