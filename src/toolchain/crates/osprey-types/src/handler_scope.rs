//! Handler scope forms the checker types beside the arms: a `finally` cleanup
//! runs outside its own activation and returns `Unit`; `mask` hides one
//! activation; `instance` creates a capability that effect forms can name.
//! Implements [EFFECTS-FINALIZATION], [EFFECTS-MASK] and [EFFECTS-INSTANCE]
//! (docs/specs/language/0017-AlgebraicEffects.md).

use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::info::OpType;
use crate::ty::{names, Type};
use osprey_ast::{Expr, Position};
use std::collections::HashMap;

impl Checker {
    /// Dispatch the effect expression forms (split out of [`Self::infer_expr`]
    /// to keep its match within budget). Each resolves its target first: an
    /// effect declaration or a capability ([EFFECTS-INSTANCE]).
    pub(crate) fn infer_effect_expr(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        match e {
            Expr::Perform {
                effect,
                operation,
                arguments,
                named_arguments,
                position,
            } => match self.effect_target(effect, *position, env) {
                Some(target) => self.infer_perform(
                    target,
                    operation,
                    arguments,
                    named_arguments,
                    *position,
                    env,
                ),
                None => self.ctx.fresh(),
            },
            Expr::Mask {
                effect,
                body,
                position,
            } => self.infer_mask(effect, body, *position, env),
            Expr::Handler { .. } => self.infer_handler_form(e, env),
            other => self.infer_expr(other, env),
        }
    }

    /// A handler: its target, then its arms and body, then its cleanup. A
    /// compile-time handler of a capability never reaches here: the stage
    /// validator rejects it first, since it names no effect declaration.
    fn infer_handler_form(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        let Expr::Handler {
            effect,
            arms,
            return_clause,
            finally,
            body,
            position,
            ..
        } = e
        else {
            return self.ctx.fresh();
        };
        let Some(target) = self.effect_target(effect, *position, env) else {
            return self.ctx.fresh();
        };
        let answer =
            self.infer_handler(target, arms, body, return_clause.as_deref(), *position, env);
        if let Some(cleanup) = finally {
            self.infer_cleanup(cleanup, *position, env);
        }
        answer
    }

    /// Type `cleanup` in the handler's enclosing environment. It cannot resume
    /// the dropped continuation, so no enclosing arm's `resume` is visible to
    /// it, and it cannot replace the handler's answer, so it must be `Unit`.
    pub(crate) fn infer_cleanup(
        &mut self,
        cleanup: &Expr,
        position: Option<Position>,
        env: &TypeEnv,
    ) {
        let outer_resume = std::mem::take(&mut self.resume_ctx);
        let ty = self.infer_expr(cleanup, env);
        self.resume_ctx = outer_resume;
        if crate::unify::unify(&mut self.ctx, &Type::unit(), &ty).is_err() {
            let found = self.ctx.prune(&ty);
            self.errors.push(
                TypeError::new(format!(
                    "a handler's `finally` clause must be `Unit`, found `{found}`: \
cleanup runs after the answer is decided and cannot replace it"
                ))
                .with_pos(position),
            );
        }
    }
}

/// An effect's type arguments and its operations instantiated at them.
pub(crate) type EffectInstance = (Vec<Type>, HashMap<String, OpType>);

/// What a `perform`, `handler` or `mask` names: an effect declaration, or a
/// capability whose effect instance is already fixed by its type.
pub(crate) struct Target<'a> {
    /// The name as written: an effect spelling or a capability binding.
    pub(crate) effect: &'a str,
    /// The capability's instance; `None` for an effect declaration.
    pub(crate) instance: Option<EffectInstance>,
}

impl Checker {
    /// `instance E` is a fresh capability identity typed
    /// `Capability<E, #line:col>`: the site makes two unrelated instances
    /// distinct types, so they never unify. Implements [EFFECTS-INSTANCE].
    pub(crate) fn infer_instance(&mut self, effect: &str, position: Option<Position>) -> Type {
        let Some((args, _)) = self.effect_instance_ops(effect) else {
            self.errors
                .push(TypeError::new(format!("unknown effect `{effect}`")).with_pos(position));
            return self.ctx.fresh();
        };
        let base = osprey_ast::effect_name::base(effect);
        let identity = position.map_or_else(
            || String::from("#?"),
            |at| format!("#{}:{}", at.line, at.column),
        );
        Type::con(
            names::CAPABILITY,
            vec![Type::con(base, args), Type::con(identity, Vec::new())],
        )
    }

    /// Resolve the target of an effect form. A local binding names a
    /// capability; anything else names an effect declaration. `None` means
    /// the binding is not a capability, which has been reported.
    pub(crate) fn effect_target<'a>(
        &mut self,
        name: &'a str,
        position: Option<Position>,
        env: &TypeEnv,
    ) -> Option<Target<'a>> {
        let Some(scheme) = env.get(name) else {
            return Some(Target {
                effect: name,
                instance: None,
            });
        };
        let ty = crate::env::instantiate(&mut self.ctx, &scheme.clone());
        let resolved = self.ctx.apply(&ty);
        let Some((effect, args, identity)) = capability_parts(&resolved) else {
            self.errors.push(
                TypeError::new(format!(
                    "`{name}` is not a capability: its type is `{resolved}`; an effect form \
names an effect or a value created by `instance`"
                ))
                .with_pos(position),
            );
            return None;
        };
        if let Some(at) = position {
            self.capability_sites
                .push((at, effect.clone(), identity.clone()));
        }
        Some(Target {
            effect: name,
            instance: Some(self.effect_ops_at(&effect, args)),
        })
    }

    /// The operations of `effect` instantiated at `args`.
    fn effect_ops_at(&self, effect: &str, args: Vec<Type>) -> EffectInstance {
        let ops = self.effects.get(effect).map_or_else(HashMap::new, |info| {
            let pmap = info.type_params.iter().cloned().zip(args.iter().cloned());
            Self::instantiate_ops(info, &pmap.collect())
        });
        (args, ops)
    }

    /// `mask E { body }` answers its body's value. The masked effect's
    /// instantiation is published like a handler's, from a written
    /// instantiation, a capability, or the innermost enclosing handler.
    /// Implements [EFFECTS-MASK].
    pub(crate) fn infer_mask(
        &mut self,
        effect: &str,
        body: &Expr,
        position: Option<Position>,
        env: &TypeEnv,
    ) -> Type {
        if let Some(target) = self.effect_target(effect, position, env) {
            match target.instance.or_else(|| self.scoped_instance(effect)) {
                Some((args, ops)) => {
                    if let Some(at) = position {
                        self.handler_tys.push((at, args, ops));
                    }
                }
                None => self
                    .errors
                    .push(TypeError::new(format!("unknown effect `{effect}`")).with_pos(position)),
            }
        }
        self.infer_expr(body, env)
    }

    /// An unwritten instantiation is the innermost enclosing handler's, else a
    /// fresh one; a written one is its own. Implements
    /// [EFFECTS-GENERIC-INSTANTIATION].
    pub(crate) fn scoped_instance(&mut self, effect: &str) -> Option<EffectInstance> {
        let base = osprey_ast::effect_name::base(effect);
        self.handler_scopes
            .iter()
            .rev()
            .find(|scope| scope.name == base)
            .filter(|_| osprey_ast::effect_name::instantiation(effect).is_none())
            .map(|scope| (scope.args.clone(), scope.ops.clone()))
            .or_else(|| self.effect_instance_ops(effect))
    }
}

/// `Capability<E<args>, #site>` as (E, args, #site).
fn capability_parts(ty: &Type) -> Option<(String, Vec<Type>, String)> {
    let Type::Con { name, args } = ty else {
        return None;
    };
    match (name.as_str(), args.as_slice()) {
        (
            names::CAPABILITY,
            [Type::Con {
                name: effect,
                args: effect_args,
            }, Type::Con { name: identity, .. }],
        ) => Some((effect.clone(), effect_args.clone(), identity.clone())),
        _ => None,
    }
}
