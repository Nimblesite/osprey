//! Type checking: validation.
use super::{names, type_name_to_type, Checker, CtorInstance, HashMap, Type, TypeError};

impl Checker {
    /// Reject concrete receiver/value types that their built-in runtime cannot
    /// interpret. Unresolved generalized variables remain the one inference
    /// limitation: Osprey has no type-class constraint to preserve the
    /// obligation in a polymorphic scheme.
    pub(super) fn validate_builtin_uses(&mut self) {
        let uses = std::mem::take(&mut self.builtin_uses);
        for (name, ty) in uses {
            let resolved = self.ctx.apply(&ty);
            let resolved = match self.use_sites.get(&name) {
                Some(site) => self.ctx.exposed_at(resolved, site),
                None => resolved,
            };
            if let Some(message) = crate::builtin_constraints::invalid_use(&name, &resolved) {
                self.errors.push(
                    TypeError::new(message)
                        .with_pos(crate::builtin_constraints::source_position(&name)),
                );
            }
        }
    }

    /// Reject every statement whose value is thrown away. A statement runs for
    /// its effects, so `Unit` is the only type it may have; anything else is
    /// dead computation, and most often a juxtaposition the reader took for a
    /// call. A type variable that never resolved is left alone — nothing
    /// proves it is not `Unit`. Implements [BLOCK-DISCARD].
    pub(super) fn validate_discards(&mut self) {
        let discards = std::mem::take(&mut self.discards);
        for d in discards {
            let resolved = self.ctx.apply(&d.ty);
            if let Some(message) = discard_error(&resolved, d.explicit) {
                self.record_err(TypeError::new(message), d.position);
            }
        }
    }

    /// Settle every arithmetic overload left open by
    /// [`Checker::deferred_arith`], now that nothing further can constrain an
    /// operand. Resolving one site can constrain another's operand — `fn twice(x)
    /// = x + x` used by `fn quad(y) = twice(y) + twice(y)` — so this repeats
    /// until a pass changes nothing, which it must: each pass either resolves a
    /// site or leaves the substitution untouched.
    pub(super) fn resolve_deferred_arithmetic(&mut self) {
        self.defer_arith = false;
        for _ in 0..self.builtin_uses.len().saturating_add(1) {
            let before = self.ctx.bound_count();
            self.resolve_field_uses();
            let uses = self.builtin_uses.clone();
            for (name, ty) in &uses {
                self.resolve_deferred_arith(name, ty);
            }
            if self.ctx.bound_count() == before {
                return;
            }
        }
    }

    /// Build the instantiated field types of a constructor against fresh type
    /// arguments. Returns (per-type-param fresh var, declared field map).
    pub(crate) fn ctor_instance(&mut self, name: &str) -> Option<CtorInstance> {
        let info = self.ctors.get(name)?;
        let owner = info.owner.clone();
        let is_record = info.owner_is_record;
        let params = info.type_params.clone();
        let raw_fields = info.fields.clone();
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        for p in &params {
            let v = self.ctx.fresh();
            let _ = pmap.insert(p.clone(), v.clone());
            args.push(v);
        }
        let fields = raw_fields
            .iter()
            .map(|(fname, fty)| (fname.clone(), type_name_to_type(fty, &pmap)))
            .collect();
        Some((args, fields, owner, is_record))
    }
}
/// The diagnostic for discarding a value of type `ty`, or `None` when
/// discarding it is legal. `explicit` marks a `let _ =`, which the author wrote
/// on purpose and so needs no advice about binding it.
/// Implements [BLOCK-DISCARD], [ERROR-RESULT-DISCARD].
pub(super) fn discard_error(ty: &Type, explicit: bool) -> Option<String> {
    match ty {
        // Never constrained, so nothing here proves it is not `Unit`.
        Type::Var(_) => None,
        Type::Con { name, .. } if name == names::UNIT => None,
        // A `Result` is refused either way: dropping it drops the error channel,
        // which is the one thing the type exists to make visible, and `_` cannot
        // consent to that on the caller's behalf.
        Type::Con { name, .. } if name == names::RESULT => {
            Some("an unhandled `Result` cannot be discarded; use `match` or `?:`".to_string())
        }
        _ if explicit => None,
        other => Some(format!(
            "a `{other}` value cannot be discarded; bind it with `let _ =` \
             if that is deliberate, or remove the statement"
        )),
    }
}
