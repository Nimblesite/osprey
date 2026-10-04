//! Effect row methods.
use super::{
    method_thunk, project_callable, project_field, specialize_summary, Analyzer, CallArguments,
    CallableEnv, Expr, MethodProjection, Projection, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn method_call(
        &self,
        expression: &Expr,
        parts: &crate::methods::Parts<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let site = std::ptr::from_ref(expression).addr();
        let mut out = match self.instances.methods.get(&site) {
            Some(crate::methods::Target::Field(field)) => {
                let mut out = self.expression(parts.target, scope, env);
                out.union(self.expressions(parts.arguments, scope, env));
                out.union(self.named_expressions(parts.named, scope, env));
                let projected = self
                    .value(parts.target, scope, env)
                    .and_then(|value| project_field(value, field));
                let callee = projected.as_ref().and_then(|value| value.callable.clone());
                if let Some(callee) = callee {
                    let arguments = self
                        .call_arguments(parts.arguments, parts.named, scope, env)
                        .in_written_order();
                    out.union(self.invoke_with_arguments(callee, arguments));
                } else if projected.is_none() || self.may_be_callable_field(parts.target, field) {
                    out.unresolved_dynamic_call = true;
                }
                out
            }
            Some(crate::methods::Target::Deferred(field)) => {
                let mut out = self.expression(parts.target, scope, env);
                out.union(self.expressions(parts.arguments, scope, env));
                out.union(self.named_expressions(parts.named, scope, env));
                let callee = self
                    .deferred_method(parts, field, scope, env)
                    .and_then(|value| value.callable);
                if let Some(callee) = callee {
                    out.union(self.invoke_with_values(callee, &[]));
                } else {
                    out.unresolved_dynamic_call = true;
                }
                out
            }
            Some(crate::methods::Target::Function) | None => self.call(
                &Expr::Identifier(parts.method.to_owned()),
                &self.receiver_first(parts.target, parts.arguments),
                parts.named,
                scope,
                env,
            ),
        };
        if let Some(bindings) = self.instances.instantiations.get(&site) {
            specialize_summary(&mut out, bindings);
        }
        out
    }

    pub(super) fn deferred_method(
        &self,
        parts: &crate::methods::Parts<'_>,
        field: &str,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        let function = Expr::Identifier(parts.method.to_owned());
        let arguments = self.receiver_first(parts.target, parts.arguments);
        let fallback = method_thunk(
            self.call_effects(&function, &arguments, parts.named, scope, env),
            self.call_value(&function, &arguments, parts.named, scope, env),
        );
        let method = MethodProjection {
            field: field.to_owned(),
            arguments: parts
                .arguments
                .iter()
                .map(|argument| self.value(argument, scope, env))
                .collect(),
            named: parts
                .named
                .iter()
                .map(|argument| {
                    (
                        argument.name.clone(),
                        self.value(&argument.value, scope, env),
                    )
                })
                .collect(),
            fallback,
        };
        self.value(parts.target, scope, env)
            .and_then(|value| self.project_method(value, method))
    }

    pub(super) fn project_method(
        &self,
        mut receiver: Value,
        method: MethodProjection,
    ) -> Option<Value> {
        let mut projected = if let Some(value) = receiver.fields.remove(&method.field) {
            value.callable.map(|callee| {
                let arguments = CallArguments {
                    positional: method.arguments.clone(),
                    named: method.named.clone(),
                }
                .in_written_order();
                // A field can hold a caller's symbolic callback. Retain its
                // returned-value projection until that callback is supplied.
                method_thunk(
                    self.invoke_with_arguments(callee.clone(), arguments.clone()),
                    self.project_call_return(Value::from_callable(callee), arguments),
                )
            })
        } else if receiver
            .field_names
            .as_ref()
            .is_some_and(|fields| !fields.contains(&method.field))
        {
            Some(method.fallback.clone())
        } else {
            None
        };
        if receiver.field_names.is_none() {
            project_callable(
                receiver.callable,
                Projection::Method(Box::new(method)),
                &mut projected,
            );
        }
        projected
    }

    pub(super) fn method_value(
        &self,
        expression: &Expr,
        parts: &crate::methods::Parts<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        match self
            .instances
            .methods
            .get(&std::ptr::from_ref(expression).addr())
        {
            Some(crate::methods::Target::Field(field)) => {
                let callee = self
                    .value(parts.target, scope, env)
                    .and_then(|value| project_field(value, field))
                    .and_then(|value| value.callable);
                let arguments = self
                    .call_arguments(parts.arguments, parts.named, scope, env)
                    .in_written_order();
                self.called_value(callee, arguments)
            }
            Some(crate::methods::Target::Deferred(field)) => self
                .deferred_method(parts, field, scope, env)
                .and_then(|value| self.project_returned(value)),
            Some(crate::methods::Target::Function) | None => self.call_value(
                &Expr::Identifier(parts.method.to_owned()),
                &self.receiver_first(parts.target, parts.arguments),
                parts.named,
                scope,
                env,
            ),
        }
    }
}
