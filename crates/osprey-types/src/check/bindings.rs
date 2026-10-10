//! Type checking: bindings.
use super::{
    annotation_name, names, unify_assignable, BTreeSet, Checker, Discard, Expr, Position, Type,
    TypeEnv, TypeError, TypeExpr, VarId, DISCARD_BINDING,
};

impl Checker {
    pub(super) fn check_let(
        &mut self,
        name: &str,
        mutable: bool,
        ty: Option<&TypeExpr>,
        value: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let value_ty = self.infer_expr(value, env);
        if name == DISCARD_BINDING {
            // `let _ = e` is a deliberate discard, so it answers to the same
            // rule a bare statement does — which lets an ordinary value through
            // and still refuses a `Result`, whose error channel `_` cannot
            // consent to losing. Implements [BLOCK-DISCARD], [ERROR-RESULT-DISCARD].
            self.discards.push(Discard::explicit(value_ty.clone(), pos));
        }
        let binding_ty = if let Some(te) = ty {
            let binder = self.current_fn_typarams.clone();
            let annotated = self.annotation_type(te, &binder, te.position.or(pos));
            self.unify_or_err(
                &annotated,
                &value_ty,
                &format!("let `{}`", annotation_name(name)),
                pos,
            );
            annotated
        } else {
            value_ty.clone()
        };
        // Publish the binding's inferred type for editor hover, keyed by source
        // position (resolved against the final substitution in `infer_program`).
        // Implements [LSP-HOVER-VARIABLES]
        if let Some(p) = pos {
            self.let_tys.push((p, binding_ty.clone()));
        }
        // A let-bound value carries obligations exactly as a named function
        // does: quantifying a variable that still holds a pending arithmetic
        // overload would hand every use its own fresh copy, so nothing could
        // inform the choice and `let join = |a, b| => a + b` used only on
        // strings defaulted to the CHECKED INTEGER overload — `join("a", "b")`
        // printed `Success(<pointer>)` instead of `ab`
        // ([`Checker::deferred_arith`]).
        let mut scheme = self.generalize_with_obligations(env, &binding_ty);
        let mut pinned = self.stateful_handle_vars(&binding_ty);
        if let Expr::Call { function, .. } = value {
            if let Expr::Identifier(callee) = function.as_ref() {
                if self.handler_factories.contains(callee) {
                    self.ctx.free_vars(&binding_ty, &mut pinned);
                }
            }
        }
        scheme.vars.retain(|v| !pinned.contains(v));
        let _ = self
            .scheme_obligations
            .insert(format!("let {pos:?} {name}"), scheme.obligations.clone());
        if mutable {
            env.insert_mutable(name, scheme);
        } else {
            env.insert(name, scheme);
        }
    }

    /// Every type variable sitting inside a STATEFUL HANDLE VALUE in `ty` — the
    /// ML value restriction, narrowed to the two constructors that need it.
    ///
    /// A `let` generalizes, so quantifying such a variable hands every USE of
    /// the one handle its own copy, and nothing ties them together:
    ///
    /// ```text
    /// let ch = Channel(1)
    /// send(ch, "text")
    /// print("${recv(ch) + 1}")   // accepted; printed a raw pointer as an int
    /// ```
    ///
    /// A channel's element type is fixed at run time by what was sent, and a
    /// fiber's by the thunk that produced it; neither is a fresh value per use
    /// the way an immutable `List<t>` is. The same hole is why a
    /// `Channel<List<List<int>>>` was received back as `List<int>` and every
    /// nested read answered its fallback.
    /// Implements [CONCURRENCY-CHANNEL] and [CONCURRENCY-SPAWN-AWAIT].
    pub(super) fn stateful_handle_vars(&mut self, ty: &Type) -> BTreeSet<VarId> {
        let mut pinned = BTreeSet::new();
        let applied = self.ctx.apply(ty);
        self.collect_handle_vars(&applied, &mut pinned);
        pinned
    }

    pub(super) fn collect_handle_vars(&mut self, ty: &Type, out: &mut BTreeSet<VarId>) {
        match ty {
            Type::Con { name, args } if name == names::CHANNEL || name == names::FIBER => {
                for arg in args {
                    self.ctx.free_vars(arg, out);
                }
            }
            Type::Con { args, .. } => {
                for arg in args {
                    self.collect_handle_vars(arg, out);
                }
            }
            // NOT into a function. A function's type variables are instantiated
            // fresh at every call, so a factory or helper that merely mentions a
            // handle — `let make = |n| => Channel(n)`, or a generic drain
            // helper — hands each call its own and stays soundly polymorphic.
            // Pinning those would reject safe code to fix an unsafe case that
            // only arises when one handle VALUE is shared between uses.
            Type::Record { fields, .. } => {
                for field in fields.values() {
                    self.collect_handle_vars(field, out);
                }
            }
            _ => {}
        }
    }

    /// Unify `expected` against `actual`, recording a positioned error prefixed
    /// with `label` (e.g. `` "let `x`" ``) when they don't match.
    pub(super) fn unify_or_err(
        &mut self,
        expected: &Type,
        actual: &Type,
        label: &str,
        pos: Option<Position>,
    ) {
        if let Err(e) = unify_assignable(&mut self.ctx, expected, actual) {
            self.record_err(TypeError::new(format!("{label}: {}", e.message)), pos);
        }
    }

    pub(super) fn check_assignment(
        &mut self,
        name: &str,
        value: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let value_ty = self.infer_expr(value, env);
        match env.get(name).cloned() {
            Some(scheme) => {
                if !env.is_mutable(name) {
                    self.record_err(
                        TypeError::new(format!("cannot assign to immutable variable `{name}`")),
                        pos,
                    );
                } else if self.resume_ctx.is_empty() && !self.source_contracts_validated {
                    // Handler arms are the language's mutation boundary. The
                    // resume context is present only while an arm body is
                    // being checked and is deliberately cleared across lambda
                    // boundaries, matching where handler-owned state may be
                    // changed at runtime.
                    self.record_err(
                        TypeError::new(
                            "state mutation is only allowed inside an effect handler arm",
                        ),
                        pos,
                    );
                }
                let existing = crate::env::instantiate(&mut self.ctx, &scheme);
                self.unify_or_err(
                    &existing,
                    &value_ty,
                    &format!("assignment to `{name}`"),
                    pos,
                );
            }
            None => self.record_err(
                TypeError::new(format!("assignment to undeclared `{name}`")),
                pos,
            ),
        }
    }
}
pub(super) fn returns_expansive_handler_value(body: &Expr) -> bool {
    match body {
        Expr::Block {
            value: Some(value), ..
        } => returned_handler_branch(value),
        _ => false,
    }
}
pub(super) fn returned_handler_branch(value: &Expr) -> bool {
    match value {
        Expr::Lambda { body, .. } => matches!(body.as_ref(), Expr::Handler { .. }),
        Expr::Match { arms, .. } => arms.iter().any(|arm| returned_handler_branch(&arm.body)),
        Expr::Block {
            value: Some(value), ..
        } => returned_handler_branch(value),
        _ => false,
    }
}
