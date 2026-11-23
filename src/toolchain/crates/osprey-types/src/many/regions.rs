//! Handler regions.
//!
//! A handler of a captured effect becomes a call to that effect's answering
//! function: the body is a step, each arm is a closure taking the operation's
//! arguments and the continuation, and `resume(x)` is a call of that
//! continuation. A handler of a direct effect over a body that captures is
//! re-entered around every stretch of that body, so a resumed branch finds the
//! same handler it was captured under ([MULTI-HANDLE-MANY-LEXICAL]).

use super::build::{
    self, block, done, handle_name, ident, request_name, BIND, FINALLY, REINSTALL, RUN,
};
use super::facts::{Esc, Scope};
use super::rewrite::Rewriter;
use osprey_ast::{effect_name, Expr, HandlerArm, Stmt};

impl Rewriter<'_> {
    /// A handler where a plain value is expected.
    pub(super) fn direct_handler(&mut self, handler: &Expr, scope: &Scope) -> Expr {
        let Expr::Handler {
            effect,
            arms,
            body,
            return_clause,
            finally,
            position,
            ..
        } = handler
        else {
            return handler.clone();
        };
        self.at = position.or(self.at);
        if self.analyzer.facts.is_captured(effect) {
            self.must_be_closed(handler, scope, "this handler's region");
            let lowered = self.answered(handler, scope);
            return build::call(RUN, vec![lowered]);
        }
        self.must_be_closed(body, scope, "this handler's body");
        let mut out = handler.clone();
        if let Expr::Handler {
            arms: slots,
            body: slot,
            return_clause: returned,
            finally: cleanup,
            ..
        } = &mut out
        {
            self.direct_arms(slots, arms, scope);
            **slot = self.direct(body, scope);
            *returned = return_clause
                .as_deref()
                .map(|clause| Box::new(self.direct(clause, scope)));
            *cleanup = finally
                .as_deref()
                .map(|clause| Box::new(self.direct(clause, scope)));
        }
        out
    }

    /// Rewrite each arm's body where it yields a plain value: no arm's region
    /// is captured, so every arm keeps its direct form.
    fn direct_arms(&mut self, slots: &mut [HandlerArm], arms: &[HandlerArm], scope: &Scope) {
        for (target, arm) in slots.iter_mut().zip(arms) {
            target.body = self.direct(&arm.body, &scope.in_arm(arm, Esc::default(), false));
        }
    }

    /// A handler of a captured effect, as a step.
    pub(super) fn answered(&mut self, handler: &Expr, scope: &Scope) -> Expr {
        let Expr::Handler {
            effect,
            arms,
            return_clause,
            finally,
            body,
            position,
            ..
        } = handler
        else {
            return handler.clone();
        };
        self.at = position.or(self.at);
        let base = effect_name::base(effect).to_owned();
        let through = self.analyzer.through(handler, scope);
        let table = self.analyzer.table;
        let ops = table
            .effect(&base)
            .map_or(&[][..], |declared| &declared.ops);
        let mut arguments = vec![
            self.step(body, scope),
            self.returning(return_clause.as_deref(), scope),
        ];
        for op in ops {
            let written = arms.iter().find(|arm| arm.operation == op.name);
            arguments.push(match written {
                Some(arm) => self.arm(arm, op.control, &through, scope),
                None => self.passing(&base, &op.name, op.params.len()),
            });
        }
        let core = build::call(&handle_name(&base), arguments);
        self.finalized(core, finally.as_deref(), scope)
    }

    /// The closure applied to a branch's finished value.
    fn returning(&mut self, clause: Option<&Expr>, scope: &Scope) -> Expr {
        if let Some(Expr::Lambda {
            parameters, body, ..
        }) = clause
        {
            let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
            let rest = self.step(body, &scope.binding(names.iter().cloned()));
            return self.fresh.lambda(names, rest);
        }
        let value = self.fresh.name("r");
        let body = done(ident(&value));
        self.fresh.lambda(vec![value], body)
    }

    /// One arm: `fn(params.., resume) => step`.
    fn arm(&mut self, arm: &HandlerArm, control: bool, through: &Esc, scope: &Scope) -> Expr {
        self.at = arm.position.or(self.at);
        let resume = self.fresh.name("resume");
        let mut inner = scope.in_arm(arm, through.clone(), true);
        inner.resume_name = Some(resume.clone());
        let mut body = self.step(&arm.body, &inner);
        if !control {
            // A value arm's result is the operation's result: the performer
            // continues with it exactly once.
            let result = self.fresh.name("r");
            let continued = build::apply(ident(&resume), vec![ident(&result)]);
            body = self.fresh.bind(body, result, continued);
        }
        let mut params: Vec<String> = arm.params.iter().map(|name| self.binder(name)).collect();
        params.push(resume);
        self.fresh.lambda(params, body)
    }

    fn binder(&mut self, name: &str) -> String {
        if name == "_" {
            self.fresh.name("p")
        } else {
            name.to_owned()
        }
    }

    /// An operation this handler does not answer: ask the next handler out,
    /// then continue with its answer.
    fn passing(&mut self, effect: &str, operation: &str, arity: usize) -> Expr {
        let mut params: Vec<String> = (0..arity).map(|_| self.fresh.name("p")).collect();
        let request = build::call(
            &request_name(effect, operation),
            params.iter().map(|name| ident(name)).collect(),
        );
        let resume = self.fresh.name("resume");
        let body = build::call(BIND, vec![request, ident(&resume)]);
        params.push(resume);
        self.fresh.lambda(params, body)
    }

    /// `core` with `finally` run once per finished or abandoned branch
    /// ([EFFECTS-FINALIZATION]).
    fn finalized(&mut self, core: Expr, finally: Option<&Expr>, scope: &Scope) -> Expr {
        let Some(cleanup) = finally else { return core };
        if self.analyzer.open(cleanup, scope) {
            self.reject(
                "a `finally` clause makes a request a `many` handler captures; cleanup runs \
when a branch ends or is abandoned, where no continuation exists to capture"
                    .to_owned(),
            );
        }
        let statement = Stmt::Expr {
            value: self.direct(cleanup, scope),
            doc: None,
            position: Some(self.fresh.position()),
        };
        let run = self
            .fresh
            .lambda(Vec::new(), block(vec![statement], Expr::Integer(0)));
        build::call(FINALLY, vec![core, run])
    }

    /// A handler of a direct effect over a body that captures.
    pub(super) fn reinstalled(&mut self, handler: &Expr, scope: &Scope) -> Expr {
        let Expr::Handler {
            arms,
            return_clause,
            finally,
            body,
            position,
            ..
        } = handler
        else {
            return handler.clone();
        };
        self.at = position.or(self.at);
        let thunk = self.fresh.name("thunk");
        let mut installed = handler.clone();
        if let Expr::Handler {
            arms: slots,
            return_clause: returned,
            finally: cleanup,
            body: slot,
            ..
        } = &mut installed
        {
            self.direct_arms(slots, arms, scope);
            *returned = None;
            *cleanup = None;
            **slot = build::apply(ident(&thunk), Vec::new());
        }
        let install = self.fresh.lambda(vec![thunk], installed);
        let stretch = self.step(body, scope);
        let delayed = self.fresh.lambda(Vec::new(), stretch);
        let mut core = build::call(REINSTALL, vec![install, delayed]);
        if let Some(Expr::Lambda {
            parameters,
            body: rest,
            ..
        }) = return_clause.as_deref()
        {
            if let Some(parameter) = parameters.first() {
                let next = self.step(rest, &scope.binding([parameter.name.clone()]));
                core = self.fresh.bind(core, parameter.name.clone(), next);
            }
        }
        self.finalized(core, finally.as_deref(), scope)
    }
}
