//! Type checking: collect.
use super::{returns_expansive_handler_value, Checker, Program, Stmt, TypeEnv, TypeError};

impl Checker {
    /// Pass one: fill the declaration tables and the base environment.
    pub(super) fn collect(&mut self, program: &Program, env: &mut TypeEnv) {
        self.collect_statements(program.statements.iter(), env);
    }

    pub(super) fn collect_statements<'a>(
        &mut self,
        statements: impl Iterator<Item = &'a Stmt> + Clone,
        env: &mut TypeEnv,
    ) {
        // `env` is exactly the builtin table on entry — snapshot it so
        // `collect_function` can reject redefinition of a built-in.
        if self.builtins.is_empty() {
            self.builtins = env.bound_names();
        }
        // Register every declared type's variance first, so position
        // validation sees nested constructors' variance regardless of
        // declaration order. Implements [TYPE-VARIANCE-DECL].
        for stmt in statements.clone() {
            if let Stmt::Type {
                name, type_params, ..
            } = stmt
            {
                self.ctx.set_variance(
                    name.clone(),
                    type_params.iter().map(|p| p.variance).collect(),
                );
            }
        }
        for stmt in statements {
            match stmt {
                Stmt::Type {
                    name,
                    type_params,
                    variants,
                    validation_func,
                    alias,
                    opaque,
                    position,
                    ..
                } => {
                    if *opaque {
                        let _ = self.opaque_types.insert(name.clone());
                    }
                    if let (true, Some(alias)) = (*opaque, alias) {
                        let params = type_params.iter().map(|p| p.name.clone()).collect();
                        self.ctx.set_equation(name.clone(), params, alias.clone());
                    }
                    if validation_func.is_some() {
                        self.record_err(
                            TypeError::new("validated record `where` is not supported".to_string()),
                            *position,
                        );
                    }
                    self.collect_type(name, type_params, variants, *position);
                }
                Stmt::Effect {
                    stage,
                    name,
                    type_params,
                    operations,
                    position,
                    ..
                } => self.collect_effect(*stage, name, type_params, operations, *position),
                Stmt::Extern {
                    name,
                    parameters,
                    return_type,
                    ..
                } => self.collect_extern(name, parameters, return_type.as_ref(), env),
                Stmt::Function {
                    name,
                    type_params,
                    parameters,
                    return_type,
                    position,
                    body,
                    ..
                } => {
                    self.collect_function(name, type_params, parameters, return_type.as_ref(), env);
                    if returns_expansive_handler_value(body) {
                        let _ = self.handler_factories.insert(name.clone());
                    }
                    for e in crate::variance::reject_fn_variance(name, type_params) {
                        self.record_err(e, *position);
                    }
                }
                _ => {}
            }
        }
    }
}
