//! Call inference: callees, type applications, methods, UFCS and argument
//! ordering.

use super::*;

impl Checker {
    pub(super) fn infer_call(
        &mut self,
        site: &Expr,
        function: &Expr,
        arguments: &[Expr],
        named: &[NamedArgument],
        env: &TypeEnv,
    ) -> Type {
        if let Some(parts) = crate::methods::parts(site) {
            return self.infer_method_call(site, parts, env);
        }
        let (fname, ft) = self.infer_callee(function, env);
        let args = self.ordered_arg_types(fname.as_deref(), &ft, arguments, named, env);
        self.apply_bound_fn(fname.as_deref(), &ft, args, env)
    }

    pub(super) fn infer_callee(
        &mut self,
        function: &Expr,
        env: &TypeEnv,
    ) -> (Option<String>, Type) {
        match function {
            Expr::TypeApply {
                function,
                type_args,
                position,
            } => self.infer_type_application(function, type_args, *position, env),
            Expr::Identifier(name) => (Some(name.clone()), self.infer_expr(function, env)),
            Expr::Path(path) => {
                let name = path.to_string();
                let ty = self.infer_expr(function, env);
                (Some(name), ty)
            }
            other => (None, self.infer_expr(other, env)),
        }
    }

    /// Pin declared binders in this call's fresh instantiation [TYPE-GENERICS-APPLY].
    pub(super) fn infer_type_application(
        &mut self,
        function: &Expr,
        type_args: &[TypeExpr],
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> (Option<String>, Type) {
        let name = match function {
            Expr::Identifier(name) => name.clone(),
            Expr::Path(path) => path.to_string(),
            _ => {
                self.errors
                    .push(TypeError::new("type arguments require a named function"));
                return (None, self.infer_expr(function, env));
            }
        };
        let Some(applied) = env.applied(&mut self.ctx, &name) else {
            return (Some(name.clone()), self.lookup_ident(&name, env));
        };
        let crate::env::AppliedSignature {
            ty,
            obligations,
            params,
            bindings,
        } = applied;
        let obligations = self.located_obligations(obligations, position.or(self.expr_position));
        self.builtin_uses.extend(obligations);
        if let Some(position) = position {
            self.application_tys.push((position, bindings.clone()));
        }
        let _ = self
            .instantiations
            .insert(std::ptr::from_ref(function).addr(), bindings);
        if params.len() == type_args.len() {
            let binder = self.current_fn_typarams.clone();
            for (param, arg) in params.iter().zip(type_args) {
                let written = self.written_type_argument(arg, &binder, position);
                self.push_unify(param, &written);
            }
        } else {
            self.errors.push(
                TypeError::new(format!(
                    "function `{name}` takes {} type argument(s), got {}",
                    params.len(),
                    type_args.len()
                ))
                .with_pos(position),
            );
        }
        (Some(name), ty)
    }

    /// Validate written arguments before conversion can discard an invalid
    /// application of an enclosing binder [TYPE-GENERICS-APPLY].
    pub(crate) fn written_type_argument(
        &mut self,
        argument: &TypeExpr,
        binder: &HashMap<String, Type>,
        position: Option<osprey_ast::Position>,
    ) -> Type {
        let written = self.annotation_type(argument, binder, position);
        self.validate_type_argument(&written, position);
        written
    }

    /// Preserve lexical binders in annotations without accepting applications
    /// that conversion would otherwise silently drop [TYPE-GENERICS-FN].
    pub(crate) fn annotation_type(
        &mut self,
        argument: &TypeExpr,
        binder: &HashMap<String, Type>,
        position: Option<osprey_ast::Position>,
    ) -> Type {
        if let Some(name) = applied_type_parameter(argument, binder) {
            self.errors.push(
                TypeError::new(format!(
                    "type parameter `{name}` cannot take type arguments"
                ))
                .with_pos(position),
            );
        }
        crate::convert::type_expr_to_type(argument, binder)
    }

    /// Written arguments name declared types or an enclosing binder, never a
    /// new nominal type inferred from a misspelled name [TYPE-GENERICS-APPLY].
    fn validate_type_argument(&mut self, ty: &Type, position: Option<osprey_ast::Position>) {
        match ty {
            Type::Con { name, args } => {
                let primitive = matches!(
                    name.as_str(),
                    names::BOOL
                        | names::STRING
                        | names::UNIT
                        | names::ANY
                        | names::PTR
                        | names::CHANNEL
                        | names::ITERATOR
                        | names::GPU_BUFFER
                ) || Numeric::of_name(name).is_some();
                if !primitive
                    && self.ctx.variance_of(name).is_none()
                    && !self.ctors.values().any(|ctor| ctor.owner == *name)
                {
                    self.errors
                        .push(TypeError::new(format!("unknown type `{name}`")).with_pos(position));
                }
                let arity = self
                    .ctx
                    .variance_of(name)
                    .map(<[_]>::len)
                    .or_else(|| {
                        self.ctors
                            .values()
                            .find(|ctor| ctor.owner == *name)
                            .map(|ctor| ctor.type_params.len())
                    })
                    .or_else(|| {
                        primitive.then_some(usize::from(matches!(
                            name.as_str(),
                            names::CHANNEL | names::ITERATOR | names::GPU_BUFFER
                        )))
                    });
                if let Some(expected) = arity.filter(|expected| *expected != args.len()) {
                    self.errors.push(
                        TypeError::new(format!(
                            "type `{name}` takes {expected} type argument(s), got {}",
                            args.len()
                        ))
                        .with_pos(position),
                    );
                }
                for arg in args {
                    self.validate_type_argument(arg, position);
                }
            }
            Type::Fun { params, ret } => {
                for param in params {
                    self.validate_type_argument(param, position);
                }
                self.validate_type_argument(ret, position);
            }
            _ => {}
        }
    }

    pub(super) fn infer_method_call(
        &mut self,
        site: &Expr,
        parts: crate::methods::Parts<'_>,
        env: &TypeEnv,
    ) -> Type {
        let receiver = self.infer_expr(parts.target, env);
        let field = crate::methods::field_name(parts.method);
        if matches!(self.ctx.expose_settled(&receiver), Type::Var(_))
            && env.get(parts.method).is_some()
        {
            self.record_method_target(site, crate::methods::Target::Deferred(field.clone()));
            return self.infer_deferred_method(site, receiver, &field, &parts, env);
        }
        let known_field = match self.ctx.expose(&receiver) {
            Type::Record { fields, .. } => fields.contains_key(&field),
            Type::Con { name, args } => self
                .ctx
                .record_field(&name, &args, &field)
                .is_ok_and(|ty| ty.is_some()),
            Type::Var(_) => env.get(parts.method).is_none(),
            _ => false,
        };
        if known_field {
            self.record_method_target(site, crate::methods::Target::Field(field.clone()));
            return self.infer_field_call(&receiver, &field, &parts, env);
        }
        if env.get(parts.method).is_some() {
            self.record_method_target(site, crate::methods::Target::Function);
        }
        self.infer_ufcs_call(site, &receiver, &parts, env)
    }

    fn infer_deferred_method(
        &mut self,
        site: &Expr,
        receiver: Type,
        field: &str,
        parts: &crate::methods::Parts<'_>,
        env: &TypeEnv,
    ) -> Type {
        let Some(fallback) = env.applied(&mut self.ctx, parts.method) else {
            return self.ctx.fresh();
        };
        let _ = self
            .instantiations
            .insert(std::ptr::from_ref(site).addr(), fallback.bindings.clone());
        let (count, position, written) = match parts.application {
            Some((args, position)) => {
                if let Some(position) = position {
                    self.application_tys.push((position, fallback.bindings));
                }
                let binder = self.current_fn_typarams.clone();
                let written = args
                    .iter()
                    .map(|arg| self.written_type_argument(arg, &binder, position))
                    .collect();
                (args.len().to_string(), position, written)
            }
            None => ("-".to_owned(), None, Vec::new()),
        };
        let arguments = parts
            .arguments
            .iter()
            .map(|arg| self.infer_expr(arg, env))
            .collect();
        let named = parts
            .named
            .iter()
            .map(|arg| Type::con(arg.name.clone(), vec![self.infer_expr(&arg.value, env)]))
            .collect();
        let result = self.ctx.fresh();
        let relation = Type::con(
            "$method-relation",
            vec![
                receiver,
                fallback.ty,
                Type::fun(arguments, result.clone()),
                Type::con(
                    "$obligations",
                    fallback
                        .obligations
                        .into_iter()
                        .map(|(name, ty)| Type::con(name, vec![ty]))
                        .collect(),
                ),
                Type::con("$binders", fallback.params),
                Type::con("$written", written),
                Type::con("$named", named),
                Type::con(
                    if env.is_builtin(parts.method) {
                        "$builtin"
                    } else {
                        "$source"
                    },
                    vec![],
                ),
            ],
        );
        let line = position.map_or(0, |position| position.line);
        let column = position.map_or(0, |position| position.column);
        self.builtin_uses.push((
            format!(
                "{}{count}:{line}:{column}:{}:{field}:{}",
                crate::methods::OBLIGATION,
                self.site,
                parts.method
            ),
            relation,
        ));
        self.resolve_field_uses();
        result
    }

    fn infer_field_call(
        &mut self,
        receiver: &Type,
        field: &str,
        parts: &crate::methods::Parts<'_>,
        env: &TypeEnv,
    ) -> Type {
        if let Some((args, position)) = parts.application {
            self.errors.push(
                TypeError::new(format!(
                    "function `{field}` takes 0 type argument(s), got {}",
                    args.len()
                ))
                .with_pos(position),
            );
        }
        let ft = self.infer_record_field_type(receiver, field);
        let args = self.ordered_arg_types(None, &ft, parts.arguments, parts.named, env);
        self.apply_named_fn(None, &ft, args)
    }

    fn infer_ufcs_call(
        &mut self,
        site: &Expr,
        receiver: &Type,
        parts: &crate::methods::Parts<'_>,
        env: &TypeEnv,
    ) -> Type {
        let ft = match parts.application {
            Some((arguments, position)) => {
                let callee = Expr::Identifier(parts.method.to_owned());
                let (_, ty) = self.infer_type_application(&callee, arguments, position, env);
                if let Some(bindings) = self
                    .instantiations
                    .remove(&std::ptr::from_ref(&callee).addr())
                {
                    let _ = self
                        .instantiations
                        .insert(std::ptr::from_ref(site).addr(), bindings);
                }
                ty
            }
            None => self.lookup_ident_at(parts.method, env, Some(site)),
        };
        let tail = match self.ctx.expose(&ft) {
            Type::Fun { params, ret } => Type::fun(params.into_iter().skip(1).collect(), *ret),
            other => other,
        };
        let mut args = vec![receiver.clone()];
        args.extend(self.positional_arg_types(&tail, parts.arguments, env));
        let mut named: Vec<_> = parts.named.iter().collect();
        if let Some(parameters) = self.fn_params.get(parts.method) {
            named.sort_by_key(|arg| parameters.iter().position(|name| name == &arg.name));
        }
        for na in named {
            args.push(self.infer_expr(&na.value, env));
        }
        self.apply_bound_fn(Some(parts.method), &ft, args, env)
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
    /// [GPU-KERNEL-ELEM-TYPING] (docs/specs/runtime/0034-GPUComputation.md) because of
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
        match self.ctx.expose(ft) {
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
}
