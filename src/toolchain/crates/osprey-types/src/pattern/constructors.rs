//! Constructor and `Result` variant patterns: owner unification and binder aliases.

use super::ctor_owner_ty;
use super::is_result;
use super::is_result_variant;
use super::starts_uppercase;
use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::names;
use crate::ty::Scheme;
use crate::ty::Type;
use osprey_ast::Pattern;
use std::collections::BTreeMap;

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
        if !sub_patterns.is_empty() && self.is_result_pattern(name, disc) {
            self.bind_result_slot(name, fields, sub_patterns, disc, local);
            return;
        }
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
            let field_ty = declared.get(i).map(|(_, t)| t.clone());
            let target = field_ty.unwrap_or_else(|| self.ctx.fresh());
            self.bind_pattern(sub, &target, local);
        }
    }

    /// Whether `name` is a `Success`/`Error` pattern over a `Result`.
    fn is_result_pattern(&mut self, name: &str, disc: &Type) -> bool {
        is_result_variant(name)
            && (is_result(&self.ctx.expose(disc))
                || self
                    .ctors
                    .get(name)
                    .is_some_and(|ctor| ctor.owner == names::RESULT))
    }

    /// `Success(p)`, `Error { message: p }`: the one slot is matched at the
    /// payload's own type, nested to any depth ([PATTERN-NESTED]).
    fn bind_result_slot(
        &mut self,
        name: &str,
        fields: &[String],
        patterns: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        let expected = if name == names::SUCCESS {
            "value"
        } else {
            "message"
        };
        if !fields.is_empty() && fields != [expected] {
            self.errors.push(TypeError::new(format!(
                "expected pattern field `{expected}` on `{name}`"
            )));
        }
        if patterns.len() > 1 {
            self.errors.push(TypeError::new(format!(
                "`{name}` carries one payload, found {} patterns",
                patterns.len()
            )));
        }
        let payload = self.result_payload(name, disc);
        for pattern in patterns {
            self.bind_pattern(pattern, &payload, local);
        }
    }

    /// Named field labels select payloads; each slot is matched at its
    /// field's type ([PATTERN-NESTED]).
    fn bind_named_aliases(
        &mut self,
        name: &str,
        fields: &[String],
        patterns: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
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

    /// Bind the built-in `Result` pattern fields against `disc`: `value` is the
    /// success payload (the discriminant itself when it is not a `Result` — the
    /// match auto-wrap rule), `message` is the error payload.
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

    /// The payload a `Success`/`Error` pattern matches: the success value or
    /// the error at the Result's own `E`.
    fn result_payload(&mut self, variant: &str, disc: &Type) -> Type {
        let (ok, err) = self.result_sides(disc);
        if variant == names::SUCCESS {
            ok
        } else {
            err
        }
    }

    /// The success and error payload types of a `Result` scrutinee.
    fn result_sides(&mut self, disc: &Type) -> (Type, Type) {
        // An UNRESOLVED discriminant is not a candidate for the auto-wrap rule
        // below — auto-wrap answers "what does `Success { value }` mean over a
        // value that is known not to be a Result", and nothing is known here
        // yet. A `Success`/`Error` pattern is itself the evidence, so pin the
        // scrutinee to a Result with an open payload. Binding the whole
        // variable as the payload instead detached the two: once the scrutinee
        // later became `Result<int, string>`, `value` was still the Result, and
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
        // The error payload has the scrutinee's own error type. Implements
        // [ERR-PAYLOAD].
        let err = match &dp {
            Type::Con { name, args } if name == names::RESULT => {
                args.get(1).cloned().unwrap_or_else(Type::string)
            }
            _ => Type::string(),
        };
        (ok, err)
    }

    fn bind_result_fields(
        &mut self,
        variant: &str,
        fields: &[String],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        let (ok, err) = self.result_sides(disc);
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
            // and `Error message` binds the error payload. Binding a renamed
            // positional binder to a fresh variable detached it from the
            // payload, so a generic callee reached through it (`find` in a
            // nested match) kept an unconstrained element type and codegen
            // refused the closure as still generic.
            let ft = match fname.as_str() {
                _ if index == 0 && variant == names::SUCCESS => ok.clone(),
                _ if index == 0 => err.clone(),
                "value" | osprey_ast::RESULT_DEFAULT_PAYLOAD => ok.clone(),
                "message" => err.clone(),
                _ => self.ctx.fresh(),
            };
            local.insert(fname.clone(), Scheme::mono(ft));
        }
    }
}
