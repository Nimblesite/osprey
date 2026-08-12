//! Expression inference. One `infer_expr` dispatch covers every `ast::Expr`.
//! Where a type genuinely cannot be resolved (an opaque field access, an
//! unknown dynamic builtin) the inferencer yields a fresh variable rather than
//! a false error; the structured cases (calls, arithmetic, constructors,
//! lambdas, match) do real unification.

use crate::check::Checker;
use crate::convert::type_expr_to_type;

/// Builtin types that carry no fields, so `x.field` on one can never resolve.
/// Deliberately excludes `any` (which unifies with everything, including
/// records) and every collection/generic whose element MIGHT be a record.
/// Implements [TYPE-FIELD-ACCESS-NON-RECORD].
const FIELDLESS_TYPES: &[&str] = &[
    names::INT,
    names::FLOAT,
    names::STRING,
    names::BOOL,
    names::UNIT,
];
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::{names, Type};
use crate::unify::unify;
use osprey_ast::{
    Expr, FieldAssignment, InterpolatedPart, NamedArgument, Parameter, Stmt, TypeExpr,
};
use std::collections::{BTreeMap, HashMap};

fn math_err() -> Type {
    Type::prim(names::MATH_ERROR)
}
fn res_math(ok: Type) -> Type {
    Type::result(ok, math_err())
}
fn generic_err() -> Type {
    Type::prim("Error")
}

impl Checker {
    pub(crate) fn infer_expr(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        match e {
            Expr::Integer(_) => Type::int(),
            Expr::Float(_) => Type::float(),
            Expr::Str(_) => Type::string(),
            Expr::Bool(_) => Type::bool(),
            Expr::InterpolatedStr(parts) => {
                for p in parts {
                    if let InterpolatedPart::Expr(inner) = p {
                        // Interpolation preserves a Result as its complete
                        // Success/Error rendering; it never extracts a payload.
                        let _ = self.infer_expr(inner, env);
                    }
                }
                Type::string()
            }
            Expr::Identifier(name) => self.lookup_ident(name, env),
            Expr::Path(path) => self.lookup_ident(&path.to_string(), env),
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
                    self.list_tys.push((*p, list.clone()));
                }
                list
            }
            Expr::Map(entries) => self.infer_map(entries, env),
            Expr::Object(fields) => self.infer_object(fields, env),
            Expr::Binary { op, left, right } => self.infer_binary(op, left, right, env),
            Expr::Unary { op, operand } => {
                let t = self.infer_expr(operand, env);
                if op == "!" {
                    self.push_assign(&Type::bool(), &t);
                    Type::bool()
                } else {
                    self.infer_negation(&t)
                }
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.infer_call(function, arguments, named_arguments, env),
            Expr::Pipe { left, right } => self.infer_pipe(left, right, env),
            Expr::FieldAccess { target, field } => self.infer_field_access(target, field, env),
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => self.infer_method_call(target, method, arguments, named_arguments, env),
            Expr::Index { target, index } => self.infer_index(target, index, env),
            Expr::Lambda {
                parameters,
                return_type,
                body,
                position,
            } => self.infer_lambda(parameters, return_type.as_ref(), body, *position, env),
            Expr::Match { value, arms } => self.infer_match(value, arms, env),
            Expr::Block { statements, value } => {
                self.infer_block(statements, value.as_deref(), env)
            }
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
            Expr::Select { .. } => {
                // Parsing remains reserved, but accepting the node would reach
                // a backend that cannot perform channel selection.
                // Implements [CONCURRENCY-SELECT-REJECT].
                self.errors.push(TypeError::new(
                    "`select` is not supported; use explicit `send`, `recv`, and `await`",
                ));
                self.ctx.fresh()
            }
            Expr::Perform { .. } | Expr::Handler { .. } => self.infer_effect_expr(e, env),
            Expr::Resume(value) => self.infer_resume(value.as_deref(), env),
        }
    }

    /// Dispatch the two effect expression forms (split out of [`Self::infer_expr`]
    /// to keep its match within budget).
    fn infer_effect_expr(&mut self, e: &Expr, env: &TypeEnv) -> Type {
        match e {
            Expr::Perform {
                effect,
                operation,
                arguments,
                named_arguments,
                position,
            } => self.infer_perform(
                effect,
                operation,
                arguments,
                named_arguments,
                *position,
                env,
            ),
            Expr::Handler {
                effect,
                arms,
                body,
                position,
                ..
            } => self.infer_handler(effect, arms, body, *position, env),
            other => self.infer_expr(other, env),
        }
    }

    /// Field access yields the record field's declared type, or a fresh var when
    /// the target is not a known record (split out of [`Self::infer_expr`]).
    fn infer_field_access(&mut self, target: &Expr, field: &str, env: &TypeEnv) -> Type {
        let tt = self.infer_expr(target, env);
        let pruned = self.ctx.prune(&tt);
        match &pruned {
            Type::Record { fields, .. } => fields
                .get(field)
                .cloned()
                .unwrap_or_else(|| self.ctx.fresh()),
            other => {
                if let Type::Con { name, .. } = other {
                    if FIELDLESS_TYPES.contains(&name.as_str()) {
                        self.record_field_access_on_fieldless(field, name);
                    }
                }
                self.ctx.fresh()
            }
        }
    }

    /// Reject `x.field` where `x` is a builtin that has no fields at all.
    ///
    /// This arm used to fall through to a fresh variable, so the program passed
    /// the checker and CODEGEN emitted invalid LLVM (`bitcast i8* 42 to
    /// { i64, i64 }*`) — the "error" then surfaced from clang, naming a
    /// temporary `.ll` file rather than the offending source line. Only names
    /// that can NEVER be a user record are listed, and an unresolved type
    /// variable is deliberately left alone: inference may still resolve it to a
    /// record. Implements [TYPE-FIELD-ACCESS-NON-RECORD].
    fn record_field_access_on_fieldless(&mut self, field: &str, ty: &str) {
        self.errors.push(TypeError::new(format!(
            "cannot access field '{field}' on non-struct type {ty}"
        )));
    }

    /// A channel and its sent value share one element type; the send is `Unit`.
    /// Implements [CONCURRENCY-CHANNEL] (split out of [`Self::infer_expr`]).
    fn infer_send(&mut self, channel: &Expr, value: &Expr, env: &TypeEnv) -> Type {
        let channel_ty = self.infer_expr(channel, env);
        let value_ty = self.infer_expr(value, env);
        let element_ty = self.ctx.fresh();
        self.push_unify(
            &channel_ty,
            &Type::con(names::CHANNEL, vec![element_ty.clone()]),
        );
        self.push_assign(&element_ty, &value_ty);
        if self.ctx.prune(&value_ty).is_named(names::RESULT) {
            self.errors.push(TypeError::new(
                "Result-valued channels are not supported by this backend; handle the Result before sending",
            ));
        }
        Type::unit()
    }

    fn infer_recv(&mut self, channel: &Expr, env: &TypeEnv) -> Type {
        let elem = self.infer_unwrap_con(channel, names::CHANNEL, env);
        if self.ctx.prune(&elem).is_named(names::RESULT) {
            self.errors.push(TypeError::new(
                "Result-valued channels are not supported by this backend; receiving must never erase the Result wrapper",
            ));
        }
        elem
    }

    /// Infer `resume(v)`: its argument is delivered as the operation's result and
    /// must match that slot without erasing a Result error channel. The expression
    /// itself evaluates to the handler's answer type. A `resume` outside any
    /// handler arm is a hard error. Implements [EFFECTS-RESUME].
    fn infer_resume(&mut self, value: Option<&Expr>, env: &TypeEnv) -> Type {
        let arg = value.map_or_else(Type::unit, |v| self.infer_expr(v, env));
        if let Some((op_ret, answer)) = self.resume_ctx.last().cloned() {
            self.push_assign(&op_ret, &arg);
            answer
        } else {
            self.errors.push(TypeError::new(
                "`resume` is only valid inside a handler arm".to_string(),
            ));
            self.ctx.fresh()
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

    /// Infer a `perform`: resolve the effect's instantiation innermost-first
    /// against the enclosing handler/effect-row scopes (falling back to a
    /// fresh instantiation of the declaration), unify the arguments against
    /// the instantiated parameters, and yield the instantiated result type.
    /// The resolved signature is published per site for the code generator.
    /// Implements [EFFECTS-GENERIC-INSTANTIATION] and [EFFECTS-OP-TYPING].
    fn infer_perform(
        &mut self,
        effect: &str,
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
        let actual_instance = self.effect_instance_ops(effect);
        if !named_arguments.is_empty() {
            self.errors.push(TypeError::new(format!(
                "perform `{effect}.{operation}` does not support named arguments"
            )));
        }
        let scope = self
            .handler_scopes
            .iter()
            .rev()
            .find(|s| s.name == effect)
            .map(|s| (s.args.clone(), s.ops.clone()))
            .or_else(|| self.effect_instance_ops(effect));
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
    /// The handled body, the arms, and the whole expression all share one
    /// answer type; a Result answer remains a Result unless explicitly handled.
    /// Implements [EFFECTS-RESUME],
    /// [EFFECTS-GENERIC-INSTANTIATION], and [EFFECTS-OP-TYPING].
    fn infer_handler(
        &mut self,
        effect: &str,
        arms: &[osprey_ast::HandlerArm],
        body: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        let (eff_args, inst_ops, effect_known) =
            if let Some((args, ops)) = self.effect_instance_ops(effect) {
                (args, ops, true)
            } else {
                self.errors
                    .push(TypeError::new(format!("unknown effect `{effect}`")));
                (Vec::new(), HashMap::new(), false)
            };
        let answer = self.ctx.fresh();
        for arm in arms {
            let (params, op_ret) = match inst_ops.get(&arm.operation) {
                Some(op) if op.params.len() == arm.params.len() => {
                    (op.params.clone(), op.ret.clone())
                }
                Some(op) => {
                    self.errors.push(TypeError::new(format!(
                        "handler operation `{effect}.{}` expects {} parameter(s), got {}",
                        arm.operation,
                        op.params.len(),
                        arm.params.len()
                    )));
                    (
                        (0..arm.params.len()).map(|_| self.ctx.fresh()).collect(),
                        op.ret.clone(),
                    )
                }
                None => {
                    if effect_known {
                        self.errors.push(TypeError::new(format!(
                            "effect `{effect}` has no operation `{}`",
                            arm.operation
                        )));
                    }
                    (
                        (0..arm.params.len()).map(|_| self.ctx.fresh()).collect(),
                        self.ctx.fresh(),
                    )
                }
            };
            let mut local = env.child();
            for (p, pty) in arm.params.iter().zip(params) {
                local.insert(p.clone(), crate::ty::Scheme::mono(pty));
            }
            self.resume_ctx.push((op_ret.clone(), answer.clone()));
            let arm_ty = self.infer_expr(&arm.body, &local);
            let _ = self.resume_ctx.pop();
            // A non-resuming arm's value substitutes for the operation's
            // RESULT (value substitution — this is what pins a generic
            // effect's instantiation from its handler); a resuming arm's
            // value is the handler's ANSWER. A `Unit` operation discards the
            // arm's value, so anything goes there. Implements
            // [EFFECTS-RESUME] and [EFFECTS-GENERIC-INSTANTIATION].
            if osprey_ast::contains_resume(&arm.body) {
                self.push_assign(&answer, &arm_ty);
            } else if !self.ctx.prune(&op_ret).is_named(crate::ty::names::UNIT) {
                self.push_assign(&op_ret, &arm_ty);
            } else if self.ctx.prune(&arm_ty).is_named(crate::ty::names::RESULT) {
                self.errors.push(TypeError::new(
                    "an unhandled `Result` cannot be discarded by a Unit effect operation arm; use `match` or `?:`",
                ));
            }
        }
        self.handler_scopes.push(crate::check::EffectScope {
            name: effect.to_string(),
            args: eff_args.clone(),
            ops: inst_ops.clone(),
        });
        let body_ty = self.infer_expr(body, env);
        let _ = self.handler_scopes.pop();
        if let Some(pos) = position {
            self.handler_tys.push((pos, eff_args, inst_ops));
        }
        self.push_assign(&answer, &body_ty);
        answer
    }

    fn lookup_ident(&mut self, name: &str, env: &TypeEnv) -> Type {
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
        if let Some(scheme) = env.get(name).cloned() {
            // Re-state the scheme's built-in obligations against this site's
            // fresh variables, so a generalized wrapper's constraint is checked
            // against the types this call actually supplies
            // ([`crate::ty::Scheme::obligations`]).
            let (ty, obligations) = crate::env::instantiated(&mut self.ctx, &scheme);
            self.builtin_uses.extend(obligations);
            return ty;
        }
        self.errors
            .push(TypeError::new(format!("unknown identifier `{name}`")));
        self.ctx.fresh()
    }

    fn infer_call(
        &mut self,
        function: &Expr,
        arguments: &[Expr],
        named: &[NamedArgument],
        env: &TypeEnv,
    ) -> Type {
        let (fname, ft) = self.infer_callee(function, env);
        let args = self.ordered_arg_types(fname.as_deref(), &ft, arguments, named, env);
        self.apply_named_fn(fname.as_deref(), &ft, args)
    }

    fn infer_callee(&mut self, function: &Expr, env: &TypeEnv) -> (Option<String>, Type) {
        match function {
            Expr::Identifier(name) => (Some(name.clone()), self.lookup_ident(name, env)),
            Expr::Path(path) => {
                let name = path.to_string();
                let ty = self.lookup_ident(&name, env);
                (Some(name), ty)
            }
            other => (None, self.infer_expr(other, env)),
        }
    }

    fn infer_method_call(
        &mut self,
        target: &Expr,
        method: &str,
        arguments: &[Expr],
        named: &[NamedArgument],
        env: &TypeEnv,
    ) -> Type {
        // UFCS: `t.m(a)` is `m(t, a)`.
        let ft = self.lookup_ident(method, env);
        let mut args = vec![self.infer_expr(target, env)];
        for a in arguments {
            args.push(self.infer_expr(a, env));
        }
        for na in named {
            args.push(self.infer_expr(&na.value, env));
        }
        self.apply_named_fn(Some(method), &ft, args)
    }

    /// Resolve call arguments to types, reordering named arguments to the
    /// declared parameter order when the callee is a known function
    /// ([CALL-ARGUMENTS]).
    fn ordered_arg_types(
        &mut self,
        fname: Option<&str>,
        ft: &Type,
        arguments: &[Expr],
        named: &[NamedArgument],
        env: &TypeEnv,
    ) -> Vec<Type> {
        if !named.is_empty() {
            if let Some(pnames) = fname.and_then(|n| self.fn_params.get(n).cloned()) {
                let mut out = Vec::new();
                for pn in &pnames {
                    if let Some(na) = named.iter().find(|a| &a.name == pn) {
                        out.push(self.infer_expr(&na.value, env));
                    }
                }
                if out.len() == named.len() {
                    return out;
                }
            }
            return named
                .iter()
                .map(|a| self.infer_expr(&a.value, env))
                .collect();
        }
        self.positional_arg_types(ft, arguments, env)
    }

    /// Positional arguments, inferred in two passes: every argument that is
    /// not a lambda first — each linked to the parameter slot it fills — then
    /// each lambda, CHECKED against the parameter type those links have now
    /// pinned.
    ///
    /// A kernel's parameter types flow from the buffer and the seed
    /// [GPU-KERNEL-ELEM-TYPING] (docs/specs/0034-GPUComputation.md) because of
    /// this order. Inferring a lambda body before its parameter slot is known
    /// leaves bare arithmetic (`a + v`) with two unconstrained operands, which
    /// int-defaults for want of a numeric class — so
    /// `gpuFold(0.0, fn(a, v) => a + v)` over a float buffer was rejected with
    /// `cannot unify int with float` and every float kernel needed
    /// annotations.
    ///
    /// Pass one's links are best-effort: a genuine mismatch is reported once,
    /// by [`Self::apply_fn`], which re-checks every argument against its slot.
    fn positional_arg_types(&mut self, ft: &Type, arguments: &[Expr], env: &TypeEnv) -> Vec<Type> {
        let params = self.callee_params(ft, arguments.len());
        let Some(params) = params else {
            return arguments.iter().map(|a| self.infer_expr(a, env)).collect();
        };
        let mut out: Vec<Option<Type>> = vec![None; arguments.len()];
        for ((slot, a), param) in out.iter_mut().zip(arguments).zip(&params) {
            if !matches!(a, Expr::Lambda { .. }) {
                *slot = Some(self.linked_arg(a, param, env));
            }
        }
        for ((slot, a), param) in out.iter_mut().zip(arguments).zip(&params) {
            if slot.is_none() {
                *slot = Some(self.infer_checked(a, param, env));
            }
        }
        out.into_iter().flatten().collect()
    }

    /// The callee's parameter types when it is a known function of matching
    /// arity, and any argument is a lambda whose body the parameter types can
    /// inform. Every other call keeps the plain left-to-right pass.
    fn callee_params(&mut self, ft: &Type, arity: usize) -> Option<Vec<Type>> {
        match self.ctx.prune(ft) {
            Type::Fun { params, .. } if params.len() == arity => Some(params),
            _ => None,
        }
    }

    /// An argument inferred and linked to the parameter slot it fills, so a
    /// later lambda argument sees what this one pinned. An `any` slot is left
    /// unlinked — [`Self::apply_named_fn`] keeps a constrained builtin's
    /// argument variables open for the surrounding expression to refine.
    fn linked_arg(&mut self, argument: &Expr, param: &Type, env: &TypeEnv) -> Type {
        let ty = self.infer_expr(argument, env);
        if !param.is_named(names::ANY) {
            let _ = crate::unify::unify_assignable(&mut self.ctx, param, &ty);
        }
        ty
    }

    /// A lambda argument inferred against the function type its slot declares.
    fn infer_checked(&mut self, argument: &Expr, param: &Type, env: &TypeEnv) -> Type {
        match argument {
            Expr::Lambda {
                parameters,
                return_type,
                body,
                position,
            } => {
                let expected = self.ctx.prune(param);
                self.infer_lambda_of(
                    parameters,
                    return_type.as_ref(),
                    body,
                    *position,
                    env,
                    Some(&expected),
                )
            }
            other => self.infer_expr(other, env),
        }
    }

    fn apply_fn(&mut self, ft: &Type, args: Vec<Type>, defer_any_binding: bool) -> Type {
        match self.ctx.prune(ft) {
            Type::Fun { params, ret } => {
                if params.len() != args.len() {
                    self.errors.push(TypeError::new(format!(
                        "call arity mismatch: expected {} argument(s), got {}",
                        params.len(),
                        args.len()
                    )));
                    return *ret;
                }
                for (p, a) in params.iter().zip(&args) {
                    if !self.absorbed_by_any(p, a, defer_any_binding) {
                        self.push_assign(p, a);
                    }
                }
                *ret
            }
            ft @ Type::Var(_) => {
                let ret = self.ctx.fresh();
                let f = Type::fun(args, ret.clone());
                let _ = unify(&mut self.ctx, &ft, &f);
                ret
            }
            other => {
                self.errors.push(TypeError::new(format!(
                    "cannot call non-function `{other}`"
                )));
                self.ctx.fresh()
            }
        }
    }

    /// Whether passing `argument` to an `any` parameter would only DESTROY
    /// information, so the assignment is skipped.
    ///
    /// `any` unifies with every type ([TYPE-ANY]), so assigning a resolved
    /// argument to it already learns nothing. Assigning an unresolved VARIABLE
    /// to it learns nothing either — and costs the variable, because unify
    /// binds a variable before it reaches the `any` wildcard arm. That is how
    /// `expect(add(1, 1), 2)` erased the pending overload of
    /// `fn add(a, b) = a + b`: the site's open result became `any`, so
    /// settling it later could no longer say `Result<int, MathError>` and the
    /// backend read the call as a plain word. A constrained built-in defers the
    /// same way for the same reason, by name.
    fn absorbed_by_any(&mut self, param: &Type, argument: &Type, deferred: bool) -> bool {
        param.is_named(names::ANY) && (deferred || matches!(self.ctx.prune(argument), Type::Var(_)))
    }

    fn apply_named_fn(&mut self, name: Option<&str>, ft: &Type, args: Vec<Type>) -> Type {
        let constrained_builtin = name.is_some_and(|name| {
            matches!(name, "length" | "isEmpty" | "print" | "toString" | "toGpu")
                || crate::builtin_constraints::is_gpu_buffer_builtin(name)
        });
        if let (Some(name), Some(receiver)) = (name, args.first()) {
            if constrained_builtin {
                self.builtin_uses.push((name.to_string(), receiver.clone()));
            }
        }
        let fused = self.fuse_gpu_source(name, ft, &args);
        // Preserve unresolved argument variables until the surrounding expression
        // can refine them; the recorded constraint validates the final type.
        self.apply_fn(fused.as_ref().unwrap_or(ft), args, constrained_builtin)
    }

    /// `toGpu` is written over `List<t>`, but an iterator pipeline is an
    /// equally valid source — it fuses straight into the dense buffer with no
    /// list in between [GPU-BUFFER-FUSE]. Retarget the parameter's container
    /// to `Iterator` when the argument is one, reusing the *same* element type
    /// so the `GpuBuffer<t>` return stays linked to it. `None` leaves the
    /// declared scheme untouched.
    fn fuse_gpu_source(&mut self, name: Option<&str>, ft: &Type, args: &[Type]) -> Option<Type> {
        if name != Some("toGpu") {
            return None;
        }
        let Type::Fun { params, ret } = self.ctx.prune(ft) else {
            return None;
        };
        let [Type::Con {
            name: con,
            args: elem,
        }] = params.as_slice()
        else {
            return None;
        };
        if con != names::LIST || !self.is_iterator(args.first()?) {
            return None;
        }
        Some(Type::fun(
            vec![Type::con(names::ITERATOR, elem.clone())],
            (*ret).clone(),
        ))
    }

    /// Whether an argument has already resolved to an `Iterator<_>`.
    fn is_iterator(&mut self, ty: &Type) -> bool {
        matches!(self.ctx.apply(ty), Type::Con { name, .. } if name == names::ITERATOR)
    }

    fn infer_pipe(&mut self, left: &Expr, right: &Expr, env: &TypeEnv) -> Type {
        if let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = right
        {
            let mut args = Vec::with_capacity(arguments.len() + 1);
            args.push(left.clone());
            args.extend(arguments.iter().cloned());
            let call = Expr::Call {
                function: function.clone(),
                arguments: args,
                named_arguments: named_arguments.clone(),
            };
            self.infer_expr(&call, env)
        } else {
            let (name, ft) = self.infer_callee(right, env);
            let lt = self.infer_expr(left, env);
            self.apply_named_fn(name.as_deref(), &ft, vec![lt])
        }
    }

    fn infer_index(&mut self, target: &Expr, index: &Expr, env: &TypeEnv) -> Type {
        let tt = self.infer_expr(target, env);
        let it = self.infer_expr(index, env);
        match self.ctx.prune(&tt) {
            Type::Con { name, args } if name == names::LIST && !args.is_empty() => {
                self.push_assign(&Type::int(), &it);
                res_math_like(args.first().cloned().unwrap_or_else(|| self.ctx.fresh()))
            }
            Type::Con { name, args } if name == names::MAP && args.len() == 2 => {
                if let Some(key) = args.first() {
                    self.push_assign(key, &it);
                }
                res_math_like(args.get(1).cloned().unwrap_or_else(|| self.ctx.fresh()))
            }
            t if t.is_named(names::STRING) => {
                self.push_assign(&Type::int(), &it);
                res_math_like(Type::string())
            }
            _ => {
                let fresh = self.ctx.fresh();
                res_math_like(fresh)
            }
        }
    }

    fn infer_lambda(
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
    fn infer_lambda_of(
        &mut self,
        parameters: &[Parameter],
        return_type: Option<&TypeExpr>,
        body: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
        expected: Option<&Type>,
    ) -> Type {
        let empty = HashMap::new();
        let mut local = env.child();
        let mut ptys = Vec::new();
        let wanted = expected_params(expected, parameters.len());
        for (i, p) in parameters.iter().enumerate() {
            let ty = match &p.ty {
                Some(te) => type_expr_to_type(te, &empty),
                None => wanted.get(i).cloned().unwrap_or_else(|| self.ctx.fresh()),
            };
            local.insert(p.name.clone(), crate::ty::Scheme::mono(ty.clone()));
            ptys.push(ty);
        }
        // A lambda body runs when *called*, not where it is written: the
        // enclosing arm's continuation is not live inside it, so `resume`
        // there is the same hard error as at top level. Codegen already
        // clears its arm state across lambda boundaries (builder.rs) — this
        // keeps the checker in agreement so the program is rejected here,
        // with a type error, instead of deep in codegen. [EFFECTS-RESUME]
        let saved_resume_ctx = std::mem::take(&mut self.resume_ctx);
        // A lambda is a runtime VALUE with one ABI, not a definition the
        // backend re-specialises per call site, so its body never leaves an
        // arithmetic overload open ([`Self::defers`]). Where a lambda's slot
        // pins its parameters this changes nothing — they are already concrete
        // by the time the body is inferred.
        let saved_defer = std::mem::replace(&mut self.defer_arith, false);
        let body_ty = self.infer_expr(body, &local);
        self.defer_arith = saved_defer;
        self.resume_ctx = saved_resume_ctx;
        let ret = match return_type {
            Some(te) => {
                let r = type_expr_to_type(te, &empty);
                self.push_assign(&r, &body_ty);
                r
            }
            None => body_ty,
        };
        let fun = Type::fun(ptys, ret);
        // Publish this lambda's type for the backend, keyed by source position
        // (resolved against the final substitution in `infer_program`).
        if let Some(pos) = position {
            self.lambda_tys.push((pos, fun.clone()));
        }
        fun
    }

    fn infer_block(&mut self, statements: &[Stmt], value: Option<&Expr>, env: &TypeEnv) -> Type {
        let mut local = env.child();
        for s in statements {
            self.infer_block_stmt(s, &mut local);
        }
        match value {
            Some(v) => self.infer_expr(v, &local),
            None => Type::unit(),
        }
    }

    fn infer_constructor(
        &mut self,
        name: &str,
        type_args: &[osprey_ast::TypeExpr],
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
                        let written = crate::convert::type_expr_to_type(te, &binder);
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
            if is_record {
                Type::Record {
                    name: owner,
                    fields: dmap,
                }
            } else {
                Type::con(owner, args)
            }
        } else {
            // The grammar lowers a record update `rec { f: v }` over a
            // lower-cased binding as a constructor; recover it as an update
            // when the name resolves to an in-scope record.
            if env.get(name).is_some() {
                return self.infer_update(name, fields, env);
            }
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

    fn infer_update(&mut self, record: &str, fields: &[FieldAssignment], env: &TypeEnv) -> Type {
        let base = self.lookup_ident(record, env);
        let base_p = self.ctx.prune(&base);
        if let Type::Record { fields: rf, .. } = &base_p {
            let rf = rf.clone();
            for fa in fields {
                let vt = self.infer_expr(&fa.value, env);
                if let Some(dt) = rf.get(&fa.name) {
                    self.push_assign(&dt.clone(), &vt);
                }
            }
        } else {
            for fa in fields {
                let _ = self.infer_expr(&fa.value, env);
            }
        }
        base_p
    }
}

fn res_math_like(ok: Type) -> Type {
    Type::result(ok, generic_err())
}

fn both_vars(l: &Type, r: &Type) -> bool {
    matches!(l, Type::Var(_)) && matches!(r, Type::Var(_))
}

/// Marks a pending arithmetic overload in the obligation list
/// ([`Checker::deferred_arith`]). Obligations already carry per-site types
/// through generalization and instantiation, and an operator can never collide
/// with a built-in's name, so a deferred overload rides that list rather than a
/// parallel one.
const DEFERRED_ARITH: &str = "arith ";

fn deferred_arith_name(op: &str) -> String {
    format!("{DEFERRED_ARITH}{op}")
}

/// The operator a deferred-arithmetic obligation name carries; `None` for a
/// built-in's obligation.
fn parse_deferred_arith(name: &str) -> Option<&str> {
    name.strip_prefix(DEFERRED_ARITH)
}

/// Whether an obligation marks a pending arithmetic overload rather than a
/// built-in's representation constraint.
pub(crate) fn is_deferred_arith(name: &str) -> bool {
    parse_deferred_arith(name).is_some()
}

/// Operator → result type. Lives free of `self` so the borrow checker is happy.
fn unwrap_result(t: &Type) -> Type {
    match t {
        Type::Con { name, args } if name == names::RESULT => {
            args.first().cloned().unwrap_or_else(|| t.clone())
        }
        _ => t.clone(),
    }
}

fn is_result(t: &Type) -> bool {
    t.is_named(names::RESULT)
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

/// Arithmetic propagation is intentionally a single `MathError` channel. A
/// Result carrying any other error type must be handled before arithmetic so
/// its identity cannot be silently relabelled.
fn result_error(t: &Type) -> Option<Type> {
    match t {
        Type::Con { name, args } if name == names::RESULT => args.get(1).cloned(),
        _ => None,
    }
}

impl Checker {
    fn infer_binary(&mut self, op: &str, left: &Expr, right: &Expr, env: &TypeEnv) -> Type {
        let lt = self.infer_expr(left, env);
        let rt = self.infer_expr(right, env);
        match classify(op) {
            OpKind::Logical => {
                self.push_assign(&Type::bool(), &lt);
                self.push_assign(&Type::bool(), &rt);
                Type::bool()
            }
            OpKind::Comparison => {
                let l = self.ctx.prune(&lt);
                let r = self.ctx.prune(&rt);
                if is_result(&l) || is_result(&r) {
                    self.errors.push(TypeError::new(
                        "cannot compare a `Result` directly; handle it explicitly with `match` or `?:`",
                    ));
                } else {
                    let _ = unify(&mut self.ctx, &l, &r);
                }
                Type::bool()
            }
            OpKind::Arith => self.infer_arith(op, &lt, &rt),
        }
    }

    fn infer_arith(&mut self, op: &str, lt: &Type, rt: &Type) -> Type {
        let l = self.ctx.prune(lt);
        let r = self.ctx.prune(rt);
        // Arithmetic is the sole failure-preserving Result flattening context:
        // inspect an operand's success type to choose the operator overload, but
        // keep one outer Result whenever either operand already has an error
        // channel. No other value context may erase that channel.
        let propagates_error = is_result(&l) || is_result(&r);
        if let Some(error) = result_error(&l) {
            self.push_unify(&math_err(), &error);
        }
        if let Some(error) = result_error(&r) {
            self.push_unify(&math_err(), &error);
        }
        let lu = unwrap_result(&l);
        let ru = unwrap_result(&r);
        match op {
            "%" if lu.is_named(names::FLOAT) || ru.is_named(names::FLOAT) => {
                res_math(Type::float())
            }
            "%" if self.defers(&lu, &ru) => self.deferred_arith(op, &l, &r, &lu, &ru),
            "%" => {
                self.push_unify(&Type::int(), &lu);
                self.push_unify(&Type::int(), &ru);
                res_math(Type::int())
            }
            "/" => res_math(Type::float()),
            "+" => {
                let total = if lu.is_named(names::STRING) || ru.is_named(names::STRING) {
                    self.push_unify(&Type::string(), &lu);
                    self.push_unify(&Type::string(), &ru);
                    Type::string()
                } else if lu.is_named(names::FLOAT) || ru.is_named(names::FLOAT) {
                    Type::float()
                } else if lu.is_named(names::LIST) {
                    let _ = unify(&mut self.ctx, &lu, &ru);
                    lu
                } else if ru.is_named(names::LIST) {
                    let _ = unify(&mut self.ctx, &lu, &ru);
                    ru
                } else if lu.is_named(names::MAP) || ru.is_named(names::MAP) {
                    let _ = unify(&mut self.ctx, &lu, &ru);
                    if lu.is_named(names::MAP) {
                        lu
                    } else {
                        ru
                    }
                } else if self.defers(&lu, &ru) {
                    return self.deferred_arith(op, &l, &r, &lu, &ru);
                } else {
                    return self.int_arithmetic_result(&lu, &ru);
                };
                if propagates_error {
                    res_math(total)
                } else {
                    total
                }
            }
            // Unlike `+`, `-` and `*` have no string/list overload; their
            // unconstrained form still defers, and resolves without the string
            // case ([`Checker::resolve_deferred_arith`]).
            _ => {
                if self.defers(&lu, &ru) {
                    self.deferred_arith(op, &l, &r, &lu, &ru)
                } else if lu.is_named(names::FLOAT) || ru.is_named(names::FLOAT) {
                    if propagates_error {
                        res_math(Type::float())
                    } else {
                        Type::float()
                    }
                } else {
                    self.int_arithmetic_result(&lu, &ru)
                }
            }
        }
    }

    /// Whether an arithmetic site with these (unwrapped) operands may still
    /// leave its overload open. Two unconstrained variables can; and only while
    /// inference is still running — the resolution pass re-enters
    /// [`Self::infer_arith`] with deferral CLOSED, so a site nothing ever
    /// constrained lands on the integer default there.
    fn defers(&self, left: &Type, right: &Type) -> bool {
        self.defer_arith && both_vars(left, right)
    }

    /// Type an arithmetic operator whose BOTH operands are still unconstrained
    /// variables, without committing to the integer overload.
    ///
    /// Eagerly defaulting here is what made a named numeric helper unusable as
    /// a float kernel: `fn plus(a, x) = a + x` is checked before anything says
    /// what `a` is, so it became `(int, int) -> Result<int, MathError>` and
    /// `gpuFold(0.0, plus)` over a float buffer was rejected with `cannot unify
    /// int with float` ([GPU-KERNEL-ELEM-TYPING], [GPU-KERNEL-FORM]). A lambda
    /// in the same slot already works, because its parameters are pinned from
    /// the slot before its body is inferred ([`Self::positional_arg_types`]);
    /// this is the same rule for the named form.
    ///
    /// Both operands unify with one another and the result is left OPEN. The
    /// overload is picked once, after all unification, by re-running the very
    /// same selection over the operands' final types
    /// ([`Self::resolve_deferred_arith`]) — so the pending site records the
    /// operands AS WRITTEN, error channel included, and not the unwrapped pair
    /// the selection happens to compare.
    ///
    /// An operand carrying a pending overload does NOT generalize
    /// ([`Checker::generalize_with_obligations`]): with no numeric class to
    /// quantify over, one definition gets ONE overload, chosen by how the
    /// program actually uses it. `fn plus(a, x) = a + x` folded over a float
    /// buffer is a float addition; the same helper used only on integers is
    /// still the checked integer one; using it at both in a single program is a
    /// type error rather than a silent reinterpretation.
    fn deferred_arith(&mut self, op: &str, l: &Type, r: &Type, lu: &Type, ru: &Type) -> Type {
        self.push_unify(lu, ru);
        let result = self.ctx.fresh();
        self.builtin_uses.push((
            deferred_arith_name(op),
            Type::fun(vec![l.clone(), r.clone()], result.clone()),
        ));
        result
    }

    /// Settle one deferred site: re-run the ordinary overload selection over
    /// the operands' final types with deferral closed, and tie the site's open
    /// result to the answer. Re-running rather than restating the rules is what
    /// keeps `p.x * p.x + p.y * p.y` right — by the time the outer `+` resolves,
    /// its operands have become `Result<int, MathError>`, and only the real
    /// selection knows to unwrap them and keep one flattened error channel.
    pub(crate) fn resolve_deferred_arith(&mut self, name: &str, site: &Type) {
        let Some(op) = parse_deferred_arith(name) else {
            return;
        };
        let Type::Fun { params, ret } = self.ctx.apply(site) else {
            return;
        };
        let [left, right] = params.as_slice() else {
            return;
        };
        let answer = self.infer_arith(op, left, right);
        self.push_unify(&answer, &ret);
    }

    /// Constrain both operands to integers and preserve overflow as a typed
    /// failure. Integer `+`, `-`, and `*` can all exceed the i64 range.
    fn int_arithmetic_result(&mut self, left: &Type, right: &Type) -> Type {
        self.push_unify(&Type::int(), left);
        self.push_unify(&Type::int(), right);
        res_math(Type::int())
    }

    /// Integer negation fails for `INT64_MIN`; float negation is total. A
    /// Result operand is inspected only to propagate its existing failure into
    /// the one flattened arithmetic Result.
    fn infer_negation(&mut self, operand: &Type) -> Type {
        let operand = self.ctx.prune(operand);
        let propagates_error = is_result(&operand);
        if let Some(error) = result_error(&operand) {
            self.push_unify(&math_err(), &error);
        }
        let inner = unwrap_result(&operand);
        if inner.is_named(names::FLOAT) {
            if propagates_error {
                res_math(Type::float())
            } else {
                Type::float()
            }
        } else {
            self.push_unify(&Type::int(), &inner);
            res_math(Type::int())
        }
    }
}

enum OpKind {
    Arith,
    Comparison,
    Logical,
}

fn classify(op: &str) -> OpKind {
    match op {
        "&&" | "||" => OpKind::Logical,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => OpKind::Comparison,
        _ => OpKind::Arith,
    }
}

#[cfg(test)]
mod tests {
    use crate::check::check_program;
    use crate::testutil::{bad, check, ok};
    use crate::{infer_program, Type};
    use osprey_syntax::parse_program;

    #[test]
    fn pipe_into_call_and_bare_function() {
        // Call form: `x |> f(a)` prepends `x`. Bare form: `x |> f` applies `f(x)`.
        ok("fn add(a: int, b: int) -> Result<int, MathError> = a + b\n\
            fn inc(n: int) -> Result<int, MathError> = n + 1\n\
            let r = 10 |> add(5)\n\
            let s = 10 |> inc\n");
    }

    #[test]
    fn fused_iterator_functions_reject_materialized_lists() {
        ok("range(0, 3) |> map(fn(x) => x + 1) |> forEach(print)\n");
        let errs = bad("[1, 2, 3] |> map(fn(x) => x + 1) |> forEach(print)\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("Iterator") && e.message.contains("List")));
    }

    #[test]
    fn covers_every_simple_expression_form() {
        // The parser only emits many `Expr` arms from real source, so one program
        // mixes float/string/bool/interpolation/list/map/object/unary/field-access/
        // index/lambda/block/spawn/await/channel send+recv/yield/perform.
        ok("type Box = { v: int }\n\
            effect Logger { log: fn(string) -> Unit }\n\
            fn other() -> int = 7\n\
            fn demo() -> Unit !Logger = {\n\
              let f = 3.14\n\
              let s = \"hi\"\n\
              let b = true\n\
              let count = 5\n\
              let i = \"val=${count}\"\n\
              let xs = [1, 2, 3]\n\
              let m = { \"a\": 1, \"b\": 2 }\n\
              let obj = { x: 1, y: 2 }\n\
              let neg = -5\n\
              let no = !b\n\
              let bx = Box { v: 9 }\n\
              let fx = bx.v\n\
              let first = xs[0]\n\
              let g = fn(n) => n + 1\n\
              let fib = spawn other()\n\
              let r = await(fib)\n\
              let ch = Channel(1)\n\
              send(ch, 42)\n\
              let got = recv(ch)\n\
              yield\n\
              perform Logger.log(\"hello\")\n\
            }\n");
    }

    #[test]
    fn select_is_rejected_until_channel_selection_has_runtime_semantics() {
        let errs = bad("fn pick() -> int = select {\n\
              x => x\n\
              _ => 0\n\
            }\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("`select` is not supported")));
    }

    #[test]
    fn yield_forwards_its_value_type_and_send_checks_the_channel_element() {
        // Implements [CONCURRENCY-YIELD] and [CONCURRENCY-CHANNEL].
        ok("fn hand_off(value: int) -> int = yield value\n");
        let errs = bad("fn wrong(ch: Channel<int>) -> Unit = send(ch, \"wrong\")\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("cannot unify int with string")));
        let result_channel = bad(
            "fn sendFailed(ch: Channel<Result<int, MathError>>, value: Result<int, MathError>) -> Unit = send(ch, value)\n",
        );
        assert!(result_channel
            .iter()
            .any(|e| e.message.contains("Result-valued channels")));
        // The receiving end is gated for the same reason: handing the Result
        // wrapper back out of `recv` would erase it silently.
        let result_recv = bad(
            "fn recvFailed(ch: Channel<Result<int, MathError>>) -> Result<int, MathError> = recv(ch)\n",
        );
        assert!(result_recv
            .iter()
            .any(|e| e.message.contains("Result-valued channels")));
    }

    #[test]
    fn a_unit_operation_arm_may_not_swallow_a_result() {
        // A `Unit` operation discards its arm's value, so a `Result` produced
        // there would lose its failure with nothing left to observe it.
        // Implements [EFFECTS-RESUME].
        let errs = bad("effect Sink { drop: fn(int) -> Unit }\n\
                        fn risky(n: int) -> Result<int, MathError> = n + 1\n\
                        fn go() -> int = handle Sink\n\
                          drop v => risky(v)\n\
                        in 0\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("Unit effect operation arm")));
        ok("effect Sink { drop: fn(int) -> Unit }\n\
            fn risky(n: int) -> Result<int, MathError> = n + 1\n\
            fn go() -> int = handle Sink\n\
              drop v => { let handled = risky(v) ?: 0 }\n\
            in 0\n");
    }

    #[test]
    fn handling_an_undeclared_effect_names_it_and_keeps_checking_the_arms() {
        // The handler's own operations are unknown, so the arms cannot be
        // typed against a signature — but checking must continue rather than
        // abandon the body, or one typo would hide every later diagnostic.
        let errs = bad("fn go() -> int = handle Nowhere\n\
                          chime => 0\n\
                        in 1\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown effect `Nowhere`")));
    }

    #[test]
    fn perform_of_an_undeclared_effect_names_the_effect_it_could_not_find() {
        let errs = bad("fn ring() -> int = perform Nowhere.chime()\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown effect `Nowhere`")));
    }

    #[test]
    fn a_generic_constructor_checks_its_type_argument_arity() {
        // Implements [GENERICS-CTOR-ARITY]. Writing the arguments out is a
        // contract with the declaration, so a mismatch is an error rather than
        // a silently ignored annotation.
        ok("type Box<T> = { v: T }\n\
            let good = Box<int> { v: 1 }\n");
        let errs = bad("type Box<T> = { v: T }\n\
                        let wrong = Box<int, string> { v: 1 }\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("takes 1 type argument(s), got 2")));
    }

    #[test]
    fn a_qualified_path_callee_is_looked_up_under_its_whole_name() {
        // `infer_callee`'s path arm looks up `a::b` as ONE name rather than
        // resolving `a` and projecting. A single file has no assembled project
        // graph to bind the qualified name, so the diagnostic must name the
        // full path — naming only `twice` would send the reader to the wrong
        // declaration.
        let errs = bad("namespace tools;\n\
                        fn twice(n: int) -> int = (n * 2) ?: 0\n\
                        let doubled = tools::twice(21)\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown identifier `tools::twice`")));
    }

    #[test]
    fn handler_expressions_type_their_arms() {
        ok("effect Logger { log: fn(string) -> Unit }\n\
            fn run() -> int = handle Logger\n\
              log msg => 0\n\
            in 42\n");
        // Resume feeds the operation-result slot and the handled body feeds the
        // handler-answer slot. Guards the [EFFECTS-RESUME] fix.
        ok("effect Guard { check: fn(int) -> int }\n\
            fn guarded() -> int = handle Guard\n\
              check v => match v < 100 {\n\
                true => resume(v)\n\
                false => 0\n\
              }\n\
            in {\n\
              let a = perform Guard.check(5)\n\
              a\n\
            }\n");
    }

    #[test]
    fn performed_effect_operation_must_be_declared() {
        // [EFFECTS-OP-TYPING]
        let missing = bad("effect Logger { log: fn(string) -> Unit }\n\
             fn run() -> Unit !Logger = perform Logger.missing(\"hi\")\n");
        assert!(missing
            .iter()
            .any(|e| e.message.contains("has no operation `missing`")));
    }

    #[test]
    fn performed_effect_must_be_declared() {
        let errors = bad("fn run() -> Unit !Missing = perform Missing.log(\"hi\")\n");
        assert!(errors
            .iter()
            .any(|e| e.message.contains("unknown effect `Missing`")));
    }

    #[test]
    fn performed_effect_operation_must_match_declared_arity() {
        let wrong_arity = bad("effect Logger { log: fn(string) -> Unit }\n\
             fn run() -> Unit !Logger = perform Logger.log()\n");
        assert!(wrong_arity
            .iter()
            .any(|e| e.message.contains("expects 1 argument(s), got 0")));
    }

    #[test]
    fn handler_arm_operation_must_be_declared() {
        let bad_arm = bad("effect Logger { log: fn(string) -> Unit }\n\
             let result = handle Logger\n\
               missing msg => {}\n\
             in {}\n");
        assert!(bad_arm
            .iter()
            .any(|e| e.message.contains("has no operation `missing`")));
    }

    #[test]
    fn handler_arm_operation_must_match_declared_arity() {
        let errors = bad("effect Logger { log: fn(string) -> Unit }\n\
             let result = handle Logger\n\
               log => {}\n\
             in {}\n");
        assert!(errors
            .iter()
            .any(|e| e.message.contains("expects 1 parameter(s), got 0")));
    }

    #[test]
    fn record_update_on_record_and_field_assign() {
        ok("type Point = { x: int, y: int }\n\
            let p = Point { x: 1, y: 2 }\n\
            let q = p { x: 10 }\n");
    }

    #[test]
    fn pipe_and_update_ast_nodes() {
        // The parser desugars `|>` into a `Call` and record-update `r { f }` into a
        // `TypeConstructor`, so `Expr::Pipe`/`Expr::Update` are built directly.
        use osprey_ast::{Expr, FieldAssignment, Parameter, Program, Stmt, TypeExpr};
        let inc = Stmt::Function {
            name: "inc".into(),
            type_params: Vec::new(),
            parameters: vec![Parameter {
                name: "n".into(),
                ty: Some(TypeExpr::named("int")),
            }],
            return_type: Some(TypeExpr::named("int")),
            body: Expr::Identifier("n".into()),
            effects: Vec::new(),
            doc: None,
            position: None,
        };
        // Pipe, non-call form: `10 |> inc` applies `inc(10)`.
        let bare_pipe = Stmt::Expr {
            value: Expr::Pipe {
                left: Box::new(Expr::Integer(10)),
                right: Box::new(Expr::Identifier("inc".into())),
            },
            doc: None,
            position: None,
        };
        // Pipe, call form: `10 |> inc(0)` prepends `10`, becoming `inc(10, 0)`
        // (an arity mismatch — but the call-form branch is what we exercise).
        let call_pipe = Stmt::Expr {
            value: Expr::Pipe {
                left: Box::new(Expr::Integer(10)),
                right: Box::new(Expr::Call {
                    function: Box::new(Expr::Identifier("inc".into())),
                    arguments: vec![Expr::Integer(0)],
                    named_arguments: Vec::new(),
                }),
            },
            doc: None,
            position: None,
        };
        // `Expr::Update` over a non-record binding hits the else arm of
        // `infer_update` (the field values are still inferred).
        let update = Stmt::Expr {
            value: Expr::Update {
                record: "n".into(),
                fields: vec![FieldAssignment {
                    name: "x".into(),
                    value: Expr::Integer(1),
                }],
            },
            doc: None,
            position: None,
        };
        let prog = Program {
            statements: vec![
                inc,
                Stmt::Let {
                    name: "n".into(),
                    mutable: false,
                    ty: None,
                    value: Expr::Integer(2),
                    doc: None,
                    position: None,
                },
                bare_pipe,
                call_pipe,
                update,
            ],
        };
        // Only the deliberate pipe arity mismatch is expected.
        let errs = check_program(&prog);
        assert!(
            errs.iter().all(|e| e.message.contains("arity")),
            "unexpected errors: {errs:?}"
        );
    }

    #[test]
    fn nullary_record_constructor_used_as_value() {
        // `type Foo = Foo` is a single-variant record whose constructor has no
        // fields: the bare name `Foo` is a record value (the empty-record arm).
        ok("type Foo = Foo\n\
            let x = Foo\n");
    }

    #[test]
    fn method_call_with_positional_and_named_args() {
        // The parser lowers `t.m(..)` to a plain `Call`, so an `Expr::MethodCall`
        // is built directly to exercise `infer_method_call`'s arg loops.
        use osprey_ast::{Expr, NamedArgument, Parameter, Program, Stmt, TypeExpr};
        let int_param = |name: &str| Parameter {
            name: name.into(),
            ty: Some(TypeExpr::named("int")),
        };
        let body = Expr::MethodCall {
            target: Box::new(Expr::Integer(1)),
            method: "combine".into(),
            arguments: vec![Expr::Integer(2)],
            named_arguments: vec![NamedArgument {
                name: "third".into(),
                value: Expr::Integer(3),
            }],
        };
        let prog = Program {
            statements: vec![Stmt::Function {
                name: "combine".into(),
                type_params: Vec::new(),
                parameters: vec![int_param("self"), int_param("other"), int_param("third")],
                return_type: Some(TypeExpr::named("int")),
                body: Expr::Identifier("self".into()),
                effects: Vec::new(),
                doc: None,
                position: None,
            }],
        };
        // The function's signature pass registers `combine`; the MethodCall is a
        // bare top-level expression statement that drives `infer_method_call`.
        let mut stmts = prog.statements;
        stmts.push(Stmt::Expr {
            value: body,
            doc: None,
            position: None,
        });
        let errs = check_program(&Program { statements: stmts });
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    }

    #[test]
    fn named_args_reorder_and_fall_back() {
        // Reorder succeeds when every name matches a parameter.
        ok("fn mk(a: int, b: string) -> int = a\n\
            let r = mk(b: \"x\", a: 1)\n");
        // A named call to an unknown function still type-checks its args (the
        // fallback that maps the named args positionally).
        ok("let f = fn(a) => a\n\
            let r = f(a: 7)\n");
    }

    #[test]
    fn index_string_and_unknown_target() {
        // String index yields Result<string, _>; an opaque target falls back to
        // a fresh Result.
        ok("fn ch(s: string) -> Result<string, Error> = s[0]\n\
            fn anyIdx(x) = x[0]\n");
    }

    #[test]
    fn arith_list_map_concat_and_float_subtraction() {
        // `+` over lists and maps unifies operands.
        ok("let xs = [1, 2] + [3, 4]\n\
            fn fsub(a: float, b: float) -> float = a - b\n");
    }

    #[test]
    fn map_literals_and_public_operations_reject_non_string_keys() {
        let literal_errors = bad("let m = { 1: \"one\" }\n");
        assert!(!literal_errors.is_empty());

        let operation_errors = bad("let m = mapSet(Map(), 1, \"one\")\n");
        assert!(!operation_errors.is_empty());
    }

    #[test]
    fn natural_arithmetic_infers_one_result() {
        let parsed = parse_program(
            "fn intCalc(a: int, b: int, c: int) = (a + b) * c - 1\n\
             fn floatCalc(a: float, b: float, c: float) = (a + b) * c - 1.0\n\
             fn mixedChain(a: int, b: int, c: float) = (a + b) + c\n\
             fn inferredIntAdd(a, b) = a + b\n\
             fn intNeg(n: int) = -n\n\
             fn floatNeg(n: float) = -n\n",
        );
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        let types = infer_program(&parsed.program);
        let result = |inner| Type::result(inner, Type::prim("MathError"));
        assert_eq!(types.return_type("intCalc"), Some(&result(Type::int())));
        assert_eq!(types.return_type("floatCalc"), Some(&Type::float()));
        assert_eq!(
            types.return_type("mixedChain"),
            Some(&result(Type::float()))
        );
        assert_eq!(
            types.return_type("inferredIntAdd"),
            Some(&result(Type::int()))
        );
        assert_eq!(types.return_type("intNeg"), Some(&result(Type::int())));
        assert_eq!(types.return_type("floatNeg"), Some(&Type::float()));

        let errors = bad("fn wrong(r: Result<int, Error>) = r + 1\n");
        assert!(errors
            .iter()
            .any(|e| { e.message.contains("MathError") && e.message.contains("Error") }));
    }

    #[test]
    fn interpolation_preserves_the_complete_result() {
        ok("fn show(a: int, b: int) -> string = \"sum=${a + b}\"\n");
    }

    #[test]
    fn perform_named_arguments_are_rejected_before_codegen() {
        // Codegen has no named-operation argument mapping; reject this source
        // form instead of silently dropping its value.
        // [EFFECTS-OP-TYPING]
        let errs = bad("effect Logger { log: fn(string) -> Unit }\n\
             fn run() -> Unit !Logger = perform Logger.log(msg: \"hi\")\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("does not support named arguments")));
    }

    #[test]
    fn nullary_union_variant_used_as_value() {
        // A bare nullary *union* variant (`Red`) is a value of its owner type —
        // the non-record `Type::con(owner, args)` arm of `lookup_ident`.
        ok("type Color = Red | Green | Blue\n\
            let c = Red\n");
    }

    #[test]
    fn comparison_modulo_division_and_float_arith() {
        ok("fn lt(a: int, b: int) -> bool = a < b\n\
            fn md(a: int, b: int) -> Result<int, MathError> = a % b\n\
            fn dv(a: int, b: int) -> Result<float, MathError> = a / b\n\
            fn fadd(a: float, b: float) -> float = a + b\n\
            fn fmul(a: float, b: float) -> float = a * b\n");
    }

    #[test]
    fn list_concat_when_only_right_is_a_list() {
        // `+` where the left operand starts unconstrained and the right is a
        // known list ties them and yields the list type (the r-side list arm).
        ok("fn cat(a, b: List<int>) = a + b\n");
    }

    #[test]
    fn map_index_yields_value_result() {
        ok("fn lookup(m: Map<string, int>) -> Result<int, Error> = m[\"k\"]\n");
        assert!(!bad("fn bad(m: Map<string, int>) = m[1 + 1]\n").is_empty());
        assert!(!bad("fn bad(xs: List<int>) = xs[1 + 1]\n").is_empty());
    }

    #[test]
    fn map_concatenation_unifies_operands() {
        ok("fn merge(a: Map<string, int>, b: Map<string, int>) -> Map<string, int> = a + b\n");
    }

    #[test]
    fn calling_an_unannotated_param_constrains_it_to_a_function() {
        // `g` is an unannotated parameter (an unbound var); calling it drives the
        // `apply_fn` Var branch that synthesises a function shape.
        ok("fn apply(g, x) = g(x)\n");
    }

    #[test]
    fn unknown_constructor_with_fields_is_an_error() {
        let errs = check("let r = Nonexistent { field: 1 }\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown constructor `Nonexistent`")));
    }

    #[test]
    fn comparison_over_results_requires_explicit_handling() {
        let errs = bad("fn cmp(a: int, b: int) -> bool = (a % b) == (b % a)\n");
        assert!(errs.iter().any(|e| {
            e.message.contains("cannot compare a `Result` directly")
                && e.message.contains("match")
                && e.message.contains("?:")
        }));
        ok("fn cmp(a: int, b: int) -> bool = ((a % b) ?: 0) == ((b % a) ?: 0)\n");
    }

    #[test]
    fn calling_a_non_identifier_and_a_non_function() {
        // Calling the result of a lambda expression directly: the callee is not a
        // bare identifier, so `infer_call` takes the `other` branch.
        ok("let r = (fn(x) => x + 1)(41)\n");
        // Calling a non-function value is an error (`apply_fn` non-function arm).
        let errs = check("let x = 5\nlet r = x(1)\n");
        assert!(errs.iter().any(|e| e.message.contains("cannot call")));
    }

    #[test]
    fn lambda_with_param_and_return_annotations() {
        ok("let f = fn(x: int) -> int => (x + 1) ?: 0\n\
            let r = f(10)\n");
    }

    #[test]
    fn lowercase_record_update_via_constructor_syntax() {
        // The grammar lowers `rec { f: v }` over an in-scope lower-cased binding
        // as a constructor; `infer_constructor` recovers it as an update.
        ok("type Point = { x: int, y: int }\n\
            fn shift(p: Point) -> Point = p { x: 99 }\n");
    }
}
