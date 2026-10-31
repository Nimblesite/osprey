//! Effect row values.
use super::{
    bind_pattern, builtin_callable_value, expression_name, expression_site, merge_optional_value,
    project_element, project_fiber_value, project_field, specialize_value, Analyzer, BTreeSet,
    CallArguments, Callable, CallableEnv, EmptyCase, Expr, ExprArgs, KnownCallable, Requirement,
    Value,
};

impl Analyzer<'_> {
    fn scalar_extern_value(&self, scope: &[String], name: &str) -> Option<Value> {
        let parameters = self.index.extern_parameters(scope, name)?;
        let (params, ret) = self.extern_signature(scope, name)?;
        (params.iter().all(scalar_type) && scalar_type(ret)).then(|| {
            Value::from_callable(Callable::Known(Box::new(KnownCallable {
                parameters: parameters.clone(),
                summary: super::Summary::default(),
                returned: super::value_shapes::value_shape(ret).map(Box::new),
            })))
        })
    }

    pub(super) fn extern_signature(
        &self,
        scope: &[String],
        name: &str,
    ) -> Option<&(Vec<crate::ty::Type>, crate::ty::Type)> {
        self.instances
            .signatures
            .get(self.index.extern_signature_name(scope, name)?)
    }

    pub(super) fn callable(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Callable> {
        self.value(expression, scope, env)
            .and_then(Self::callable_from_value)
    }

    pub(super) fn callable_from_value(value: Value) -> Option<Callable> {
        if value.performed.is_empty() {
            value.callable
        } else {
            Some(Callable::Unknown)
        }
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
        self.value_with_raw(expression, env, || self.raw_value(expression, scope, env))
    }

    pub(super) fn value_with_raw(
        &self,
        expression: &Expr,
        env: &CallableEnv,
        raw: impl FnOnce() -> Option<Value>,
    ) -> Option<Value> {
        #[cfg(test)]
        super::field_rows::tests::record_value_visit(expression);
        let empty_case = self.empty_case(expression, env);
        let value = self
            .scalar_lookup(expression, empty_case)
            .or_else(|| osprey_ast::with_stack(raw))
            .or_else(|| empty_case.map(|_| Value::default()))
            .or_else(|| self.scalar_shape(expression))
            .or_else(|| self.typed_unknown(expression))?;
        Some(self.typed_value(expression, value, empty_case))
    }

    fn scalar_lookup(&self, expression: &Expr, empty_case: Option<EmptyCase>) -> Option<Value> {
        if empty_case.is_some() {
            return None;
        }
        let target = match expression {
            Expr::Identifier(_) | Expr::Path(_) => expression,
            Expr::FieldAccess { target, .. } => target.as_ref(),
            _ => return None,
        };
        matches!(target, Expr::Identifier(_) | Expr::Path(_))
            .then(|| self.scalar_shape(expression))
            .flatten()
    }

    fn typed_unknown(&self, expression: &Expr) -> Option<Value> {
        (!matches!(expression, Expr::Identifier(_) | Expr::Path(_))
            && self
                .instances
                .expression_types
                .borrow()
                .contains_key(&expression_site(expression)))
        .then(Value::unknown_value)
    }

    pub(super) fn typed_value(
        &self,
        expression: &Expr,
        mut value: Value,
        empty_case: Option<EmptyCase>,
    ) -> Value {
        if empty_case.is_some() {
            value.empty_case = empty_case;
        } else if let Some(scalar) = self.scalar_shape(expression) {
            value = scalar;
        }
        match self.field_owner(expression) {
            super::field_rows::FieldOwner::Named(owner) => value.field_owner = Some(owner),
            super::field_rows::FieldOwner::Anonymous => value.field_owner = None,
            super::field_rows::FieldOwner::Unknown => {}
        }
        self.instantiated_value(expression, value)
    }

    fn instantiated_value(&self, expression: &Expr, mut value: Value) -> Value {
        if let Some(bindings) = self
            .instances
            .instantiations
            .get(&std::ptr::from_ref(expression).addr())
        {
            specialize_value(&mut value, bindings);
        }
        value
    }

    /// The whole provenance of `expression` when its type alone decides it: a
    /// scalar, or a value of a type that can never hold a function
    /// ([`super::data_shapes`]).
    pub(super) fn scalar_shape(&self, expression: &Expr) -> Option<Value> {
        let types = self.instances.expression_types.borrow();
        let ty = types.get(&expression_site(expression))?;
        if scalar_type(ty) {
            return super::value_shapes::value_shape(ty);
        }
        super::data_shapes::data_shape(ty, &self.instances.data_types)
    }

    fn empty_case(&self, expression: &Expr, env: &CallableEnv) -> Option<EmptyCase> {
        match expression {
            Expr::List(items, _) if items.is_empty() => Some(EmptyCase::Collection),
            Expr::Map(entries) if entries.is_empty() => Some(EmptyCase::Collection),
            Expr::Identifier(name)
                if !env.shadowed.contains(name) && !env.values.contains_key(name) =>
            {
                self.empty_constructor(name)
            }
            Expr::TypeConstructor { name, fields, .. } if !env.shadowed.contains(name) => {
                self.constructor_case(name, fields.is_empty())
            }
            _ => None,
        }
    }

    fn constructor_case(&self, name: &str, empty: bool) -> Option<EmptyCase> {
        match (name, empty) {
            ("Error", _) => Some(EmptyCase::Error),
            (_, true) => self.empty_constructor(name),
            _ => None,
        }
    }

    fn empty_constructor(&self, name: &str) -> Option<EmptyCase> {
        self.index
            .constructors
            .get(name)
            .filter(|fields| fields.is_empty())
            .map(|_| EmptyCase::Nullary)
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
                    .or_else(|| self.scalar_extern_value(scope, name))
                    .or_else(|| {
                        if self.index.extern_parameters(scope, name).is_some() {
                            return None;
                        }
                        let types = self.instances.expression_types.borrow();
                        builtin_callable_value(name, types.get(&expression_site(expression)))
                    })
            }
            Expr::Path(path) => self
                .index
                .resolve(scope, &path.to_string())
                .map(|id| self.function_value(id))
                .or_else(|| self.scalar_extern_value(scope, &path.to_string())),
            Expr::Lambda {
                parameters, body, ..
            } => {
                let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let mut local = env.enter_lambda(&names);
                let (summary, returned) = self.lambda_body_model(body, scope, &mut local);
                let returned = returned.map(Box::new);
                Some(Value::from_callable(Callable::Known(Box::new(
                    KnownCallable {
                        parameters: names,
                        summary,
                        returned,
                    },
                ))))
            }
            Expr::FieldAccess { target, field } => {
                let projected = self
                    .value(target, scope, env)
                    .and_then(|value| project_field(value, field));
                let owner = self.field_owner(target);
                self.owned_field_value(&owner, field, projected)
            }
            Expr::List(items, _) => {
                let mut element = None;
                for item in items {
                    merge_optional_value(
                        &mut element,
                        self.value(item, scope, env)
                            .unwrap_or_else(Value::unknown_value),
                    );
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
                    merge_optional_value(
                        &mut element,
                        self.value(&entry.value, scope, env)
                            .unwrap_or_else(Value::unknown_value),
                    );
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
                None => self.call_value(
                    function,
                    ExprArgs::new(arguments),
                    named_arguments,
                    scope,
                    env,
                ),
            },
            Expr::MethodCall { .. } => crate::methods::parts(expression)
                .and_then(|parts| self.method_value(expression, &parts, scope, env)),
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
                    bind_pattern(
                        &arm.pattern,
                        matched.as_ref(),
                        &self.field_owner(value),
                        self.index,
                        &mut local,
                    );
                    merge_optional_value(
                        &mut merged,
                        self.value(&arm.body, scope, &local)
                            .unwrap_or_else(Value::unknown_value),
                    );
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
                // Cleanup returns `Unit`; it never contributes the answer.
                finally: _,
                position,
                stage,
            } => {
                let site = super::handlers::HandlerSite {
                    effect,
                    arms,
                    body,
                    position: *position,
                    stage: *stage,
                };
                let local = self.handler_body_env(&site, scope, env);
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
                let _ = merged.get_or_insert_with(Value::unknown_value);
                for arm in arms {
                    if !self
                        .index
                        .operations
                        .mode_of(effect, &arm.operation)
                        .is_control()
                    {
                        continue;
                    }
                    let local = self.handler_arm_env(effect, arm, body, *stage, scope, env);
                    merge_optional_value(
                        &mut merged,
                        self.value(&arm.body, scope, &local)
                            .unwrap_or_else(Value::unknown_value),
                    );
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
            // A mask changes which activation answers, not the value.
            Expr::Mask { body, .. } => self.value(body, scope, env),
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

fn scalar_type(ty: &crate::ty::Type) -> bool {
    use crate::ty::{names, Type};
    matches!(ty, Type::Con { name, args } if args.is_empty() && matches!(name.as_str(),
        names::INT | names::FLOAT | names::BOOL | names::STRING | names::UNIT | names::PTR))
}
