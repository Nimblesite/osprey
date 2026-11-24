//! Scope-resolved `mut` reachability for `many` arms.
//!
//! [MULTI-REPLAY-STATE] forbids branches implicitly sharing a mutable cell,
//! so what an arm may not capture is decided by the names its body can reach,
//! resolved in the arm's own lexical scope: a free identifier counts only
//! when it denotes a `mut` cell visible at the arm, and a called local
//! function or `let`-bound lambda contributes the cells its own body reaches
//! in the scope where it was defined. Collecting `mut` names program-wide
//! instead would collide an immutable `let` with an unrelated `mut` of the
//! same spelling in another function: a wrong name resolution, not the
//! conservative whole-region row [MULTI-REPLAY-COARSE] permits.

use osprey_ast::{
    freevars::{free_idents, pattern_bindings},
    AstNode, Expr, HandlerArm, MatchArm, Parameter, Program, SelectOperation, Stmt,
};
use std::collections::{BTreeMap, BTreeSet};

/// What a name in lexical scope denotes for the state rule.
#[derive(Clone)]
enum Denotation {
    /// A `mut` cell: one shared cell across every branch.
    Cell,
    /// A local function or `let`-bound lambda whose body reachability follows.
    Callable(usize),
    /// An immutable binding, parameter or builtin: safe to close over.
    Plain,
}

/// The names visible at one point, by what they denote.
type Env = BTreeMap<String, Denotation>;

/// One callable body: its free names and the scope it was defined in.
struct Callable {
    free: BTreeSet<String>,
    env: Env,
}

/// The statements of one declaration level, queued with its reserved scope.
#[derive(Clone, Copy)]
enum Declarations<'a> {
    /// A file, block-list or namespace body.
    Statements(&'a [Stmt]),
    /// A module body: its items' declarations.
    Items(&'a [osprey_ast::ModuleItem]),
}

/// Every handler arm's scope and the callables reachability follows.
#[derive(Default)]
pub(super) struct State<'a> {
    callables: Vec<Callable>,
    /// One walk job per declaration level, queued while scopes are reserved.
    jobs: Vec<(Declarations<'a>, Env)>,
    /// Each arm's address, free names and the names visible at it.
    arms: Vec<(usize, BTreeSet<String>, Env)>,
}

impl<'a> State<'a> {
    /// Reserve every declaration level of `program`, then walk them all.
    pub(super) fn collect(program: &'a Program) -> Self {
        let mut state = Self::default();
        let _ = state.reserve(&program.statements, &Env::new(), &[]);
        for (declarations, env) in std::mem::take(&mut state.jobs) {
            state.walk(declarations, &env);
        }
        state
    }

    /// Each arm's address paired with the `mut` cells its body can reach,
    /// following called callables to a fixed point. Implements
    /// [MULTI-REPLAY-STATE].
    pub(super) fn reachability(&self) -> BTreeMap<usize, BTreeSet<String>> {
        let mut memo = vec![BTreeSet::new(); self.callables.len()];
        while self.grows(&mut memo) {}
        self.arms
            .iter()
            .map(|(at, free, env)| (*at, Self::resolve(free, env, &memo)))
            .collect()
    }

    /// One round of growing each callable's reached cells; whether any grew.
    fn grows(&self, memo: &mut [BTreeSet<String>]) -> bool {
        let mut grew = false;
        for (id, callable) in self.callables.iter().enumerate() {
            let reached = Self::resolve(&callable.free, &callable.env, memo);
            if let Some(current) = memo.get_mut(id) {
                grew |= *current != reached;
                *current = reached;
            }
        }
        grew
    }

    /// The cells `names` denote in `env`, with callables read through `memo`.
    fn resolve(names: &BTreeSet<String>, env: &Env, memo: &[BTreeSet<String>]) -> BTreeSet<String> {
        names
            .iter()
            .filter_map(|name| match env.get(name) {
                Some(Denotation::Cell) => Some(BTreeSet::from([name.clone()])),
                Some(Denotation::Callable(id)) => memo.get(*id).cloned(),
                Some(Denotation::Plain) | None => None,
            })
            .fold(BTreeSet::new(), |mut all, found| {
                all.extend(found);
                all
            })
    }

    fn placeholder(&mut self, body: &Expr) -> usize {
        let mut free = BTreeSet::new();
        free_idents(body, &mut free);
        let id = self.callables.len();
        self.callables.push(Callable {
            free,
            env: Env::new(),
        });
        id
    }
}

/// The reserved bindings of one declaration level: everything the level makes
/// visible everywhere in itself, however its statements are ordered.
struct Level {
    env: Env,
    /// The members this level itself declares, for its parent to qualify.
    own: Env,
    /// `(callable id, parameters)` of this level's own functions.
    fns: Vec<(usize, Vec<String>)>,
}

impl Level {
    fn declare(&mut self, prefix: &[String], name: &str, denotation: Denotation) {
        let _ = self.own.insert(name.to_owned(), denotation.clone());
        let _ = self.env.insert(qualify(prefix, name), denotation);
    }
}

/// One declaration level: file scope, a namespace or a module body. Functions
/// are visible throughout their level however they are ordered, and `mut`
/// cells of the level conservatively reach every function body in it, which
/// fails closed the way [MULTI-REPLAY-STATE] requires.
impl<'a> State<'a> {
    fn reserve(&mut self, statements: &'a [Stmt], outer: &Env, prefix: &[String]) -> Level {
        let mut level = Level {
            env: outer.clone(),
            own: Env::new(),
            fns: Vec::new(),
        };
        for statement in statements {
            self.reserve_statement(statement, &mut level, prefix);
        }
        for statement in statements {
            self.reserve_nested(statement, &mut level, prefix);
        }
        self.finished(&level);
        self.jobs
            .push((Declarations::Statements(statements), level.env.clone()));
        level
    }

    /// Give every function of `level` the completed scope it is defined in.
    fn finished(&mut self, level: &Level) {
        for (id, params) in &level.fns {
            if let Some(callable) = self.callables.get_mut(*id) {
                callable.env = without(level.env.clone(), params);
            }
        }
    }

    /// One member declaration: a function, a `mut` cell or a `let`-bound
    /// lambda, each callable or shared however the level orders its items.
    fn reserve_statement(&mut self, statement: &'a Stmt, level: &mut Level, prefix: &[String]) {
        match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => self.member(level, prefix, name, body, param_names(parameters)),
            Stmt::Let {
                mutable: true,
                name,
                ..
            } => level.declare(prefix, name, Denotation::Cell),
            Stmt::Let {
                name,
                value: Expr::Lambda {
                    parameters, body, ..
                },
                ..
            } => self.member(level, prefix, name, body, param_names(parameters)),
            _ => {}
        }
    }

    /// Reserve one callable member and its parameters for the scope fill.
    fn member(
        &mut self,
        level: &mut Level,
        prefix: &[String],
        name: &str,
        body: &Expr,
        params: Vec<String>,
    ) {
        let id = self.placeholder(body);
        level.declare(prefix, name, Denotation::Callable(id));
        level.fns.push((id, params));
    }

    /// A namespace or module inside this level: reserved against the level's
    /// own members, then exposed to it under qualified names.
    fn reserve_nested(&mut self, statement: &'a Stmt, level: &mut Level, prefix: &[String]) {
        let Level { env, .. } = level;
        if let Stmt::Namespace { name, body, .. } = statement {
            let inner = nested(prefix, [name.label().to_owned()]);
            let reserved = self.reserve(body, env, &inner);
            for (member, denotation) in reserved.own {
                let _ = env.insert(qualify(&inner, &member), denotation);
            }
        }
        if let Stmt::Module { path, body, .. } = statement {
            let inner = nested(prefix, path.segments.iter().cloned());
            let reserved = self.reserve_items(body, env, &inner);
            for (member, denotation) in reserved.own {
                let _ = env.insert(qualify(&inner, &member), denotation);
            }
        }
    }

    fn reserve_items(
        &mut self,
        items: &'a [osprey_ast::ModuleItem],
        outer: &Env,
        prefix: &[String],
    ) -> Level {
        let mut level = Level {
            env: outer.clone(),
            own: Env::new(),
            fns: Vec::new(),
        };
        for item in items {
            self.reserve_statement(&item.declaration, &mut level, prefix);
        }
        self.finished(&level);
        self.jobs
            .push((Declarations::Items(items), level.env.clone()));
        level
    }
}

/// Walk one declaration level, recording each handler arm with the names
/// visible at it: the queued counterpart of [`State::reserve`].
impl<'a> State<'a> {
    fn walk(&mut self, declarations: Declarations<'a>, env: &Env) {
        let mut local = env.clone();
        match declarations {
            Declarations::Statements(statements) => {
                for statement in statements {
                    self.walk_statement(statement, env, &mut local);
                }
            }
            Declarations::Items(items) => {
                for item in items {
                    self.walk_statement(&item.declaration, env, &mut local);
                }
            }
        }
    }

    fn walk_statement(&mut self, statement: &'a Stmt, reserved: &Env, local: &mut Env) {
        match statement {
            Stmt::Function { name, body, .. } => {
                if let Some(Denotation::Callable(id)) = reserved.get(name) {
                    let env = self
                        .callables
                        .get(*id)
                        .map_or_else(Env::new, |c| c.env.clone());
                    self.expression(body, &env);
                }
            }
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => self.reserved_let(name, *mutable, value, reserved, local),
            Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
                self.expression(value, local);
            }
            _ => AstNode::Statement(statement).for_each_child(|child| self.node(child, local)),
        }
    }

    /// A `let` at a declaration level: a directly bound lambda walks in the
    /// scope reserved for it, everything else where it stands.
    fn reserved_let(
        &mut self,
        name: &str,
        mutable: bool,
        value: &'a Expr,
        reserved: &Env,
        local: &mut Env,
    ) {
        let denotation = match reserved.get(name) {
            Some(denotation @ Denotation::Callable(id)) if matches!(value, Expr::Lambda { .. }) => {
                let env = self
                    .callables
                    .get(*id)
                    .map_or_else(Env::new, |c| c.env.clone());
                self.expression(value, &env);
                denotation.clone()
            }
            _ => {
                self.expression(value, local);
                cell_or_plain(mutable)
            }
        };
        let _ = local.insert(name.to_owned(), denotation);
    }

    fn node(&mut self, node: AstNode<'a>, env: &Env) {
        if let AstNode::Expression(expression) = node {
            self.expression(expression, env);
        }
    }

    fn expression(&mut self, expression: &'a Expr, env: &Env) {
        match expression {
            Expr::Lambda {
                parameters, body, ..
            } => {
                let inner = without(env.clone(), &param_names(parameters));
                self.expression(body, &inner);
            }
            Expr::Block {
                statements, value, ..
            } => self.block(statements, value.as_deref(), env),
            Expr::Match { value, arms } => self.scrutinee(value, arms, env),
            Expr::Select { operations, arms } => self.selections(operations, arms, env),
            Expr::Handler {
                arms,
                body,
                return_clause,
                finally,
                ..
            } => self.region(
                arms,
                body,
                return_clause.as_deref(),
                finally.as_deref(),
                env,
            ),
            _ => AstNode::Expression(expression).for_each_child(|child| self.node(child, env)),
        }
    }

    /// A block: bindings accumulate statement by statement, lambdas among them.
    fn block(&mut self, statements: &'a [Stmt], value: Option<&'a Expr>, env: &Env) {
        let mut local = env.clone();
        for statement in statements {
            match statement {
                Stmt::Let {
                    name,
                    mutable,
                    value,
                    ..
                } => self.local_let(name, *mutable, value, &mut local),
                Stmt::Function {
                    name,
                    parameters,
                    body,
                    ..
                } => self.local_function(name, parameters, body, &mut local),
                Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
                    self.expression(value, &local);
                }
                _ => AstNode::Statement(statement).for_each_child(|child| self.node(child, &local)),
            }
        }
        if let Some(value) = value {
            self.expression(value, &local);
        }
    }

    /// One block binding: the value is walked before the name is bound, the
    /// way `let` itself evaluates.
    fn local_let(&mut self, name: &str, mutable: bool, value: &'a Expr, local: &mut Env) {
        let denotation = match value {
            Expr::Lambda {
                parameters, body, ..
            } => {
                let id = self.placeholder(body);
                if let Some(callable) = self.callables.get_mut(id) {
                    callable.env = without(local.clone(), &param_names(parameters));
                }
                Denotation::Callable(id)
            }
            _ => cell_or_plain(mutable),
        };
        self.expression(value, local);
        let _ = local.insert(name.to_owned(), denotation);
    }

    /// A function declared inside a block: a closure of the bindings so far.
    fn local_function(
        &mut self,
        name: &str,
        parameters: &[Parameter],
        body: &'a Expr,
        local: &mut Env,
    ) {
        let id = self.placeholder(body);
        let inner = without(local.clone(), &param_names(parameters));
        if let Some(callable) = self.callables.get_mut(id) {
            callable.env = inner.clone();
        }
        let _ = local.insert(name.to_owned(), Denotation::Callable(id));
        self.expression(body, &inner);
    }

    fn scrutinee(&mut self, value: &'a Expr, arms: &'a [MatchArm], env: &Env) {
        self.expression(value, env);
        self.arm_bodies(arms, env);
    }

    fn selections(&mut self, operations: &'a [SelectOperation], arms: &'a [MatchArm], env: &Env) {
        for operand in operations.iter().flat_map(SelectOperation::operands) {
            self.expression(operand, env);
        }
        self.arm_bodies(arms, env);
    }

    /// Each arm's body, recorded with the names its pattern takes out of
    /// scope — the names it cannot close over.
    fn arm_bodies(&mut self, arms: &'a [MatchArm], env: &Env) {
        for arm in arms {
            let inner = without(env.clone(), &pattern_bindings(&arm.pattern));
            self.expression(&arm.body, &inner);
        }
    }

    /// One handler: each arm's body is recorded with the names it can close
    /// over, then walked for the handlers nested inside it.
    fn region(
        &mut self,
        arms: &'a [HandlerArm],
        body: &'a Expr,
        return_clause: Option<&'a Expr>,
        finally: Option<&'a Expr>,
        env: &Env,
    ) {
        for arm in arms {
            let inner = without(env.clone(), &arm.params);
            let mut free = BTreeSet::new();
            free_idents(&arm.body, &mut free);
            self.arms
                .push((std::ptr::from_ref(arm).addr(), free, inner.clone()));
            self.expression(&arm.body, &inner);
        }
        self.expression(body, env);
        for clause in return_clause.into_iter().chain(finally) {
            self.expression(clause, env);
        }
    }
}

/// `env` with `params` rebound as plain immutable names.
fn without(mut env: Env, params: &[String]) -> Env {
    for param in params {
        let _ = env.insert(param.clone(), Denotation::Plain);
    }
    env
}

/// A cell when `mutable`, otherwise a plain binding.
fn cell_or_plain(mutable: bool) -> Denotation {
    if mutable {
        Denotation::Cell
    } else {
        Denotation::Plain
    }
}

/// The names of `parameters`.
fn param_names(parameters: &[Parameter]) -> Vec<String> {
    parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect()
}

/// `prefix` extended by `more`, the way namespaces and modules nest names.
fn nested(prefix: &[String], more: impl IntoIterator<Item = String>) -> Vec<String> {
    prefix.iter().cloned().chain(more).collect()
}

/// `prefix::name`, or plain `name` at file scope.
fn qualify(prefix: &[String], name: &str) -> String {
    let mut qualified = prefix.to_vec();
    qualified.push(name.to_owned());
    qualified.join("::")
}
