//! Effect row values.
use super::{
    bind_pattern, builtin_callable_value, expression_name, expression_site, merge_optional_value,
    project_element, project_fiber_value, project_field, specialize_value, Analyzer, BTreeSet,
    CallArguments, Callable, CallableEnv, Expr, KnownCallable, Requirement, Value,
};

impl Analyzer<'_> {
    pub(super) fn callable(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Callable> {
        self.value(expression, scope, env).and_then(|value| {
            if value.performed.is_empty() {
                value.callable
            } else {
                Some(Callable::Unknown)
            }
        })
    }

    pub(super) fn channel_payload(channel: Value, env: &CallableEnv) -> Option<Value> {
        let mut payload = None;
        for site in channel.channel_sites {
            if let Some(value) = env.channel_payloads.get(&site) {
                merge_optional_value(&mut payload, value.clone());
            }
        }
        payload
    }

    pub(super) fn value(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        let mut value = osprey_ast::with_stack(|| self.raw_value(expression, scope, env))?;
        if let Some(bindings) = self
            .instances
            .instantiations
            .get(&std::ptr::from_ref(expression).addr())
        {
            specialize_value(&mut value, bindings);
        }
        Some(value)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "value provenance mirrors the exhaustive expression fold"
    )]
    pub(super) fn raw_value(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        match expression {
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::InterpolatedStr(_) => Some(Value {
                field_names: Some(BTreeSet::new()),
                ..Value::default()
            }),
            Expr::TypeApply { function, .. } => self.value(function, scope, env),
            Expr::Identifier(name) => {
                if let Some(value) = env.values.get(name) {
                    return Some(value.clone());
                }
                if env.shadowed.contains(name) {
                    return None;
                }
                self.index
                    .resolve(scope, name)
                    .map(|id| self.function_value(id))
                    .or_else(|| {
                        let types = self.instances.expression_types.borrow();
                        builtin_callable_value(name, types.get(&expression_site(expression)))
                    })
            }
            Expr::Path(path) => self
                .index
                .resolve(scope, &path.to_string())
                .map(|id| self.function_value(id)),
            Expr::Lambda {
                parameters, body, ..
            } => {
                let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let mut local = env.enter_lambda(&names);
                let summary = self.expression(body, scope, &local);
                let returned = self.returned_value(body, scope, &mut local).map(Box::new);
                Some(Value::from_callable(Callable::Known(Box::new(
                    KnownCallable {
                        parameters: names,
                        summary,
                        returned,
                    },
                ))))
            }
            Expr::FieldAccess { target, field } => self
                .value(target, scope, env)
                .and_then(|value| project_field(value, field)),
            Expr::List(items, _) => {
                let mut element = None;
                for item in items {
                    if let Some(value) = self.value(item, scope, env) {
                        merge_optional_value(&mut element, value);
                    }
                }
                Some(Value {
                    element: element.map(Box::new),
                    field_names: Some(BTreeSet::new()),
                    ..Value::default()
                })
            }
            Expr::Map(entries) => {
                let mut element = None;
                for entry in entries {
                    if let Some(value) = self.value(&entry.value, scope, env) {
                        merge_optional_value(&mut element, value);
                    }
                }
                Some(Value {
                    element: element.map(Box::new),
                    field_names: Some(BTreeSet::new()),
                    ..Value::default()
                })
            }
            Expr::TypeConstructor { name, fields, .. }
                if !self.index.constructors.contains_key(name) && env.shadowed.contains(name) =>
            {
                let mut updated = self
                    .value(&Expr::Identifier(name.clone()), scope, env)
                    .unwrap_or_default();
                for field in fields {
                    if let Some(value) = self.value(&field.value, scope, env) {
                        let _ = updated.fields.insert(field.name.clone(), value);
                    } else {
                        let _ = updated.fields.remove(&field.name);
                    }
                }
                Some(updated)
            }
            // Ordered after the shadowed-constructor guard above: a shadowed
            // name is a record update, not a fresh record.
            Expr::Object(fields) | Expr::TypeConstructor { fields, .. } => {
                Some(self.record_value(fields, scope, env))
            }
            Expr::Update { record, fields } => {
                let mut updated = self
                    .value(&Expr::Identifier(record.clone()), scope, env)
                    .unwrap_or_default();
                for field in fields {
                    if let Some(value) = self.value(&field.value, scope, env) {
                        let _ = updated.fields.insert(field.name.clone(), value);
                    } else {
                        let _ = updated.fields.remove(&field.name);
                    }
                }
                Some(updated)
            }
            Expr::Index { target, .. } => Some(Value {
                result_payload: Some(Box::new(
                    self.value(target, scope, env)
                        .and_then(project_element)
                        .unwrap_or_else(Value::unknown_callable),
                )),
                ..Value::default()
            }),
            Expr::Spawn(value) => Some(Value {
                field_names: Some(BTreeSet::new()),
                fiber_payload: Some(Box::new(
                    self.value(value, scope, env)
                        .unwrap_or_else(Value::unknown_callable),
                )),
                ..Value::default()
            }),
            Expr::Await(fiber) => Some(
                self.value(fiber, scope, env)
                    .and_then(project_fiber_value)
                    .unwrap_or_else(Value::unknown_callable),
            ),
            Expr::Recv(channel) => Some(
                self.value(channel, scope, env)
                    .and_then(|channel| Self::channel_payload(channel, env))
                    .unwrap_or_else(Value::unknown_callable),
            ),
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } if expression_name(function) == Some("Channel")
                && !env.shadowed.contains("Channel") =>
            {
                Some(Value::channel(expression_site(expression)))
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => match crate::methods::parts(expression) {
                Some(parts) => self.method_value(expression, &parts, scope, env),
                None => self.call_value(function, arguments, named_arguments, scope, env),
            },
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => self.method_value(
                expression,
                &crate::methods::Parts {
                    target,
                    method,
                    arguments,
                    named: named_arguments,
                    application: None,
                },
                scope,
                env,
            ),
            Expr::Pipe { left, right } => self.pipe_value(left, right, scope, env),
            Expr::Perform {
                effect,
                operation,
                position,
                ..
            } => {
                let mut merged = None;
                for arguments in self.perform_instance_arguments(effect, *position) {
                    let requirement = Requirement::new(effect, operation, arguments);
                    if let Some(value) = env.handler_returns.get(&requirement) {
                        merge_optional_value(&mut merged, value.clone());
                    } else {
                        // A callback created outside a handler may run inside
                        // one later. Keep the exact requested operation until
                        // that invocation is projected; an unrelated generic
                        // instance cannot supply its result.
                        merge_optional_value(
                            &mut merged,
                            Value {
                                performed: [requirement].into_iter().collect(),
                                ..Value::default()
                            },
                        );
                    }
                }
                merged
            }
            Expr::Match { value, arms } => {
                let matched = self.value(value, scope, env);
                let mut merged = None;
                for arm in arms {
                    let mut local = env.clone();
                    bind_pattern(&arm.pattern, matched.as_ref(), self.index, &mut local);
                    if let Some(value) = self.value(&arm.body, scope, &local) {
                        merge_optional_value(&mut merged, value);
                    }
                }
                merged
            }
            // A handler governs evaluation of its body, not later use of a
            // value produced by that body or one of its operation arms.
            Expr::Handler {
                effect,
                arms,
                body,
                return_clause,
                position,
                ..
            } => {
                let local = self.handler_body_env(effect, arms, body, *position, scope, env);
                let normal = self
                    .value(body, scope, &local)
                    .map(|value| Self::resolve_handled_value(value, &local.handler_returns));
                let mut merged = if let Some(clause) = return_clause {
                    self.called_value(
                        self.callable(clause, scope, env),
                        CallArguments {
                            positional: vec![normal],
                            named: vec![],
                        },
                    )
                } else {
                    normal
                };
                for arm in arms {
                    if !self
                        .index
                        .operations
                        .mode_of(effect, &arm.operation)
                        .is_control()
                    {
                        continue;
                    }
                    let local = self.handler_arm_env(effect, arm, body, scope, env);
                    if let Some(value) = self.value(&arm.body, scope, &local) {
                        merge_optional_value(&mut merged, value);
                    }
                }
                merged
            }
            Expr::Block {
                statements, value, ..
            } => {
                let mut local = env.clone();
                let _ = self.statements(statements, scope, &mut local);
                value
                    .as_deref()
                    .and_then(|value| self.value(value, scope, &local))
            }
            Expr::Resume(value) | Expr::Yield(value) => value
                .as_deref()
                .and_then(|value| self.value(value, scope, env)),
            Expr::Binary {
                op, left, right, ..
            } => Some(self.binary_value(expression, op, left, right, scope, env)),
            _ => None,
        }
    }
}
