//! Effect performs and handler expressions: operation lookup, arm typing
//! and the resume discipline of [EFFECTS-RESUME].
use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::names;
use crate::ty::Type;
use crate::unify::unify;
use osprey_ast::Expr;
use osprey_ast::NamedArgument;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

impl Checker {
    /// Infer a `perform`: resolve the effect's instantiation innermost-first
    /// against the enclosing handler/effect-row scopes (falling back to a
    /// fresh instantiation of the declaration), unify the arguments against
    /// the instantiated parameters, and yield the instantiated result type.
    /// The resolved signature is published per site for the code generator.
    pub(crate) fn infer_perform(
        &mut self,
        target: crate::handler_scope::Target<'_>,
        operation: &str,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        let arg_tys: Vec<Type> = arguments.iter().map(|a| self.infer_expr(a, env)).collect();
        for na in named_arguments {
            let _ = self.infer_expr(&na.value, env);
        }
        // Infer a second, annotation-independent instance from the operation
        // payload. A declared row is a contract, not authority: if
        // `!Stash<string>` surrounds `Stash.put(42)`, the row must not rewrite
        // the performed instance from `Stash<int>` to `Stash<string>` in the
        // later static discharge proof.
        let effect = target.effect;
        let actual_instance = target
            .instance
            .clone()
            .or_else(|| self.effect_instance_ops(effect));
        if !named_arguments.is_empty() {
            self.errors.push(TypeError::new(format!(
                "perform `{effect}.{operation}` does not support named arguments"
            )));
        }
        // A capability's instance is fixed by its type; otherwise a written
        // instantiation is the request's identity and an unwritten one is the
        // innermost enclosing handler's, else a fresh instance for inference.
        // Implements [EFFECTS-GENERIC-INSTANTIATION] and [EFFECTS-INSTANCE].
        let scope = target.instance.or_else(|| self.scoped_instance(effect));
        let Some((eff_args, ops)) = scope else {
            self.errors
                .push(TypeError::new(format!("unknown effect `{effect}`")));
            return self.ctx.fresh();
        };
        let Some(op) = ops.get(operation).cloned() else {
            self.errors.push(TypeError::new(format!(
                "effect `{effect}` has no operation `{operation}`"
            )));
            return self.ctx.fresh();
        };
        if !named_arguments.is_empty() {
            // Named arguments have already produced their specific diagnostic;
            // do not add a misleading positional-arity error.
        } else if op.params.len() == arg_tys.len() {
            for (p, a) in op.params.iter().zip(&arg_tys) {
                self.push_assign(p, a);
            }
        } else {
            self.errors.push(TypeError::new(format!(
                "effect operation `{effect}.{operation}` expects {} argument(s), got {}",
                op.params.len(),
                arg_tys.len()
            )));
        }
        if let Some(pos) = position {
            self.perform_tys.push((
                pos,
                crate::info::OpType {
                    params: op.params.clone(),
                    ret: op.ret.clone(),
                    mode: op.mode,
                },
                eff_args,
            ));
            if let Some((actual_args, actual_ops)) = actual_instance {
                if let Some(actual_op) = actual_ops.get(operation) {
                    if actual_op.params.len() == arg_tys.len() {
                        for (parameter, argument) in actual_op.params.iter().zip(&arg_tys) {
                            let _ = unify(&mut self.ctx, parameter, argument);
                        }
                    }
                }
                self.perform_actual_tys.push((pos, actual_args));
            }
        }
        op.ret
    }

    /// Infer a `handle`: instantiate the handled effect once for this site,
    /// type each arm body in a child scope whose params are the instantiated
    /// operation's parameter types, with the arm's `(op result, answer)`
    /// pushed so any `resume` inside types correctly. The handled body infers
    /// under this instantiation (innermost-first, matching the runtime's
    /// handler stack), so its `perform` sites pin the same type arguments.
    /// A return clause maps the body's A to the handler's B. Control arms and
    /// resumes answer B; value arms supply their operation's result R.
    /// Implements [EFFECTS-RESUME],
    /// [EFFECTS-GENERIC-INSTANTIATION], and [EFFECTS-OP-TYPING].
    pub(crate) fn infer_handler(
        &mut self,
        target: crate::handler_scope::Target<'_>,
        arms: &[osprey_ast::HandlerArm],
        body: &Expr,
        return_clause: Option<&Expr>,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        let effect = target.effect;
        let (eff_args, inst_ops, effect_known) = if let Some((args, ops)) =
            target.instance.or_else(|| self.effect_instance_ops(effect))
        {
            (args, ops, true)
        } else {
            self.errors
                .push(TypeError::new(format!("unknown effect `{effect}`")));
            (Vec::new(), HashMap::new(), false)
        };
        let answer = self.ctx.fresh();
        let mut answering: Vec<(String, Type)> = Vec::new();
        let mut seen_arms = HashSet::new();
        for arm in arms {
            if !seen_arms.insert(&arm.operation) {
                self.errors.push(
                    TypeError::new(format!(
                        "duplicate handler arm `{effect}.{}`",
                        arm.operation
                    ))
                    .with_pos(arm.position),
                );
            }
            let (params, op_ret, mode) = self.arm_signature(effect, arm, &inst_ops, effect_known);
            let mut local = env.child();
            for (p, pty) in arm.params.iter().zip(params) {
                local.insert(p.clone(), crate::ty::Scheme::mono(pty));
            }
            self.resume_ctx.push(if mode.is_control() {
                crate::check::ResumeSite::Control {
                    op_ret: op_ret.clone(),
                    answer: answer.clone(),
                }
            } else {
                crate::check::ResumeSite::Value {
                    effect: osprey_ast::effect_name::base(effect).to_owned(),
                    operation: arm.operation.clone(),
                }
            });
            let arm_ty = self.infer_expr(&arm.body, &local);
            let _ = self.resume_ctx.pop();
            // Mode comes from the OPERATION DECLARATION, never from searching
            // the arm for `resume`. A control arm's value is the handler's
            // ANSWER — `resume` supplies the operation's result and the arm
            // outlives it. A value arm's value SUBSTITUTES: it is what the
            // `perform` evaluates to, and what pins a generic effect's
            // instantiation from its handler. Adding an unreachable `resume`
            // to an arm cannot change which of the two it is.
            // Implements [EFFECTS-HANDLER-ARMS], [EFFECTS-RESUME] and
            // [EFFECTS-GENERIC-INSTANTIATION].
            if mode.is_control() {
                answering.push((arm.operation.clone(), arm_ty));
            } else {
                self.check_value_arm(&op_ret, &arm_ty);
            }
        }
        self.handler_scopes.push(crate::check::EffectScope {
            name: osprey_ast::effect_name::base(effect).to_string(),
            args: eff_args.clone(),
            ops: inst_ops.clone(),
        });
        let body_ty = self.infer_expr(body, env);
        let _ = self.handler_scopes.pop();
        if let Some(pos) = position {
            self.handler_tys.push((pos, eff_args, inst_ops));
        }
        if let Some(clause) = return_clause {
            let outer_resume = std::mem::take(&mut self.resume_ctx);
            let transform = self.infer_expr(clause, env);
            self.resume_ctx = outer_resume;
            self.push_assign(&Type::fun(vec![body_ty], answer.clone()), &transform);
        } else {
            self.push_assign(&answer, &body_ty);
        }
        // Checked only now: the handled expression is what pins the answer, so
        // blaming an arm before it is known would report the mismatch backwards
        // — the first resuming arm would simply BIND the still-free answer and
        // the body would then fail with an anonymous "cannot unify".
        for (op, arm_ty) in answering {
            self.check_resuming_arm_answer(effect, &op, &answer, &arm_ty);
        }
        answer
    }

    /// The instantiated parameter types, result type and declared mode of the
    /// operation `arm` answers, reporting an unknown operation or a parameter
    /// count that disagrees with the declaration. Implements
    /// [EFFECTS-OP-TYPING].
    fn arm_signature(
        &mut self,
        effect: &str,
        arm: &osprey_ast::HandlerArm,
        inst_ops: &HashMap<String, crate::info::OpType>,
        effect_known: bool,
    ) -> (Vec<Type>, Type, osprey_ast::OperationMode) {
        let fresh: Vec<Type> = (0..arm.params.len()).map(|_| self.ctx.fresh()).collect();
        let Some(op) = inst_ops.get(&arm.operation) else {
            if effect_known {
                self.errors.push(TypeError::new(format!(
                    "effect `{effect}` has no operation `{}`",
                    arm.operation
                )));
            }
            return (
                fresh,
                self.ctx.fresh(),
                osprey_ast::OperationMode::default(),
            );
        };
        if op.params.len() != arm.params.len() {
            self.errors.push(TypeError::new(format!(
                "handler operation `{effect}.{}` expects {} parameter(s), got {}",
                arm.operation,
                op.params.len(),
                arm.params.len()
            )));
            return (fresh, op.ret.clone(), op.mode);
        }
        (op.params.clone(), op.ret.clone(), op.mode)
    }

    /// A value arm supplies the operation's RESULT, so its value must be
    /// assignable to that result type. A `Unit` operation discards the value,
    /// which is the one place anything goes — except an unhandled `Result`,
    /// whose failure would vanish silently. Implements [EFFECTS-HANDLER-ARMS].
    fn check_value_arm(&mut self, op_ret: &Type, arm_ty: &Type) {
        if !self.ctx.expose(op_ret).is_named(names::UNIT) {
            self.push_assign(op_ret, arm_ty);
        } else if self.ctx.expose(arm_ty).is_named(names::RESULT) {
            self.errors.push(TypeError::new(
                "an unhandled `Result` cannot be discarded by a Unit effect operation arm; use `match` or `?:`",
            ));
        }
    }

    /// A resuming arm's value is the handler's ANSWER, not the operation's
    /// result: `resume` already supplied the result, and the arm keeps running
    /// afterwards, so what it finally returns answers for the whole `handle` —
    /// as does any branch of it that returns without resuming, abandoning the
    /// continuation. Its value must therefore be able to BE that answer — an
    /// assignment, hence the directional check: a concrete arm erases into an
    /// `any` region, while an erased arm cannot answer a concrete one
    /// ([TYPE-ANY]). Codegen cannot tell an erased word from a genuine `int`,
    /// which is why this belongs in inference. Implements
    /// [EFFECTS-HANDLER-ARMS].
    fn check_resuming_arm_answer(&mut self, effect: &str, op: &str, answer: &Type, arm_ty: &Type) {
        if crate::unify::unify_assignable(&mut self.ctx, answer, arm_ty).is_ok() {
            return;
        }
        let (want, got) = (self.ctx.prune(answer), self.ctx.prune(arm_ty));
        self.errors.push(TypeError::new(format!(
            "handler arm `{effect}.{op}` is a control arm, so its value becomes the whole \
             `handle` expression's result — but it is `{got}` and that result is `{want}`. \
             Make the arm's value agree with the handled expression's type"
        )));
    }

    pub(super) fn lookup_ident(&mut self, name: &str, env: &TypeEnv) -> Type {
        self.lookup_ident_at(name, env, None)
    }

    pub(super) fn lookup_ident_at(
        &mut self,
        name: &str,
        env: &TypeEnv,
        site: Option<&Expr>,
    ) -> Type {
        // A bare nullary constructor (`Red`, `Empty`) is a value of its owner type.
        if self.ctors.get(name).is_some_and(|i| i.fields.is_empty()) {
            if let Some((args, _f, owner, is_record)) = self.ctor_instance(name) {
                return if is_record {
                    Type::Record {
                        name: owner,
                        fields: BTreeMap::new(),
                    }
                } else {
                    Type::con(owner, args)
                };
            }
        }
        if let Some(applied) = env.applied(&mut self.ctx, name) {
            // Re-state the scheme's built-in obligations against this site's
            // fresh variables, so a generalized wrapper's constraint is checked
            // against the types this call actually supplies
            // ([`crate::ty::Scheme::obligations`]). A key with no location of
            // its own is located HERE, so its diagnostic names this use.
            let obligations = self.located_obligations(applied.obligations, self.expr_position);
            self.builtin_uses.extend(obligations);
            if let Some(site) = site {
                let _ = self
                    .instantiations
                    .insert(std::ptr::from_ref(site).addr(), applied.bindings);
            }
            return applied.ty;
        }
        // Preserve the physical helper's owner after assembly [LSP-DIAGNOSTICS].
        self.errors.push(
            TypeError::new(format!("unknown identifier `{name}`")).with_pos(self.expr_position),
        );
        self.ctx.fresh()
    }
}
