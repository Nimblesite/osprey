//! Constructor and `Result` variant patterns: owner unification and binder aliases.

use super::*;

impl Checker {
    /// When `name` looks like a variant (capitalised) but the discriminant is a
    /// known union that has no such variant, return the union's name — a
    /// lower-case identifier is an ordinary catch-all binding instead.
    pub(super) fn unknown_variant_owner(&mut self, name: &str, disc: &Type) -> Option<String> {
        if !starts_uppercase(name) {
            return None;
        }
        match self.ctx.expose(disc) {
            Type::Con { name: owner, .. } => {
                let variants = self.union_variants.get(&owner)?;
                if variants.iter().any(|v| v == name) {
                    None
                } else {
                    Some(owner)
                }
            }
            _ => None,
        }
    }

    pub(super) fn bind_constructor(
        &mut self,
        name: &str,
        fields: &[String],
        sub_patterns: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        if !fields.is_empty() && !sub_patterns.is_empty() {
            self.bind_named_aliases(name, fields, sub_patterns, disc, local);
            return;
        }
        // `Success { value }` / `Error { message }` over a real `Result` always
        // bind the built-in variant's fields, even when a user union shadows
        // those constructor names.
        if is_result_variant(name) && is_result(&self.ctx.expose(disc)) {
            self.bind_result_fields(name, fields, disc, local);
            return;
        }
        let Some((args, declared, owner, is_record)) = self.ctor_instance(name) else {
            self.errors.push(TypeError::new(format!(
                "unknown constructor `{name}` in match pattern"
            )));
            for f in fields {
                let fv = self.ctx.fresh();
                local.insert(f.clone(), Scheme::mono(fv));
            }
            return;
        };
        // `Result` patterns (`Success`/`Error`) auto-wrap a non-Result
        // discriminant: `match a + b { Success { value } => .. }` over a `string`
        // binds `value : string` (the match auto-wrap rule: any value may be
        // matched as if wrapped in `Success`). This also lets validated record
        // constructions be matched without a real Result.
        if owner == names::RESULT {
            self.reject_plain_result_default(fields, disc);
            self.bind_result_fields(name, fields, disc, local);
            return;
        }
        let declared_map: BTreeMap<String, Type> = declared.iter().cloned().collect();
        // Tie the discriminant's type arguments to this constructor's owner.
        let owner_ty = ctor_owner_ty(owner, args, declared_map.clone(), is_record);
        self.push_unify(&owner_ty, disc);

        // Named field destructure: `Ctor { a, b }`.
        for fname in fields {
            let ft = declared_map
                .get(fname)
                .cloned()
                .unwrap_or_else(|| self.ctx.fresh());
            local.insert(fname.clone(), Scheme::mono(ft));
        }
        // Positional destructure: `Ctor(p0, p1)`.
        for (i, sub) in sub_patterns.iter().enumerate() {
            self.reject_nested_pattern(sub);
            let field_ty = declared.get(i).map(|(_, t)| t.clone());
            let target = field_ty.unwrap_or_else(|| self.ctx.fresh());
            self.bind_pattern(sub, &target, local);
        }
    }

    /// Named field labels select payloads; only written binders enter scope.
    fn bind_named_aliases(
        &mut self,
        name: &str,
        fields: &[String],
        patterns: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        if is_result_variant(name)
            && (is_result(&self.ctx.expose(disc))
                || self
                    .ctors
                    .get(name)
                    .is_some_and(|ctor| ctor.owner == names::RESULT))
        {
            let expected = if name == names::SUCCESS {
                "value"
            } else {
                "message"
            };
            if fields != [expected] {
                self.errors.push(TypeError::new(format!(
                    "expected pattern field `{expected}` on `{name}`"
                )));
            }
            let binders = patterns
                .iter()
                .flat_map(pattern_binder_names)
                .collect::<Vec<_>>();
            self.bind_result_fields(name, &binders, disc, local);
            return;
        }
        let Some((args, declared, owner, record)) = self.ctor_instance(name) else {
            self.errors.push(TypeError::new(format!(
                "unknown constructor `{name}` in match pattern"
            )));
            return;
        };
        let owner_ty = ctor_owner_ty(owner, args, declared.iter().cloned().collect(), record);
        self.push_unify(&owner_ty, disc);
        for (label, pattern) in fields.iter().zip(patterns) {
            match declared.iter().find(|(field, _)| field == label) {
                Some((_, ty)) => self.bind_pattern(pattern, ty, local),
                None => self.errors.push(TypeError::new(format!(
                    "unknown pattern field `{label}` on `{name}`"
                ))),
            }
        }
    }

    /// A positional slot destructures to a binder or `_` and nothing else.
    /// Codegen binds a slot by index and has no register to match a deeper
    /// pattern against, so an unsupported sub-pattern would be silently
    /// discarded and the arm would behave as `Ctor(_, _)`. The ML flavor
    /// rejects the same shape in its parser ([FLAVOR-ML-PATTERN-GROUP]); this
    /// is the Default flavor's half of that rule.
    fn reject_nested_pattern(&mut self, sub: &Pattern) {
        if matches!(sub, Pattern::Binding(_) | Pattern::Wildcard) {
            return;
        }
        self.errors.push(TypeError::new(
            "nested constructor patterns are not supported; \
             bind the payload and match it in a second expression"
                .to_owned(),
        ));
    }

    /// Bind the built-in `Result` pattern fields against `disc`: `value` is the
    /// success payload (the discriminant itself when it is not a `Result` — the
    /// match auto-wrap rule), `message` is the error string.
    /// `?:` is an explicit Result handling operation, so it does NOT get the
    /// ordinary `Success`-arm auto-wrap: [PATTERN-RESULT-DEFAULT] states that
    /// its scrutinee must be a `Result` and that it "never reinterprets a plain
    /// value as `Success`". Inheriting auto-wrap made `5 ?: -1` well-typed with
    /// an unreachable fallback. Only the desugarer's unspellable payload binder
    /// distinguishes the two, since both are a `Success`/`Error` match.
    ///
    /// An unresolved scrutinee is left alone: this checker unifies eagerly, so a
    /// type variable here may still become a `Result` later.
    fn reject_plain_result_default(&mut self, fields: &[String], disc: &Type) {
        if !fields
            .iter()
            .any(|f| f == osprey_ast::RESULT_DEFAULT_PAYLOAD)
        {
            return;
        }
        let pruned = self.ctx.expose_settled(disc);
        if is_result(&pruned) || matches!(pruned, Type::Var(_)) {
            return;
        }
        self.errors.push(TypeError::new(format!(
            "`?:` needs a Result on its left, found {pruned}"
        )));
    }

    fn bind_result_fields(
        &mut self,
        variant: &str,
        fields: &[String],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        // An UNRESOLVED discriminant is not a candidate for the auto-wrap rule
        // below — auto-wrap answers "what does `Success { value }` mean over a
        // value that is known not to be a Result", and nothing is known here
        // yet. A `Success`/`Error` pattern is itself the evidence, so pin the
        // scrutinee to a Result with an open payload. Binding the whole
        // variable as the payload instead detached the two: once the scrutinee
        // later became `Result<int, Error>`, `value` was still the Result, and
        // `Success { value: value }` failed to unify it with `int`.
        if matches!(self.ctx.expose_settled(disc), Type::Var(_)) {
            let open = Type::result(self.ctx.fresh(), self.ctx.fresh());
            self.push_unify(&open, disc);
        }
        let dp = self.ctx.expose(disc);
        let ok = match &dp {
            Type::Con { name, args } if name == names::RESULT && !args.is_empty() => {
                args.first().cloned().unwrap_or_else(|| dp.clone())
            }
            _ => dp.clone(),
        };
        for (index, fname) in fields.iter().enumerate() {
            // The desugarer's unspellable `?:` binder names the SAME success
            // payload `value` does. Leaving it to the fresh-variable fallback
            // detached the payload from the fallback expression, so
            // `listGet([1, 2, 3], 0) ?: 9.5` type-checked and only failed in the
            // backend ("match arms disagree on type"), and an empty literal's
            // element type was never resolved at all [PATTERN-RESULT-DEFAULT].
            //
            // `Success`/`Error` bind by ROLE, not by spelling
            // ([FLAVOR-ML-MATCH], [PATTERN-RESULT-BINDERS]): `Success section` binds the success payload
            // under `section` exactly as `Success value` binds it under `value`,
            // and `Error message` binds the error string. Binding a renamed
            // positional binder to a fresh variable detached it from the
            // payload, so a generic callee reached through it (`find` in a
            // nested match) kept an unconstrained element type and codegen
            // refused the closure as still generic.
            let ft = match fname.as_str() {
                _ if index == 0 && variant == names::SUCCESS => ok.clone(),
                _ if index == 0 => Type::string(),
                "value" | osprey_ast::RESULT_DEFAULT_PAYLOAD => ok.clone(),
                "message" => Type::string(),
                _ => self.ctx.fresh(),
            };
            local.insert(fname.clone(), Scheme::mono(ft));
        }
    }
}
