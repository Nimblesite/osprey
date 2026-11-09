//! Applying a function type to checked arguments, GPU fusion, pipes and
//! indexing.

use super::*;

impl Checker {
    pub(super) fn infer_checked(&mut self, argument: &Expr, param: &Type, env: &TypeEnv) -> Type {
        match argument {
            Expr::Lambda {
                parameters,
                return_type,
                body,
                position,
            } => {
                let expected = self.ctx.expose(param);
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
        match self.ctx.expose(ft) {
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
    /// settling it later could no longer select the integer overload and its
    /// Arith obligation. A constrained built-in defers the
    /// same way for the same reason, by name.
    fn absorbed_by_any(&mut self, param: &Type, argument: &Type, deferred: bool) -> bool {
        param.is_named(names::ANY)
            && (deferred || matches!(self.ctx.expose(argument), Type::Var(_)))
    }

    pub(crate) fn apply_named_fn(
        &mut self,
        name: Option<&str>,
        ft: &Type,
        args: Vec<Type>,
    ) -> Type {
        let constrained_builtin = name.is_some_and(constrained_builtin);
        if let (Some(name), Some(receiver)) = (name, args.first()) {
            if constrained_builtin {
                let name = self.sited_use(name);
                self.builtin_uses.push((name, receiver.clone()));
            }
        }
        self.constrain_named_comparison(name, &args);
        self.constrain_structural_equals(name, &args);
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
        let Type::Fun { params, ret } = self.ctx.expose(ft) else {
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

    pub(super) fn infer_pipe(&mut self, left: &Expr, right: &Expr, env: &TypeEnv) -> Type {
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
            let ty = self.infer_expr(&call, env);
            if let (
                Some(types),
                Expr::Call {
                    arguments: cloned, ..
                },
            ) = (&mut self.expression_types, &call)
            {
                for (source, original) in cloned.iter().zip(std::iter::once(left).chain(arguments))
                {
                    crate::arithmetic::transfer(source, original, types);
                }
            }
            ty
        } else {
            let (name, ft) = self.infer_callee(right, env);
            let lt = self.infer_expr(left, env);
            self.apply_bound_fn(name.as_deref(), &ft, vec![lt], env)
        }
    }

    pub(super) fn infer_index(&mut self, target: &Expr, index: &Expr, env: &TypeEnv) -> Type {
        let tt = self.infer_expr(target, env);
        let it = self.infer_expr(index, env);
        if self.reject_erased_operand("index", &tt) {
            return self.ctx.fresh();
        }
        match self.ctx.expose(&tt) {
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
}
