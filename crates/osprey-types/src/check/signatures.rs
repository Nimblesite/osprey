//! Type checking: signatures.
use super::{
    type_expr_to_type, Checker, ExternParameter, HashMap, ParamName, Parameter, Scheme, Type,
    TypeEnv, TypeError, TypeExpr, TypeParam,
};

impl Checker {
    pub(super) fn collect_extern(
        &mut self,
        name: &str,
        parameters: &[ExternParameter],
        return_type: Option<&TypeExpr>,
        env: &mut TypeEnv,
    ) {
        let empty = HashMap::new();
        let params: Vec<Type> = parameters
            .iter()
            .map(|p| type_expr_to_type(&p.ty, &empty))
            .collect();
        let ret = return_type.map_or_else(Type::unit, |r| type_expr_to_type(r, &empty));
        // Publishing the resolved signature is what lets the backend type FFI
        // calls with the declared parameter/return types (a `Ptr` as `i8*`, not
        // the `i64` default) — same as `collect_function`.
        self.publish_signature(name, parameters, params, ret, env);
    }

    /// Record a declaration's parameter names, publish its resolved signature
    /// for the backend, and bind it monomorphically in `env`. Both collectors
    /// (extern and function) finish here, so a signature can never reach one
    /// table and miss another.
    pub(super) fn publish_signature<P: ParamName>(
        &mut self,
        name: &str,
        parameters: &[P],
        params: Vec<Type>,
        ret: Type,
        env: &mut TypeEnv,
    ) {
        self.record_fn_params(name, parameters);
        let _ = self
            .fn_sigs
            .insert(name.to_string(), (params.clone(), ret.clone()));
        env.insert(name, Scheme::mono(Type::fun(params, ret)));
    }

    /// Record a function/extern's positional parameter names (for named-argument
    /// reordering at call sites). Generic over the two parameter node types,
    /// which both carry a `name`.
    pub(super) fn record_fn_params<P: ParamName>(&mut self, name: &str, parameters: &[P]) {
        let _ = self.fn_params.insert(
            name.to_string(),
            parameters
                .iter()
                .map(|p| p.param_name().to_string())
                .collect(),
        );
    }

    pub(super) fn collect_function(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        parameters: &[Parameter],
        return_type: Option<&TypeExpr>,
        env: &mut TypeEnv,
    ) {
        // The testing built-ins are shadowable by design [TESTING-SHADOWING]:
        // a user `fn test/expect/check` replaces the built-in scheme below.
        if self.builtins.contains(name) && !crate::builtins::SHADOWABLE_BUILTINS.contains(&name) {
            self.errors.push(TypeError::new(format!(
                "cannot redefine built-in function `{name}`"
            )));
            return;
        }
        if self.fn_sigs.contains_key(name) {
            self.errors
                .push(TypeError::new(format!("duplicate definition `{name}`")));
            return;
        }
        // Declared type parameters (`fn map<T, U>`) bind to fresh inference
        // variables so every `T` in the signature is the SAME variable —
        // without a binder, `T` would be a nominal type named "T".
        // Implements [TYPE-GENERICS-FN].
        let mut typarams = HashMap::new();
        for tp in type_params {
            let v = self.ctx.fresh();
            if typarams.insert(tp.name.clone(), v).is_some() {
                self.errors.push(TypeError::new(format!(
                    "duplicate type parameter `{}`",
                    tp.name
                )));
            }
        }
        let params: Vec<Type> = parameters
            .iter()
            .map(|p| match &p.ty {
                Some(te) => self.annotation_type(te, &typarams, te.position),
                None => self.ctx.fresh(),
            })
            .collect();
        let ret = match return_type {
            Some(te) => self.annotation_type(te, &typarams, te.position),
            None => self.ctx.fresh(),
        };
        let ordered = type_params
            .iter()
            .filter_map(|p| typarams.get(&p.name).cloned())
            .collect();
        let _ = self.fn_typarams.insert(name.to_string(), typarams);
        self.publish_signature(name, parameters, params, ret, env);
        env.declare_type_params(name, ordered);
    }
}
