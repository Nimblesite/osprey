//! Pattern inference and match exhaustiveness.
//!
//! Binding a constructor pattern unifies the discriminant with the
//! constructor's owner type, so the discriminant's type arguments flow into the
//! bound field types (`Success { value }` over `Result<int, E>` binds
//! `value : int`). Exhaustiveness is enforced only where the checker can decide
//! it with confidence — `bool` and known union/`Result` discriminants — and is
//! otherwise deferred to a catch-all, so it never reports a false positive.

use crate::check::Checker;
use crate::convert::type_expr_to_type;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::{names, Scheme, Type};
use osprey_ast::{Expr, MatchArm, Pattern};
use std::collections::{BTreeMap, HashMap};

mod constructors;
mod exhaustive;
mod joins;
mod structural;
#[cfg(test)]
mod tests_constructors;
#[cfg(test)]
mod tests_structural;

fn is_result(t: &Type) -> bool {
    matches!(t, Type::Con { name, .. } if name == names::RESULT)
}

/// Whether `name` is one of the built-in `Result` variant constructors. A
/// user-declared variant may reuse these names (shadowing the builtin for
/// construction), but a pattern over a discriminant that *is* a `Result` always
/// means the built-in variant.
fn is_result_variant(name: &str) -> bool {
    name == names::SUCCESS || name == names::ERROR
}

/// An initial-uppercase identifier reads as a constructor/variant; a lower-case
/// one reads as an ordinary variable binding.
fn starts_uppercase(name: &str) -> bool {
    osprey_ast::is_constructor_name(name)
}

impl Checker {
    pub(crate) fn infer_match(&mut self, value: &Expr, arms: &[MatchArm], env: &TypeEnv) -> Type {
        let disc = self.infer_expr(value, env);
        let result = self.ctx.fresh();
        let bodies = arms
            .iter()
            .map(|arm| self.infer_arm(arm, &disc, env))
            .collect::<Vec<_>>();
        let result = self.match_result(result, &bodies);
        self.check_exhaustive(&disc, arms);
        self.check_redundant_arms(arms, &disc);
        result
    }

    /// Infer one arm's body type after binding its pattern against the exact
    /// discriminant type.
    pub(crate) fn infer_arm(&mut self, arm: &MatchArm, disc: &Type, env: &TypeEnv) -> Type {
        let mut local = env.child();
        self.bind_pattern(&arm.pattern, disc, &mut local);
        self.record_arm_binders(arm, &local);
        self.infer_expr(&arm.body, &local)
    }

    /// Publish each binder of a source arm under the arm's position, which its
    /// body block carries; a desugared arm has none and binds nothing a reader
    /// wrote. Implements [LSP-HOVER-BINDERS].
    fn record_arm_binders(&mut self, arm: &MatchArm, local: &TypeEnv) {
        let Expr::Block {
            position: Some(at), ..
        } = &arm.body
        else {
            return;
        };
        let names = pattern_binders_where(&arm.pattern, &|name| !self.names_variant(name));
        for name in names {
            if let Some(scheme) = local.get(&name) {
                self.binder_tys.record(*at, &name, &scheme.ty);
            }
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, disc: &Type, local: &mut TypeEnv) {
        if self.reject_reading_erased(pattern, disc, local) {
            return;
        }
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind_binding(name, disc, local),
            Pattern::Literal(expr) => {
                let lt = self.infer_expr(expr, local);
                self.push_unify(disc, &lt);
            }
            Pattern::TypeAnnotated { name, ty } => {
                let t = type_expr_to_type(ty, &HashMap::new());
                local.insert(name.clone(), Scheme::mono(t));
            }
            Pattern::Structural { fields, open } => {
                self.bind_structural(fields, *open, disc, local);
            }
            Pattern::Constructor {
                name,
                fields,
                sub_patterns,
            } => self.bind_constructor(name, fields, sub_patterns, disc, local),
            Pattern::List { elements, rest } => {
                self.bind_list_pattern(elements, rest.as_deref(), disc, local);
            }
        }
    }

    /// Every pattern that READS a value's shape without a runtime row test — a
    /// literal compare, a variant tag, a list length, a type-annotated binding
    /// that simply renames the static type — is an unchecked read over an
    /// erased `any`: the same recovery [TYPE-ANY] rejects at annotations,
    /// respelled as a pattern (`s: string => length(s)` compiled and read the
    /// word with no test at all). Only a structural arm carries a row test, so
    /// everything else is rejected toward one. `Success`/`Error` stay legal:
    /// auto-wrap binds the erased value whole and reads nothing through it.
    fn reject_reading_erased(
        &mut self,
        pattern: &Pattern,
        disc: &Type,
        local: &mut TypeEnv,
    ) -> bool {
        if !self.ctx.expose(disc).is_named(names::ANY) {
            return false;
        }
        let offending = match pattern {
            Pattern::Literal(_) => "a literal pattern",
            Pattern::List { .. } => "a list pattern",
            Pattern::TypeAnnotated { .. } => "a type-annotated binding",
            Pattern::Constructor { name, .. } if !is_result_variant(name) => {
                "a constructor pattern"
            }
            Pattern::Binding(name) if self.names_variant(name) => "a variant pattern",
            _ => return false,
        };
        // Bind the pattern's names as `any` so one mistake does not cascade
        // into unknown-identifier noise in the arm body.
        for binder in pattern_binder_names(pattern) {
            local.insert(binder, Scheme::mono(Type::any()));
        }
        self.errors.push(TypeError::new(format!(
            "{offending} cannot read an erased `any`: match its structure instead"
        )));
        true
    }

    /// Bind a structural row pattern ([PATTERN-STRUCTURAL]) or its positional
    /// tuple spelling ([PATTERN-TUPLE]). Over a concrete record the row is
    /// checked here — a closed pattern names the exact row, an open one a
    /// subset — and each binder takes its declared field type. Over `any`
    /// every binder is itself `any`: the row is unknown until run time, so a
    /// fresh-variable binder would let the body read the field at any type it
    /// pleased — recovery by pattern, the hole [TYPE-ANY] deletes.
    /// A structural pattern reads every field it names, so an opaque record
    /// outside its module is rejected at the first one ([MODULES-OPAQUE-TYPES]).
    fn hidden_row(&self, scrutinee: &Type, fields: &[(String, String)]) -> Option<TypeError> {
        fields
            .iter()
            .find_map(|(field, _)| self.hidden_field(scrutinee, field, &self.site))
    }
}

/// Every name a pattern would bind — the cascade-avoidance binding set for a
/// rejected arm over an erased scrutinee.
pub(crate) fn pattern_binder_names(pattern: &Pattern) -> Vec<String> {
    pattern_binders_where(pattern, &|_| true)
}

/// Bare constructor spellings select a variant rather than binding a value.
pub(crate) fn pattern_binders_where(
    pattern: &Pattern,
    binding: &impl Fn(&str) -> bool,
) -> Vec<String> {
    match pattern {
        Pattern::TypeAnnotated { name, .. } => vec![name.clone()],
        Pattern::Binding(name) => binding(name).then(|| name.clone()).into_iter().collect(),
        Pattern::Constructor {
            fields,
            sub_patterns,
            ..
        } => fields
            .iter()
            .filter(|_| sub_patterns.is_empty())
            .cloned()
            .chain(
                sub_patterns
                    .iter()
                    .flat_map(|pattern| pattern_binders_where(pattern, binding)),
            )
            .collect(),
        Pattern::List { elements, rest } => elements
            .iter()
            .flat_map(|pattern| pattern_binders_where(pattern, binding))
            .chain(rest.iter().cloned())
            .collect(),
        Pattern::Structural { fields, .. } => fields
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(_, b)| b.clone())
            .collect(),
        Pattern::Wildcard | Pattern::Literal(_) => Vec::new(),
    }
}

/// The type a variant pattern ties its discriminant to: the owning record with
/// the constructor's declared fields, or the owning union.
fn ctor_owner_ty(
    owner: String,
    args: Vec<Type>,
    fields: BTreeMap<String, Type>,
    is_record: bool,
) -> Type {
    if is_record {
        Type::Record {
            name: owner,
            fields,
        }
    } else {
        Type::con(owner, args)
    }
}
