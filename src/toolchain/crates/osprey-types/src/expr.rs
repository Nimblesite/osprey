//! Expression inference. One `infer_expr` dispatch covers every `ast::Expr`.
//! Where a type genuinely cannot be resolved (an opaque field access, an
//! unknown dynamic builtin) the inferencer yields a fresh variable rather than
//! a false error; the structured cases (calls, arithmetic, constructors,
//! lambdas, match) do real unification.

use crate::check::Checker;

use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::{names, Type};
use osprey_ast::{Expr, FieldAssignment, InterpolatedPart, TypeExpr};
use std::collections::{BTreeMap, HashMap};

mod apply;
mod arith;
mod arms;
mod calls;
mod construct;
#[cfg(test)]
mod tests_effects;
#[cfg(test)]
mod tests_forms;
#[cfg(test)]
mod tests_values;

pub(crate) use arith::{is_deferred_arith, is_monomorphic_arith};

fn generic_err() -> Type {
    Type::prim("Error")
}

/// A bound type variable is a type, not a higher-kinded constructor. Check
/// the written form before conversion resolves that name to its variable.
fn applied_type_parameter<'a>(
    ty: &'a TypeExpr,
    binders: &HashMap<String, Type>,
) -> Option<&'a str> {
    if binders.contains_key(&ty.name) && !ty.generic_params.is_empty() {
        return Some(&ty.name);
    }
    ty.generic_params
        .iter()
        .chain(&ty.parameter_types)
        .chain(ty.array_element.as_deref())
        .chain(ty.return_type.as_deref())
        .find_map(|child| applied_type_parameter(child, binders))
}

impl Checker {
    pub(crate) fn infer_expr(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        // The innermost expression carrying a position anchors every unify
        // failure recorded while inferring it or its children.
        let outer = self.expr_position;
        if let Some(position) = expression_position(e) {
            self.expr_position = Some(position);
        }
        let ty = osprey_ast::with_stack(|| {
            if let Some(error) = crate::arithmetic::constant_overflow(e) {
                self.errors.push(error);
            }
            let ty = self.infer_expr_raw(e, env);
            self.record_constant(e, &ty);
            if let Some(types) = &mut self.expression_types {
                let _ = types.insert(std::ptr::from_ref(e).addr(), ty.clone());
            }
            ty
        });
        self.expr_position = outer;
        ty
    }

    fn infer_expr_raw(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        match e {
            Expr::Integer(value) => self.integer_literal(*value),
            Expr::Float(value) => self.float_literal(*value),
            Expr::Str(_) => Type::string(),
            Expr::Bool(_) => Type::bool(),
            Expr::InterpolatedStr(parts) => self.infer_interpolation(parts, env),
            Expr::Identifier(name) => self.lookup_ident_at(name, env, Some(e)),
            Expr::Path(path) => self.lookup_ident_at(&path.to_string(), env, Some(e)),
            Expr::List(items, position) => {
                let elem = self.ctx.fresh();
                for it in items {
                    let t = self.infer_expr(it, env);
                    self.push_unify(&elem, &t);
                }
                let list = Type::list(elem);
                // Publish the literal's resolved type for the backend. An empty
                // literal has no element to lower, so this is the only channel
                // that can tell it whether `[]` is a `List<float>` or a
                // `List<int>` ([GPU-BUFFER-ELEM], [COLLECTIONS-LIST-ELEM]).
                if let Some(p) = position {
                    self.list_tys
                        .push(((self.current_fn.clone(), *p), list.clone()));
                }
                list
            }
            Expr::Map(entries) => self.infer_map(entries, env),
            Expr::Object(fields) => self.infer_object(fields, env),
            Expr::Binary {
                op,
                left,
                right,
                position,
            } => self.infer_binary(op, left, right, *position, env),
            Expr::Unary { op, operand } => {
                let t = self.infer_expr(operand, env);
                if self.reject_erased_operand(&format!("apply unary `{op}` to"), &t) {
                    Type::int()
                } else if op == "!" {
                    self.push_assign(&Type::bool(), &t);
                    Type::bool()
                } else {
                    self.infer_negation(&t)
                }
            }
            Expr::TypeApply {
                function,
                type_args,
                position,
            } => {
                self.infer_type_application(function, type_args, *position, env)
                    .1
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.infer_call(e, function, arguments, named_arguments, env),
            Expr::Pipe { left, right } => self.infer_pipe(left, right, env),
            Expr::FieldAccess { target, field } => self.infer_field_access(target, field, env),
            Expr::MethodCall { .. } => match crate::methods::parts(e) {
                Some(parts) => self.infer_method_call(e, parts, env),
                None => self.ctx.fresh(),
            },
            Expr::Index { target, index } => self.infer_index(target, index, env),
            Expr::Lambda {
                parameters,
                return_type,
                body,
                position,
            } => self.infer_lambda(parameters, return_type.as_ref(), body, *position, env),
            Expr::Match { value, arms } => self.infer_match(value, arms, env),
            Expr::Block {
                statements, value, ..
            } => self.infer_block(statements, value.as_deref(), env),
            Expr::TypeConstructor {
                name,
                type_args,
                fields,
            } => self.infer_constructor(name, type_args, fields, env),
            Expr::Update { record, fields } => self.infer_update(record, fields, env),
            Expr::Spawn(inner) => {
                let t = self.infer_expr(inner, env);
                Type::con(names::FIBER, vec![t])
            }
            Expr::Await(inner) => self.infer_unwrap_con(inner, names::FIBER, env),
            Expr::Recv(channel) => self.infer_recv(channel, env),
            Expr::Send { channel, value } => self.infer_send(channel, value, env),
            // Valued yield forwards its type; bare yield is Unit.
            // Implements [CONCURRENCY-YIELD].
            Expr::Yield(inner) => match inner {
                Some(inner) => self.infer_expr(inner, env),
                None => Type::unit(),
            },
            Expr::Select { operations, arms } => self.infer_select(operations, arms, env),
            Expr::Perform { .. } | Expr::Handler { .. } | Expr::Mask { .. } => {
                self.infer_effect_expr(e, env)
            }
            Expr::Instance { effect, position } => self.infer_instance(effect, *position),
            Expr::Resume(value) => self.infer_resume(value.as_deref(), env),
        }
    }

    fn infer_interpolation(&mut self, parts: &[InterpolatedPart], env: &TypeEnv) -> Type {
        for part in parts {
            if let InterpolatedPart::Expr(inner) = part {
                // Preserve complete Results and validate the runtime representation.
                let ty = self.infer_expr(inner, env);
                let name = self.sited_use("interpolation");
                self.builtin_uses.push((name, ty));
            }
        }
        Type::string()
    }

    /// Field access retains a relation between the receiver and its field even
    /// when the receiver is a parameter whose record is fixed at a later call.
    fn infer_field_access(&mut self, target: &Expr, field: &str, env: &TypeEnv) -> Type {
        let tt = self.infer_expr(target, env);
        self.infer_record_field_type(&tt, field)
    }

    pub(crate) fn infer_record_field_type(&mut self, receiver: &Type, field: &str) -> Type {
        // Reading a field assumes a record layout the erased word cannot
        // prove it has — the address-as-integer bug reached through `.field`
        // ([TYPE-FIELD-ACCESS-NON-RECORD], [TYPE-ANY]).
        if self.reject_erased_operand(&format!("access field `{field}` on"), receiver) {
            return self.ctx.fresh();
        }
        let result = self.ctx.fresh();
        self.builtin_uses.push((
            crate::fields::obligation_name(&self.site, field),
            Type::fun(vec![receiver.clone()], result.clone()),
        ));
        self.resolve_field_uses();
        result
    }

    /// Writing `field` of `receiver` through an update: the read's deferral
    /// with the relation's result being the value written, resolved as an
    /// assignment into the field. Implements [TYPE-RECORD-UPDATE].
    fn infer_record_field_write(&mut self, receiver: &Type, field: &str, value: &Type) {
        if self.reject_erased_operand(&format!("update field `{field}` on"), receiver) {
            return;
        }
        self.builtin_uses.push((
            crate::fields::write_obligation_name(&self.site, field),
            Type::fun(vec![receiver.clone()], value.clone()),
        ));
        self.resolve_field_uses();
    }

    /// A channel and its sent value share one element type; the send is `Unit`.
    /// Implements [CONCURRENCY-CHANNEL] (split out of [`Self::infer_expr`]).
    pub(crate) fn infer_send(&mut self, channel: &Expr, value: &Expr, env: &TypeEnv) -> Type {
        let channel_ty = self.infer_expr(channel, env);
        let value_ty = self.infer_expr(value, env);
        let element_ty = self.ctx.fresh();
        self.push_unify(
            &channel_ty,
            &Type::con(names::CHANNEL, vec![element_ty.clone()]),
        );
        self.push_assign(&element_ty, &value_ty);
        self.reject_result_channel(&value_ty, "handle the Result before sending");
        Type::unit()
    }

    /// Reject a `Result`-valued channel element. The wire word carries no
    /// discriminant, so a `Result` crossing a channel would be erased rather
    /// than delivered — both ends refuse it, differing only in which half of
    /// the transfer the advice names.
    fn reject_result_channel(&mut self, ty: &Type, detail: &str) {
        if self.ctx.expose(ty).is_named(names::RESULT) {
            self.errors.push(TypeError::new(format!(
                "Result-valued channels are not supported by this backend; {detail}"
            )));
        }
    }

    /// Record how the dotted call at `site` resolved, keyed by the expression's
    /// address — the one place the method-target table is written.
    fn record_method_target(&mut self, site: &Expr, target: crate::methods::Target) {
        let _ = self.methods.insert(std::ptr::from_ref(site).addr(), target);
    }

    pub(crate) fn infer_recv(&mut self, channel: &Expr, env: &TypeEnv) -> Type {
        let elem = self.infer_unwrap_con(channel, names::CHANNEL, env);
        self.reject_result_channel(&elem, "receiving must never erase the Result wrapper");
        elem
    }

    /// Infer `resume(v)`: its argument is delivered as the operation's result and
    /// must match that slot without erasing a Result error channel. The expression
    /// itself evaluates to the handler's answer type. A `resume` outside any
    /// handler arm is a hard error. Implements [EFFECTS-RESUME].
    fn infer_resume(&mut self, value: Option<&Expr>, env: &TypeEnv) -> Type {
        let arg = value.map_or_else(Type::unit, |v| self.infer_expr(v, env));
        match self.resume_ctx.last().cloned() {
            Some(crate::check::ResumeSite::Control { op_ret, answer }) => {
                self.push_assign(&op_ret, &arg);
                answer
            }
            site => {
                let refusal = crate::check::ResumeSite::refusal(site.as_ref());
                self.errors.push(TypeError::new(refusal));
                self.ctx.fresh()
            }
        }
    }

    /// Infer a runtime map literal. Its builder uses the string-key ABI
    /// [TYPE-MAP-LITERAL], so every key must be a string.
    fn infer_map(&mut self, entries: &[osprey_ast::MapEntry], env: &TypeEnv) -> Type {
        let v = self.ctx.fresh();
        for entry in entries {
            let kt = self.infer_expr(&entry.key, env);
            let vt = self.infer_expr(&entry.value, env);
            self.push_unify(&Type::string(), &kt);
            self.push_unify(&v, &vt);
        }
        Type::map(Type::string(), v)
    }

    /// Infer an anonymous object literal as an unnamed record of its fields.
    fn infer_object(&mut self, fields: &[FieldAssignment], env: &TypeEnv) -> Type {
        let mut map = BTreeMap::new();
        for fa in fields {
            let t = self.infer_expr(&fa.value, env);
            let _ = map.insert(fa.name.clone(), t);
        }
        Type::Record {
            name: String::new(),
            fields: map,
        }
    }

    /// Unwrap `await`/`recv`: unify the inner type with `con<elem>` and yield `elem`.
    fn infer_unwrap_con(&mut self, inner: &Expr, con: &str, env: &TypeEnv) -> Type {
        let t = self.infer_expr(inner, env);
        let elem = self.ctx.fresh();
        self.push_unify(&t, &Type::con(con, vec![elem.clone()]));
        elem
    }
}

fn res_math_like(ok: Type) -> Type {
    Type::result(ok, generic_err())
}

fn both_vars(l: &Type, r: &Type) -> bool {
    matches!(l, Type::Var(_)) && matches!(r, Type::Var(_))
}

/// The built-ins whose first argument's runtime representation the checker
/// must see: their scheme says `any`, so each call site is checked against
/// the concrete type it supplies ([BUILTIN-PRINT], [BUILTIN-TOSTRING],
/// [BUILTIN-COLLECTION-LENGTH], [BUILTIN-COLLECTION-ISEMPTY]).
fn constrained_builtin(name: &str) -> bool {
    matches!(
        name,
        "length" | "isEmpty" | "print" | "toString" | "toGpu" | "listSort"
    ) || crate::builtin_constraints::is_gpu_buffer_builtin(name)
}

/// The source position an expression itself carries, when it has one. Calls,
/// identifiers and literals have none — their diagnostics anchor to the
/// innermost enclosing expression that does, or the statement.
fn expression_position(e: &Expr) -> Option<osprey_ast::Position> {
    match e {
        Expr::List(_, position)
        | Expr::Binary { position, .. }
        | Expr::TypeApply { position, .. }
        | Expr::Lambda { position, .. }
        | Expr::Block { position, .. }
        | Expr::Perform { position, .. }
        | Expr::Handler { position, .. }
        | Expr::Mask { position, .. }
        | Expr::Instance { position, .. } => *position,
        _ => None,
    }
}

/// Whether an obligation key names a diagnostic a use site may locate. The
/// deferred-arithmetic, numeric-operand and comparison families, the
/// interpolation and structural-equality operands, and a constrained
/// built-in's receiver qualify; a field or method relation encodes its own
/// site, and an already-located or sited key keeps the location it has.
fn locatable_obligation(name: &str) -> bool {
    if name.contains(" @") {
        return false;
    }
    is_deferred_arith(name)
        || name.starts_with(crate::builtin_constraints::NUMERIC_OPERAND_PREFIX)
        || name.starts_with(crate::builtin_constraints::INTEGER_OPERAND_PREFIX)
        || name.starts_with(crate::builtin_constraints::FLOAT_OPERAND_PREFIX)
        || name.starts_with(crate::builtin_constraints::MATRIX_OPERAND_PREFIX)
        || name.starts_with(crate::comparison::COMPARED_OPERAND_PREFIX)
        || name == "interpolation"
        || name == "structuralEquals"
        || constrained_builtin(name)
}

/// The parameter types a lambda's context requires of it, when that context is
/// a function type of matching arity — the checking half of
/// [`Checker::infer_lambda_of`]. Anything else leaves every parameter open.
fn expected_params(expected: Option<&Type>, arity: usize) -> Vec<Type> {
    match expected {
        Some(Type::Fun { params, .. }) if params.len() == arity => params.clone(),
        _ => Vec::new(),
    }
}
