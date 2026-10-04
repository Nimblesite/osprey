//! Effect row handlers.
use super::{
    merge_optional_value, walk_children, Analyzer, CallableEnv, Expr, HandlerArm, Position,
    Requirement, Value,
};

impl Analyzer<'_> {
    pub(super) fn handler_arm_env(
        &self,
        effect: &str,
        arm: &HandlerArm,
        body: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallableEnv {
        let mut arguments = vec![None; arm.params.len()];
        self.collect_operation_arguments(
            body,
            effect,
            &arm.operation,
            &arm.params,
            scope,
            env,
            &mut arguments,
        );
        let mut local = env.clone();
        for (index, parameter) in arm.params.iter().enumerate() {
            let _ = local.shadowed.insert(parameter.clone());
            if let Some(value) = arguments.get(index).cloned().flatten() {
                let _ = local.values.insert(parameter.clone(), value);
            } else {
                let _ = local.values.remove(parameter);
            }
        }
        local
    }

    pub(super) fn handler_body_env(
        &self,
        effect: &str,
        arms: &[HandlerArm],
        body: &Expr,
        position: Option<Position>,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallableEnv {
        let arguments =
            self.instance_arguments(effect, position, &self.instances.handlers, "handler");
        let mut local = env.clone();
        for arm in arms {
            // Arms execute outside this handler, so resolve their returned
            // values using only the outer handler environment.
            let mut arm_env = self.handler_arm_env(effect, arm, body, scope, env);
            let value = self
                .returned_value(&arm.body, scope, &mut arm_env)
                .unwrap_or_else(Value::unknown_callable);
            let _ = local.handler_returns.insert(
                Requirement::new(effect, &arm.operation, arguments.clone()),
                value,
            );
        }
        local
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation payload provenance needs the lexical effect, arm, and value environment"
    )]
    pub(super) fn collect_operation_arguments(
        &self,
        expression: &Expr,
        effect: &str,
        operation: &str,
        parameters: &[String],
        scope: &[String],
        env: &CallableEnv,
        out: &mut [Option<Value>],
    ) {
        if let Expr::Perform {
            effect: performed_effect,
            operation: performed_operation,
            arguments,
            named_arguments,
            ..
        } = expression
        {
            if performed_effect == effect && performed_operation == operation {
                for (index, argument) in arguments.iter().enumerate() {
                    if let (Some(slot), Some(value)) =
                        (out.get_mut(index), self.value(argument, scope, env))
                    {
                        merge_optional_value(slot, value);
                    }
                }
                for argument in named_arguments {
                    if let Some(index) = parameters.iter().position(|name| name == &argument.name) {
                        if let (Some(slot), Some(value)) =
                            (out.get_mut(index), self.value(&argument.value, scope, env))
                        {
                            merge_optional_value(slot, value);
                        }
                    }
                }
            }
        }
        walk_children(expression, |child| {
            self.collect_operation_arguments(child, effect, operation, parameters, scope, env, out);
        });
    }
}
