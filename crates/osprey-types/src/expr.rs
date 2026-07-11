//! Expression inference. One `infer_expr` dispatch covers every `ast::Expr`.
//! Where a type genuinely cannot be resolved (an opaque field access, an
//! unknown dynamic builtin) the inferencer yields a fresh variable rather than
//! a false error; the structured cases (calls, arithmetic, constructors,
//! lambdas, match) do real unification.

use crate::check::Checker;
use crate::convert::type_expr_to_type;
use crate::env::{instantiate, TypeEnv};
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
                        let _ = self.infer_expr(inner, env);
                    }
                }
                Type::string()
            }
            Expr::Identifier(name) => self.lookup_ident(name, env),
            Expr::List(items) => {
                let elem = self.ctx.fresh();
                for it in items {
                    let t = self.infer_expr(it, env);
                    self.push_unify(&elem, &t);
                }
                Type::list(elem)
            }
            Expr::Map(entries) => self.infer_map(entries, env),
            Expr::Object(fields) => self.infer_object(fields, env),
            Expr::Binary { op, left, right } => self.infer_binary(op, left, right, env),
            Expr::Unary { op, operand } => {
                let t = self.infer_expr(operand, env);
                if op == "!" || op == "not" {
                    self.push_assign(&Type::bool(), &t);
                    Type::bool()
                } else {
                    // numeric negation keeps the operand type (int or float)
                    t
                }
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.infer_call(function, arguments, named_arguments, env),
            Expr::Pipe { left, right } => self.infer_pipe(left, right, env),
            Expr::FieldAccess { target, field } => {
                let tt = self.infer_expr(target, env);
                let tp = self.ctx.prune(&tt);
                match &tp {
                    Type::Record { fields, .. } => fields
                        .get(field)
                        .cloned()
                        .unwrap_or_else(|| self.ctx.fresh()),
                    _ => self.ctx.fresh(),
                }
            }
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
            Expr::Recv(channel) => self.infer_unwrap_con(channel, names::CHANNEL, env),
            Expr::Send { channel, value } => {
                let _ = self.infer_expr(channel, env);
                let _ = self.infer_expr(value, env);
                Type::unit()
            }
            Expr::Yield(inner) => {
                if let Some(inner) = inner {
                    let _ = self.infer_expr(inner, env);
                }
                Type::unit()
            }
            Expr::Select { arms } => self.infer_arm_bodies(arms, env),
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
            } => self.infer_perform(effect, operation, arguments, named_arguments, *position, env),
            Expr::Handler {
                effect,
                arms,
                body,
                position,
            } => self.infer_handler(effect, arms, body, *position, env),
            other => self.infer_expr(other, env),
        }
    }

    /// Infer `resume(v)`: its argument is delivered as the operation's result, so
    /// it lands in the op-result slot at an assignment site — a bare
    /// `Result<T, E>` (e.g. from arithmetic) auto-unwraps into a concrete `T`,
    /// exactly as function returns and `let`/`mut` assignments do. The expression
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

    /// Infer a map literal: unify all keys to one type and all values to another.
    fn infer_map(&mut self, entries: &[osprey_ast::MapEntry], env: &TypeEnv) -> Type {
        let (k, v) = (self.ctx.fresh(), self.ctx.fresh());
        for entry in entries {
            let kt = self.infer_expr(&entry.key, env);
            let vt = self.infer_expr(&entry.value, env);
            self.push_unify(&k, &kt);
            self.push_unify(&v, &vt);
        }
        Type::map(k, v)
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
    /// Implements [EFFECTS-GENERIC-INSTANTIATION].
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
        let scope = self
            .handler_scopes
            .iter()
            .rev()
            .find(|s| s.name == effect)
            .map(|s| (s.args.clone(), s.ops.clone()))
            .or_else(|| self.effect_instance_ops(effect));
        let Some((eff_args, ops)) = scope else {
            return self.ctx.fresh();
        };
        let Some(op) = ops.get(operation).cloned() else {
            return self.ctx.fresh();
        };
        if op.params.len() == arg_tys.len() {
            for (p, a) in op.params.iter().zip(&arg_tys) {
                self.push_assign(p, a);
            }
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
    /// answer type; the body lands in the answer slot at an assignment site,
    /// so a bare `Result<T, E>` body auto-unwraps into a concrete answer just
    /// as function returns do. Implements [EFFECTS-RESUME] and
    /// [EFFECTS-GENERIC-INSTANTIATION].
    fn infer_handler(
        &mut self,
        effect: &str,
        arms: &[osprey_ast::HandlerArm],
        body: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        let (eff_args, inst_ops) = self.effect_instance_ops(effect).unwrap_or_default();
        let answer = self.ctx.fresh();
        for arm in arms {
            let (params, op_ret) = match inst_ops.get(&arm.operation) {
                Some(op) if op.params.len() == arm.params.len() => {
                    (op.params.clone(), op.ret.clone())
                }
                _ => (
                    (0..arm.params.len()).map(|_| self.ctx.fresh()).collect(),
                    self.ctx.fresh(),
                ),
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
            return instantiate(&mut self.ctx, &scheme);
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
        let (fname, ft) = match function {
            Expr::Identifier(n) => (Some(n.clone()), self.lookup_ident(n, env)),
            other => (None, self.infer_expr(other, env)),
        };
        let args = self.ordered_arg_types(fname.as_deref(), arguments, named, env);
        self.apply_fn(&ft, args)
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
        self.apply_fn(&ft, args)
    }

    /// Resolve call arguments to types, reordering named arguments to the
    /// declared parameter order when the callee is a known function.
    fn ordered_arg_types(
        &mut self,
        fname: Option<&str>,
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
        arguments.iter().map(|a| self.infer_expr(a, env)).collect()
    }

    fn apply_fn(&mut self, ft: &Type, args: Vec<Type>) -> Type {
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
                    self.push_assign(p, a);
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
            let ft = self.infer_expr(right, env);
            let lt = self.infer_expr(left, env);
            self.apply_fn(&ft, vec![lt])
        }
    }

    fn infer_index(&mut self, target: &Expr, index: &Expr, env: &TypeEnv) -> Type {
        let tt = self.infer_expr(target, env);
        let _ = self.infer_expr(index, env);
        match self.ctx.prune(&tt) {
            Type::Con { name, args } if name == names::LIST && !args.is_empty() => {
                res_math_like(args.first().cloned().unwrap_or_else(|| self.ctx.fresh()))
            }
            Type::Con { name, args } if name == names::MAP && args.len() == 2 => {
                res_math_like(args.get(1).cloned().unwrap_or_else(|| self.ctx.fresh()))
            }
            t if t.is_named(names::STRING) => res_math_like(Type::string()),
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
        let empty = HashMap::new();
        let mut local = env.child();
        let mut ptys = Vec::new();
        for p in parameters {
            let ty = match &p.ty {
                Some(te) => type_expr_to_type(te, &empty),
                None => self.ctx.fresh(),
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
        let body_ty = self.infer_expr(body, &local);
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

/// Operator → result type. Lives free of `self` so the borrow checker is happy.
fn unwrap_result(t: &Type) -> Type {
    match t {
        Type::Con { name, args } if name == names::RESULT => {
            args.first().cloned().unwrap_or_else(|| t.clone())
        }
        _ => t.clone(),
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
                let lu = unwrap_result(&self.ctx.prune(&lt));
                let ru = unwrap_result(&self.ctx.prune(&rt));
                let _ = unify(&mut self.ctx, &lu, &ru);
                Type::bool()
            }
            OpKind::Arith => self.infer_arith(op, &lt, &rt),
        }
    }

    fn infer_arith(&mut self, op: &str, lt: &Type, rt: &Type) -> Type {
        let l = self.ctx.prune(lt);
        let r = self.ctx.prune(rt);
        match op {
            "%" => {
                self.push_assign(&Type::int(), lt);
                self.push_assign(&Type::int(), rt);
                res_math(Type::int())
            }
            "/" => res_math(Type::float()),
            "+" => {
                if l.is_named(names::STRING) || r.is_named(names::STRING) {
                    self.push_assign(&Type::string(), lt);
                    self.push_assign(&Type::string(), rt);
                    Type::string()
                } else if l.is_named(names::FLOAT) || r.is_named(names::FLOAT) {
                    res_math(Type::float())
                } else if l.is_named(names::LIST) {
                    let _ = unify(&mut self.ctx, lt, rt);
                    l
                } else if r.is_named(names::LIST) {
                    let _ = unify(&mut self.ctx, lt, rt);
                    r
                } else if l.is_named(names::MAP) || r.is_named(names::MAP) {
                    let _ = unify(&mut self.ctx, lt, rt);
                    if l.is_named(names::MAP) {
                        l
                    } else {
                        r
                    }
                } else if both_vars(&l, &r) {
                    // Both operands unconstrained: defer (`+` is overloaded over
                    // int/float/string/list). Tie them and yield a fresh result
                    // so usage context can pick the type.
                    let _ = unify(&mut self.ctx, lt, rt);
                    self.ctx.fresh()
                } else {
                    self.push_assign(&Type::int(), lt);
                    self.push_assign(&Type::int(), rt);
                    res_math(Type::int())
                }
            }
            // "-" and "*": unlike "+", these have no string/list overload, so
            // unconstrained operands default to int — `fn square(v) = v * v`
            // infers `(int) -> Result<int, MathError>`.
            _ => {
                if l.is_named(names::FLOAT) || r.is_named(names::FLOAT) {
                    res_math(Type::float())
                } else {
                    self.push_assign(&Type::int(), lt);
                    self.push_assign(&Type::int(), rt);
                    res_math(Type::int())
                }
            }
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
    use crate::testutil::{check, ok};

    #[test]
    fn pipe_into_call_and_bare_function() {
        // Call form: `x |> f(a)` prepends `x`. Bare form: `x |> f` applies `f(x)`.
        ok("fn add(a: int, b: int) -> int = a + b\n\
            fn inc(n: int) -> int = n + 1\n\
            let r = 10 |> add(5)\n\
            let s = 10 |> inc\n");
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
    fn select_and_handler_expressions() {
        // `select { ... }` and `handle E op => .. in body` both type their arms.
        ok("fn pick() -> int = select {\n\
              x => x\n\
              _ => 0\n\
            }\n");
        ok("effect Logger { log: fn(string) -> Unit }\n\
            fn run() -> int = handle Logger\n\
              log msg => 0\n\
            in 42\n");
        // `resume(v + 1000)` feeds arithmetic — a `Result<int, MathError>` —
        // into the `int` operation-result slot, and the handled body ends in
        // `a + 1`, another `Result`, flowing into the `int` answer pinned by the
        // `false => 0` arm. Both are assignment sites that auto-unwrap; a plain
        // unify would wrongly reject them. Guards the [EFFECTS-RESUME] fix.
        ok("effect Guard { check: fn(int) -> int }\n\
            fn guarded() -> int = handle Guard\n\
              check v => match v < 100 {\n\
                true => resume(v + 1000)\n\
                false => 0\n\
              }\n\
            in {\n\
              let a = perform Guard.check(5)\n\
              a + 1\n\
            }\n");
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
            body: Expr::Binary {
                op: "+".into(),
                left: Box::new(Expr::Identifier("n".into())),
                right: Box::new(Expr::Integer(1)),
            },
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
                body: Expr::Binary {
                    op: "+".into(),
                    left: Box::new(Expr::Identifier("self".into())),
                    right: Box::new(Expr::Identifier("other".into())),
                },
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
        // `+` over lists and maps unifies operands; `-` over floats yields a
        // float Result.
        ok("let xs = [1, 2] + [3, 4]\n\
            fn fsub(a: float, b: float) -> Result<float, MathError> = a - b\n");
    }

    #[test]
    fn perform_with_named_arguments() {
        // A perform whose operation takes a named argument drives the named-arg
        // loop; `perform` named args are built directly (the parser emits only
        // positional perform args).
        use osprey_ast::{Expr, NamedArgument, Program, Stmt};
        ok("effect Logger { log: fn(string) -> Unit }\n\
            fn shout(msg: string) -> Unit !Logger = perform Logger.log(msg)\n");
        let perform = Stmt::Expr {
            value: Expr::Perform {
                effect: "Logger".into(),
                operation: "log".into(),
                arguments: Vec::new(),
                named_arguments: vec![NamedArgument {
                    name: "msg".into(),
                    value: Expr::Str("hi".into()),
                }],
                position: None,
            },
            position: None,
        };
        let errs = check_program(&Program {
            statements: vec![perform],
        });
        assert!(errs.is_empty(), "unexpected errors: {errs:?}");
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
            fn fadd(a: float, b: float) -> Result<float, MathError> = a + b\n\
            fn fmul(a: float, b: float) -> Result<float, MathError> = a * b\n");
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
    fn comparison_over_results_unwraps_both_sides() {
        // `a % b` and `c % d` are both `Result<int, MathError>`; comparing them
        // exercises the comparison arm's `unwrap_result` on both operands.
        ok("fn cmp(a: int, b: int) -> bool = (a % b) == (b % a)\n");
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
        ok("let f = fn(x: int) -> int => x + 1\n\
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
