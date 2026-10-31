//! Effect row handlers.
use super::{
    merge_optional_value, walk_children, Analyzer, CallableEnv, Expr, HandlerArm, Position,
    Requirement, Value,
};

/// One handler as the row analysis sees it: the effect it answers, its arms,
/// the handled body, where it was written and when its arms run.
pub(super) struct HandlerSite<'a> {
    pub(super) effect: &'a str,
    pub(super) arms: &'a [HandlerArm],
    pub(super) body: &'a Expr,
    pub(super) position: Option<Position>,
    pub(super) stage: osprey_ast::Stage,
}

impl Analyzer<'_> {
    pub(super) fn handler_arm_env(
        &self,
        effect: &str,
        arm: &HandlerArm,
        body: &Expr,
        stage: osprey_ast::Stage,
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
                if stage.is_compile_time() {
                    let _ = local.deferred_arm_parameters.insert(parameter.clone());
                }
            }
        }
        local
    }

    pub(super) fn handler_body_env(
        &self,
        site: &HandlerSite<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallableEnv {
        let arguments = self.instance_arguments(
            site.effect,
            site.position,
            &self.instances.handlers,
            "handler",
        );
        let mut local = env.clone();
        for arm in site.arms {
            // Arms execute outside this handler, so resolve their returned
            // values using only the outer handler environment.
            let mut arm_env =
                self.handler_arm_env(site.effect, arm, site.body, site.stage, scope, env);
            let changes = self.flow_callable_assignments(&arm.body, scope, &mut arm_env);
            super::assignment_flow::merge_changes(&mut local.values, changes);
            let value = self
                .returned_value(&arm.body, scope, &mut arm_env)
                .unwrap_or_else(Value::unknown_value);
            let _ = local.handler_returns.insert(
                Requirement::new(site.effect, &arm.operation, arguments.clone()),
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
                    if let Some(slot) = out.get_mut(index) {
                        merge_optional_value(
                            slot,
                            self.value(argument, scope, env)
                                .unwrap_or_else(Value::unknown_value),
                        );
                    }
                }
                for argument in named_arguments {
                    if let Some(index) = parameters.iter().position(|name| name == &argument.name) {
                        if let Some(slot) = out.get_mut(index) {
                            merge_optional_value(
                                slot,
                                self.value(&argument.value, scope, env)
                                    .unwrap_or_else(Value::unknown_value),
                            );
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
