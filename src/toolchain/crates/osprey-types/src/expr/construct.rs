//! Lambdas, blocks, constructors and record updates.

use super::expected_params;
use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::Type;
use osprey_ast::Expr;
use osprey_ast::FieldAssignment;
use osprey_ast::Parameter;
use osprey_ast::Stmt;
use osprey_ast::TypeExpr;
use std::collections::BTreeMap;
use std::collections::HashSet;

impl Checker {
    pub(super) fn infer_lambda(
        &mut self,
        parameters: &[Parameter],
        return_type: Option<&TypeExpr>,
        body: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        self.infer_lambda_of(parameters, return_type, body, position, env, None)
    }

    /// [`Self::infer_lambda`] with the function type the context requires of
    /// this lambda, when there is one ([`Self::infer_checked`]). An
    /// unannotated parameter takes its expected type BEFORE the body is
    /// inferred; an annotated one keeps its annotation, which
    /// [`Self::apply_fn`] still checks against the slot.
    pub(super) fn infer_lambda_of(
        &mut self,
        parameters: &[Parameter],
        return_type: Option<&TypeExpr>,
        body: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
        expected: Option<&Type>,
    ) -> Type {
        let binder = self.current_fn_typarams.clone();
        let mut local = env.child();
        let mut ptys = Vec::new();
        let wanted = expected_params(expected, parameters.len());
        for (i, p) in parameters.iter().enumerate() {
            let ty = match &p.ty {
                Some(te) => self.annotation_type(te, &binder, te.position.or(position)),
                None => wanted.get(i).cloned().unwrap_or_else(|| self.ctx.fresh()),
            };
            local.insert(p.name.clone(), crate::ty::Scheme::mono(ty.clone()));
            if let Some(at) = position {
                self.binder_tys.record(at, &p.name, &ty);
            }
            ptys.push(ty);
        }
        // A lambda body runs when *called*, not where it is written: the
        // enclosing arm's continuation is not live inside it, so `resume`
        // there is the same hard error as at top level. Codegen already
        // clears its arm state across lambda boundaries (builder.rs) — this
        // keeps the checker in agreement so the program is rejected here,
        // with a type error, instead of deep in codegen. [EFFECTS-RESUME]
        let saved_resume_ctx = std::mem::take(&mut self.resume_ctx);
        // A lambda whose SLOT pins its parameters never leaves an arithmetic
        // overload open: they are already concrete by the time the body is
        // inferred, so the selection has everything it needs
        // ([`Self::defers`]).
        //
        // An unslotted lambda is the opposite case and must defer. `\x y => x +
        // y` bound to a name has nothing pinning `x` and `y` when its body is
        // inferred, so closing deferral there defaulted it to the integer
        // overload and every later string use was rejected. Deferring does not
        // make it polymorphic — the operand variables stay monomorphic
        // ([`Checker::generalize_with_obligations`]) and the overload is still
        // settled exactly once, by how the program actually uses it. This is
        // also what makes ML's curry-by-default agree with Default: `f a b = a
        // + b` lowers to a `Function` whose body is a `Lambda` chain
        // ([FLAVOR-ML-CURRY]), so the chain must leave the overload as open as
        // the flat `fn f(a, b) = a + b` does ([FLAVOR-IR-EQUIV]).
        let saved_defer = self.defer_arith;
        self.defer_arith = saved_defer && expected.is_none();
        let body_ty = self.infer_expr(body, &local);
        self.defer_arith = saved_defer;
        self.resume_ctx = saved_resume_ctx;
        let ret = match return_type {
            Some(te) => {
                let r = self.annotation_type(te, &binder, te.position.or(position));
                self.push_assign(&r, &body_ty);
                r
            }
            None => body_ty,
        };
        let fun = Type::fun(ptys, ret);
        // Publish this lambda's type for the backend, keyed by source position
        // (resolved against the final substitution in `infer_program`).
        if let Some(pos) = position {
            self.lambda_tys
                .push(((self.current_fn.clone(), pos), fun.clone()));
        }
        fun
    }

    pub(super) fn infer_block(
        &mut self,
        statements: &[Stmt],
        value: Option<&Expr>,
        env: &TypeEnv,
    ) -> Type {
        let mut local = env.child();
        for s in statements {
            self.infer_block_stmt(s, &mut local);
        }
        match value {
            Some(v) => self.infer_expr(v, &local),
            None => Type::unit(),
        }
    }

    pub(super) fn infer_constructor(
        &mut self,
        name: &str,
        type_args: &[TypeExpr],
        fields: &[FieldAssignment],
        env: &TypeEnv,
    ) -> Type {
        if let Some((args, declared, owner, is_record)) = self.ctor_instance(name) {
            // Explicit construction-site type arguments (`Box<int> { ... }`)
            // pin the instance's fresh variables, resolving names against the
            // enclosing function's type-parameter binder. Implements
            // [TYPE-GENERICS-DECL].
            if !type_args.is_empty() {
                if type_args.len() == args.len() {
                    let binder = self.current_fn_typarams.clone();
                    for (a, te) in args.iter().zip(type_args) {
                        let written = self.written_type_argument(te, &binder, te.position);
                        self.push_unify(a, &written);
                    }
                } else {
                    self.errors.push(TypeError::new(format!(
                        "constructor `{name}` takes {} type argument(s), got {}",
                        args.len(),
                        type_args.len()
                    )));
                }
            }
            let dmap: BTreeMap<String, Type> = declared.into_iter().collect();
            for fa in fields {
                let vt = self.infer_expr(&fa.value, env);
                if let Some(dt) = dmap.get(&fa.name) {
                    self.push_assign(&dt.clone(), &vt);
                }
            }
            self.check_ctor_fields(name, fields, &dmap);
            // Generic records must retain even phantom arguments: a field's
            // effect contract can depend on them. [EFFECTS-FIELD-ROWS]
            if is_record && args.is_empty() {
                Type::Record {
                    name: owner,
                    fields: dmap,
                }
            } else {
                Type::con(owner, args)
            }
        } else {
            for fa in fields {
                let _ = self.infer_expr(&fa.value, env);
            }
            self.errors
                .push(TypeError::new(format!("unknown constructor `{name}`")));
            self.ctx.fresh()
        }
    }

    /// A construction must supply exactly the variant's declared fields:
    /// `Success { data: 42 }` is missing `value` and names an unknown field.
    fn check_ctor_fields(
        &mut self,
        name: &str,
        fields: &[FieldAssignment],
        dmap: &BTreeMap<String, Type>,
    ) {
        if let Some(repeated) = repeated_assignment(fields) {
            self.errors.push(TypeError::new(format!(
                "constructor `{name}` assigns field `{repeated}` more than once"
            )));
        }
        for fa in fields {
            if !dmap.contains_key(&fa.name) {
                self.errors.push(TypeError::new(format!(
                    "constructor `{name}` has no field `{}`",
                    fa.name
                )));
            }
        }
        for dname in dmap.keys() {
            if !fields.iter().any(|fa| &fa.name == dname) {
                self.errors.push(TypeError::new(format!(
                    "constructor `{name}` requires field `{dname}`"
                )));
            }
        }
    }

    /// An update writes each named field once: the field must exist, the
    /// value must be assignable to it, and a still-generic record defers both
    /// to the call that fixes it — the same obligation a `.field` read leaves.
    /// Implements [TYPE-RECORD-UPDATE].
    pub(super) fn infer_update(
        &mut self,
        record: &str,
        fields: &[FieldAssignment],
        env: &TypeEnv,
    ) -> Type {
        if let Some(repeated) = repeated_assignment(fields) {
            self.errors.push(TypeError::new(format!(
                "record update assigns field `{repeated}` more than once"
            )));
        }
        let base = self.lookup_ident(record, env);
        for fa in fields {
            let vt = self.infer_expr(&fa.value, env);
            self.infer_record_field_write(&base, &fa.name, &vt);
        }
        self.ctx.prune(&base)
    }
}

/// The first field a construction or update names twice. Of two assignments
/// to one slot only one can land, and which one is an accident of lowering, so
/// the list is rejected rather than resolved.
fn repeated_assignment(fields: &[FieldAssignment]) -> Option<&str> {
    let mut seen = HashSet::new();
    fields
        .iter()
        .map(|fa| fa.name.as_str())
        .find(|name| !seen.insert(*name))
}
