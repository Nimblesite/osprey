//! Type checking: obligations.
use super::{generalize, BTreeSet, Checker, Scheme, SplitObligations, Type, TypeEnv, VarId};

impl Checker {
    /// Generalize `ty`, carrying every built-in obligation recorded on a
    /// variable this scheme quantifies ([`Scheme::obligations`]).
    ///
    /// An obligation on a variable that generalizes cannot be discharged here —
    /// the body never says what the type is — but it is not vacuous either: it
    /// binds at every call site. Keeping it in the flat list as well would
    /// report the wrapper itself, which is not an error, so the obligation
    /// MOVES into the scheme.
    pub(super) fn generalize_with_obligations(&mut self, env: &TypeEnv, ty: &Type) -> Scheme {
        self.resolve_field_uses();
        let mut scheme = generalize(&mut self.ctx, env, ty);
        if scheme.vars.is_empty() {
            return scheme;
        }
        self.generalize_connected_obligations(env, &mut scheme.vars);
        let (deferred, kept) = self.split_obligations(&scheme.vars);
        // Quantifying a variable that still carries a pending arithmetic
        // overload would hand every call site its own fresh copy, so no use
        // could ever inform the choice and the definition would default to
        // integer with nothing having looked at it. Keeping it monomorphic is
        // what lets `gpuFold(0.0, plus)` type `plus` at float.
        let pinned = self.arith_operand_vars(&kept);
        scheme.vars.retain(|v| !pinned.contains(v));
        self.builtin_uses = kept;
        scheme.obligations = deferred;
        scheme
    }

    /// Field relations can mention callback variables absent from the visible
    /// function signature. Quantify their entire connected component together.
    pub(super) fn generalize_connected_obligations(
        &mut self,
        env: &TypeEnv,
        vars: &mut Vec<VarId>,
    ) {
        let external = env.free_vars(&mut self.ctx);
        let mut connected: BTreeSet<_> = vars.iter().copied().collect();
        loop {
            let before = connected.len();
            for (_, ty) in &self.builtin_uses {
                let mut free = BTreeSet::new();
                self.ctx.free_vars(ty, &mut free);
                if !free.is_disjoint(&connected) {
                    connected.extend(free.difference(&external));
                }
            }
            if connected.len() == before {
                *vars = connected.into_iter().collect();
                return;
            }
        }
    }

    /// Every type variable a pending arithmetic overload rests on.
    pub(super) fn arith_operand_vars(&mut self, obligations: &[(String, Type)]) -> BTreeSet<VarId> {
        let mut vars = BTreeSet::new();
        for (name, ty) in obligations {
            if crate::expr::is_deferred_arith(name) {
                self.ctx.free_vars(ty, &mut vars);
            }
        }
        vars
    }

    /// Split the recorded built-in uses into those resting on `vars` (deferred
    /// to each instantiation) and those that stay for [`Self::validate_builtin_uses`].
    pub(super) fn split_obligations(&mut self, vars: &[VarId]) -> SplitObligations {
        let uses = std::mem::take(&mut self.builtin_uses);
        let resolved: Vec<(String, Type)> = uses
            .into_iter()
            .map(|(name, ty)| {
                let ty = self.ctx.apply(&ty);
                (name, ty)
            })
            .collect();
        let mut deferred = Vec::new();
        let mut kept = Vec::new();
        for (name, ty) in resolved {
            // A pending arithmetic overload never travels into a scheme: it is
            // settled once for the whole program
            // ([`Checker::deferred_arith`]), and the variables it rests on stay
            // monomorphic so every use of the definition constrains the SAME
            // choice.
            if crate::expr::is_deferred_arith(&name)
                || crate::builtin_constraints::fixed_numeric_operand(&name, &ty)
                || !self.mentions_any(&ty, vars)
            {
                kept.push((name, ty));
            } else {
                deferred.push((name, ty));
            }
        }
        (deferred, kept)
    }

    /// Whether `ty` still rests on one of the quantified variables.
    pub(super) fn mentions_any(&mut self, ty: &Type, vars: &[VarId]) -> bool {
        let mut free = BTreeSet::new();
        self.ctx.free_vars(ty, &mut free);
        vars.iter().any(|v| free.contains(v))
    }
}
