//! Steps in evaluation order.
//!
//! A step expression evaluates its operands left to right. Each operand that
//! captures becomes a `__manyBind` whose closure holds everything after it,
//! and each operand evaluated before the last capture is bound first, so work
//! done before a capture is never repeated by a second resumption
//! ([MULTI-HANDLE-MANY]).

use super::build::{self, block, done, ident, RUN, UNIT};
use super::facts::{Callee, Scope};
use super::rewrite::{rebuild_call, Rewriter};
use osprey_ast::{effect_name, mutate, Expr, MatchArm, Pattern, Stmt};

/// One operand of a step expression.
pub(super) enum Item<'e> {
    /// Already rewritten, and safe to evaluate after a capture.
    Ready(Expr),
    /// Source, still to rewrite.
    Raw(&'e Expr),
}

/// A binding hoisted in front of the expression that uses it.
enum Hoisted {
    Captured(String, Expr),
    Value(String, Expr),
}

const STATE: &str = "an assignment inside a region whose continuation a `many` handler captures: \
every resumption runs its own branch, so a branch cannot write a shared `mut` binding. Bind each \
new value with `let`, or keep the state in a handler: an arm may write its handler's own `mut` \
state ([MULTI-REPLAY-STATE])";

impl Rewriter<'_> {
    /// Rewrite `expression` where it yields a step.
    pub(super) fn step(&mut self, expression: &Expr, scope: &Scope) -> Expr {
        if !self.analyzer.open(expression, scope) {
            return done(self.direct(expression, scope));
        }
        match expression {
            Expr::Block {
                statements, value, ..
            } => self.step_block(statements, value.as_deref(), scope),
            Expr::Match { value, arms } => self.step_match(value, arms, scope),
            Expr::Perform { effect, .. } if self.analyzer.facts.is_captured(effect) => {
                self.step_request(expression, scope)
            }
            Expr::Call { .. } => self.step_call(expression, scope),
            Expr::Resume(value) => self.step_resume(value.as_deref(), scope),
            Expr::Handler { effect, .. } if self.analyzer.facts.is_captured(effect) => {
                self.answered(expression, scope)
            }
            Expr::Handler { .. } => self.reinstalled(expression, scope),
            Expr::Binary {
                op, left, right, ..
            } if self.analyzer.open(right, scope) && is_lazy(op) => {
                self.step_lazy(op, left, right, scope)
            }
            Expr::Spawn(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::Send { .. }
            | Expr::Recv(_)
            | Expr::Select { .. }
            | Expr::Mask { .. } => {
                self.reject(
                    "a `many` continuation cannot span a fiber, channel or mask operation; \
make the request before it and pass the answer in"
                        .to_owned(),
                );
                done(expression.clone())
            }
            _ => self.step_operands(expression, scope),
        }
    }

    /// Evaluate `items` in order, then `build` the step from their values.
    pub(super) fn sequence(
        &mut self,
        items: Vec<Item<'_>>,
        scope: &Scope,
        build: impl FnOnce(&mut Self, Vec<Expr>) -> Expr,
    ) -> Expr {
        let captures: Vec<bool> = items
            .iter()
            .map(|item| match item {
                Item::Raw(raw) => self.analyzer.open(raw, scope),
                Item::Ready(_) => false,
            })
            .collect();
        let last = captures.iter().rposition(|capture| *capture);
        let mut hoisted = Vec::new();
        let mut values = Vec::with_capacity(items.len());
        for (index, (item, capture)) in items.into_iter().zip(captures).enumerate() {
            let early = last.is_some_and(|last| index < last);
            // An operand after the last capture is evaluated inside the
            // replayed remainder, however pure the sequence's context was.
            let replayed = self.replaying || last.is_some_and(|last| index > last);
            values.push(self.operand(item, capture, early, replayed, scope, &mut hoisted));
        }
        let inner = build(self, values);
        hoisted
            .into_iter()
            .rev()
            .fold(inner, |rest, binding| match binding {
                Hoisted::Captured(name, step) => self.fresh.bind(step, name, rest),
                Hoisted::Value(name, value) => self.fresh.let_in(name, value, rest),
            })
    }

    fn operand(
        &mut self,
        item: Item<'_>,
        capture: bool,
        early: bool,
        replayed: bool,
        scope: &Scope,
        hoisted: &mut Vec<Hoisted>,
    ) -> Expr {
        match item {
            Item::Ready(value) => value,
            Item::Raw(raw) if capture => {
                let name = self.fresh.name("v");
                hoisted.push(Hoisted::Captured(name.clone(), self.step(raw, scope)));
                ident(&name)
            }
            Item::Raw(raw) if early && !self.settled(raw) => {
                let name = self.fresh.name("t");
                hoisted.push(Hoisted::Value(name.clone(), self.direct(raw, scope)));
                ident(&name)
            }
            Item::Raw(raw) if replayed => self.replayed(|this| this.direct(raw, scope)),
            Item::Raw(raw) => self.direct(raw, scope),
        }
    }

    /// Whether evaluating `expression` later gives the same value.
    fn settled(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Path(_)
            | Expr::Lambda { .. } => true,
            Expr::Identifier(name) => !self.mutables.contains(name),
            _ => false,
        }
    }

    /// Any other expression: its direct children are its operands.
    fn step_operands(&mut self, expression: &Expr, scope: &Scope) -> Expr {
        let mut node = expression.clone();
        let mut operands = Vec::new();
        mutate::children_mut(&mut node, &mut |child| operands.push(child.clone()));
        let items = operands.iter().map(Item::Raw).collect();
        self.sequence(items, scope, |_, values| {
            let mut values = values.into_iter();
            mutate::children_mut(&mut node, &mut |child| {
                if let Some(value) = values.next() {
                    *child = value;
                }
            });
            done(node)
        })
    }

    fn step_match(&mut self, value: &Expr, arms: &[MatchArm], scope: &Scope) -> Expr {
        self.sequence(vec![Item::Raw(value)], scope, |this, mut values| {
            Expr::Match {
                value: Box::new(values.pop().unwrap_or_else(|| value.clone())),
                arms: this.arms(arms, scope, true),
            }
        })
    }

    /// `left && right` / `left || right` where `right` captures: `right` runs
    /// only when `left` does not decide the answer.
    fn step_lazy(&mut self, op: &str, left: &Expr, right: &Expr, scope: &Scope) -> Expr {
        if op != "&&" && op != "||" {
            self.reject(format!(
                "the right side of `{op}` makes a request a `many` handler captures; bind it \
with `let` before `{op}`"
            ));
            return done(right.clone());
        }
        let decided = op == "||";
        self.sequence(vec![Item::Raw(left)], scope, |this, mut values| {
            let arm = |holds: bool, body: Expr| MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Bool(holds))),
                body,
            };
            let rest = this.step(right, scope);
            Expr::Match {
                value: Box::new(values.pop().unwrap_or_else(|| left.clone())),
                arms: vec![arm(decided, done(Expr::Bool(decided))), arm(!decided, rest)],
            }
        })
    }

    fn step_request(&mut self, request: &Expr, scope: &Scope) -> Expr {
        let Expr::Perform {
            effect,
            operation,
            arguments,
            named_arguments,
            position,
        } = request
        else {
            return request.clone();
        };
        self.at = position.or(self.at);
        if !named_arguments.is_empty() {
            self.reject(format!(
                "`{effect}.{operation}` is captured by a `many` region; pass its arguments by position"
            ));
        }
        let name = build::request_name(effect_name::base(effect), operation);
        let items = arguments.iter().map(Item::Raw).collect();
        self.sequence(items, scope, |_, values| build::call(&name, values))
    }

    fn step_call(&mut self, call: &Expr, scope: &Scope) -> Expr {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = call
        else {
            return call.clone();
        };
        let steps = match self.analyzer.callee(function, scope) {
            Callee::Function(id) => return self.step_function_call(id, call, scope),
            Callee::Alias(alias) => !alias.latent.is_empty(),
            Callee::Param(index) => self.param_steps(index, scope),
            Callee::Other => return self.step_builtin(call, scope),
        };
        let items = arguments
            .iter()
            .chain(named_arguments.iter().map(|argument| &argument.value))
            .map(Item::Raw)
            .collect();
        self.sequence(items, scope, |_, values| {
            let rebuilt = rebuild_call(function, named_arguments, values);
            if steps {
                rebuilt
            } else {
                done(rebuilt)
            }
        })
    }

    /// A call that names no function this pass follows. A list builtin whose
    /// callback captures becomes its step form; anything else evaluates its
    /// operands in order.
    fn step_builtin(&mut self, call: &Expr, scope: &Scope) -> Expr {
        let Expr::Call {
            function,
            arguments,
            ..
        } = call
        else {
            return call.clone();
        };
        let Some((helper, callback)) = self.analyzer.list_walk(function, arguments, scope) else {
            return self.step_operands(call, scope);
        };
        let captures = self
            .analyzer
            .latent(callback, scope)
            .is_some_and(|latent| !latent.is_empty());
        if !captures {
            return self.step_operands(call, scope);
        }
        let mut items = Vec::with_capacity(arguments.len());
        for argument in arguments {
            items.push(if std::ptr::eq(argument, callback) {
                Item::Ready(self.stepping(argument, scope))
            } else {
                Item::Raw(argument)
            });
        }
        self.sequence(items, scope, |_, values| build::call(helper, values))
    }

    fn step_function_call(&mut self, id: usize, call: &Expr, scope: &Scope) -> Expr {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = call
        else {
            return call.clone();
        };
        let placed = self.analyzer.placed(id, arguments, named_arguments);
        let open = !self.analyzer.applied(id, &placed, scope).is_empty();
        let steps = self.analyzer.facts.returns_step(id);
        let mut items = Vec::with_capacity(placed.len());
        for (index, argument) in &placed {
            items.push(match index {
                Some(index) if self.analyzer.facts.takes_step(id, *index) => {
                    Item::Ready(self.stepping(argument, scope))
                }
                _ => Item::Raw(argument),
            });
        }
        self.sequence(items, scope, |_, values| {
            let rebuilt = rebuild_call(function, named_arguments, values);
            match (steps, open) {
                (true, true) => rebuilt,
                (true, false) => done(build::call(RUN, vec![rebuilt])),
                (false, _) => done(rebuilt),
            }
        })
    }

    fn step_resume(&mut self, value: Option<&Expr>, scope: &Scope) -> Expr {
        let Some(name) = scope.resume_name.clone() else {
            self.reject("`resume` here has no captured continuation to call".to_owned());
            return done(Expr::Resume(value.cloned().map(Box::new)));
        };
        let items = value.map(Item::Raw).into_iter().collect();
        self.sequence(items, scope, |_, mut values| {
            if values.is_empty() {
                values.push(build::call(UNIT, Vec::new()));
            }
            build::apply(ident(&name), values)
        })
    }

    /// A block: each statement that captures holds the rest in its closure.
    pub(super) fn step_block(
        &mut self,
        statements: &[Stmt],
        value: Option<&Expr>,
        scope: &Scope,
    ) -> Expr {
        let Some((first, rest)) = statements.split_first() else {
            return match value {
                Some(value) => self.step(value, scope),
                None => done(build::call(UNIT, Vec::new())),
            };
        };
        match first {
            Stmt::Let {
                name, value: bound, ..
            } => self.step_let(first, name, bound, rest, value, scope),
            Stmt::Assignment { position, .. } => {
                self.at = position.or(self.at);
                self.reject(STATE.to_owned());
                self.step_block(rest, value, scope)
            }
            Stmt::Expr { value: effect, .. } if self.analyzer.open(effect, scope) => {
                let produced = self.step(effect, scope);
                let ignored = self.fresh.name("u");
                let tail = self.replayed(|this| this.step_block(rest, value, scope));
                self.fresh.bind(produced, ignored, tail)
            }
            other => {
                let mut kept = other.clone();
                if let (Stmt::Expr { value: slot, .. }, Stmt::Expr { value: plain, .. }) =
                    (&mut kept, other)
                {
                    *slot = self.direct(plain, scope);
                }
                let tail = self.step_block(rest, value, scope);
                block(vec![kept], tail)
            }
        }
    }

    fn step_let(
        &mut self,
        binding: &Stmt,
        name: &str,
        bound: &Expr,
        rest: &[Stmt],
        value: Option<&Expr>,
        scope: &Scope,
    ) -> Expr {
        let after = self.analyzer.bound(scope, name, bound);
        let mut kept = binding.clone();
        if !self.analyzer.open(bound, scope) {
            set_value(&mut kept, self.bound_value(bound, scope));
            let tail = self.step_block(rest, value, &after);
            return block(vec![kept], tail);
        }
        let produced = self.step(bound, scope);
        let temp = self.fresh.name("v");
        set_value(&mut kept, ident(&temp));
        // Everything after the capture lives in the closure a resumption
        // replays, so its channel operations are rejected, not duplicated.
        let tail = self.replayed(|this| this.step_block(rest, value, &after));
        self.fresh.bind(produced, temp, block(vec![kept], tail))
    }
}

fn set_value(binding: &mut Stmt, replacement: Expr) {
    if let Stmt::Let { value, .. } = binding {
        *value = replacement;
    }
}

/// Operators whose right side runs only for some left values.
fn is_lazy(op: &str) -> bool {
    matches!(op, "&&" | "||" | "?:")
}
