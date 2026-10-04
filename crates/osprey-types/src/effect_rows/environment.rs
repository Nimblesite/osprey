//! Effect row environment.
use super::{shift_value_levels, CallableEnv, HandlerArm, Value};

impl CallableEnv {
    /// The environment of a handler's body: under a compile-time handler the
    /// operations its arms answer are statically discharged there.
    pub(super) fn under_handler(
        &self,
        effect: &str,
        arms: &[HandlerArm],
        stage: osprey_ast::Stage,
    ) -> Self {
        let mut body = self.clone();
        if stage.is_compile_time() {
            body.static_answers.extend(arms.iter().map(|arm| {
                (
                    osprey_ast::effect_name::base(effect).to_owned(),
                    arm.operation.clone(),
                )
            }));
        }
        body
    }
}

impl CallableEnv {
    /// Bind each parameter at the current level, shadowing any outer binding of
    /// the same name.
    pub(super) fn bind_parameters(&mut self, parameters: &[String]) {
        for (index, parameter) in parameters.iter().enumerate() {
            let _ = self.shadowed.insert(parameter.clone());
            let _ = self
                .values
                .insert(parameter.clone(), Value::parameter(0, index));
        }
    }

    pub(super) fn enter_lambda(&self, parameters: &[String]) -> Self {
        let mut env = self.clone();
        env.handler_returns.clear();
        for value in env.values.values_mut() {
            shift_value_levels(value, 1, 0);
        }
        for value in env.channel_payloads.values_mut() {
            shift_value_levels(value, 1, 0);
        }
        env.bind_parameters(parameters);
        env
    }
}
