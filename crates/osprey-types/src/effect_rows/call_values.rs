//! Effect row call values.
use super::{
    builtin_return_shape, expression_name, merge_optional_value, project_element,
    source_named_callee, Analyzer, BTreeSet, CallArguments, Callable, CallableEnv, Expr,
    NamedArgument, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn call_value(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        if let Some(name) = self.builtin_callee(function, scope, env) {
            if let Some(value) = self.builtin_call_value(name, arguments, scope, env) {
                return Some(value);
            }
        }
        // Preserve each application in a curried spine: its result can be a
        // returned closure or aggregate with different effects from its maker.
        let resolved = self.callable(function, scope, env);
        let arguments = self.callsite_arguments(function, arguments, named_arguments, scope, env);
        self.called_value(resolved, arguments)
    }

    /// Builtin behavior belongs to the resolved binding, not its spelling.
    /// Local values and user functions may shadow callable builtin names.
    pub(super) fn builtin_callee<'b>(
        &self,
        function: &'b Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<&'b str> {
        let name = expression_name(function)?;
        (!env.shadowed.contains(name)
            && !env.values.contains_key(name)
            && self.index.resolve(scope, name).is_none())
        .then_some(name)
    }

    pub(super) fn called_value(
        &self,
        resolved: Option<Callable>,
        arguments: CallArguments,
    ) -> Option<Value> {
        let Some(callee @ (Callable::Known(_) | Callable::Parameter { .. })) = resolved else {
            // The RESULT of a call we cannot resolve is not thereby a callable.
            // This branch catches every builtin without a modelled value
            // (`print`, `listAppend`, …), whose result is plain data; calling
            // it an unknown callable makes each one poison the merges it flows
            // into. Staying fail-closed does not depend on this marker: a value
            // with no callable that is later invoked fails
            // `statically_named_callee` and is reported at that call.
            return Some(Value::default());
        };
        self.project_call_return(Value::from_callable(callee), arguments)
    }

    pub(super) fn callsite_arguments(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> CallArguments {
        let arguments = self.call_arguments(arguments, named, scope, env);
        if source_named_callee(function, scope, env, self.index) {
            arguments
        } else {
            arguments.in_written_order()
        }
    }

    pub(super) fn call_arguments(
        &self,
        arguments: &[Expr],
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> CallArguments {
        CallArguments {
            positional: arguments
                .iter()
                .map(|argument| self.value(argument, scope, env))
                .collect(),
            named: named
                .iter()
                .map(|argument| {
                    (
                        argument.name.clone(),
                        self.value(&argument.value, scope, env),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn builtin_call_value(
        &self,
        name: &str,
        arguments: &[Expr],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        let element = |index| {
            arguments
                .get(index)
                .and_then(|argument| self.value(argument, scope, env))
                .and_then(project_element)
        };
        let aggregate = |element: Option<Value>| Value {
            element: element.map(Box::new),
            field_names: Some(BTreeSet::new()),
            ..Value::default()
        };
        Some(match name {
            "range" | "List" | "Map" => Value {
                field_names: Some(BTreeSet::new()),
                ..Value::default()
            },
            "map" | "filter" => self.lazy_iterator_value(name, arguments, scope, env),
            "mapList" | "filterList" => {
                let mut value = self.lazy_iterator_value(name, arguments, scope, env);
                value.deferred = Summary::default();
                value
            }
            "listAppend" | "listPrepend" => {
                let mut merged = element(0);
                if let Some(value) = arguments
                    .get(1)
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "listConcat" | "mapMerge" => {
                let mut merged = element(0);
                if let Some(value) = element(1) {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "listReverse" | "mapRemove" => arguments
                .first()
                .and_then(|argument| self.value(argument, scope, env))
                .unwrap_or_default(),
            "listGet" | "mapGet" => Value {
                result_payload: Some(Box::new(element(0).unwrap_or_else(Value::unknown_callable))),
                ..Value::default()
            },
            "mapSet" => {
                let mut merged = element(0);
                if let Some(value) = arguments
                    .get(2)
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "mapValues" => aggregate(element(0)),
            _ => return builtin_return_shape(name),
        })
    }

    pub(super) fn lazy_iterator_value(
        &self,
        name: &str,
        arguments: &[Expr],
        scope: &[String],
        env: &CallableEnv,
    ) -> Value {
        let source = arguments
            .first()
            .and_then(|argument| self.value(argument, scope, env))
            .unwrap_or_default();
        let source_element = source.element.as_deref().cloned();
        let mut deferred = source.deferred;
        let callback = arguments
            .get(1)
            .and_then(|argument| self.callable(argument, scope, env));
        let callback_arguments = vec![source_element.clone()];
        if let Some(callback) = callback.clone() {
            deferred.union(self.invoke_with_values(callback, &callback_arguments));
        }
        let element = if matches!(name, "map" | "mapList") {
            callback.and_then(|callback| match callback {
                Callable::Known(known) => known
                    .returned
                    .as_deref()
                    .cloned()
                    .map(|returned| self.substitute_value_at(returned, 0, &callback_arguments)),
                Callable::Parameter { .. } | Callable::Unknown => None,
            })
        } else {
            source_element
        };
        Value {
            element: element.map(Box::new),
            deferred,
            field_names: Some(BTreeSet::new()),
            ..Value::default()
        }
    }

    pub(super) fn receiver_first(&self, receiver: &Expr, arguments: &[Expr]) -> Vec<Expr> {
        let originals: Vec<_> = std::iter::once(receiver).chain(arguments).collect();
        let copies: Vec<_> = originals
            .iter()
            .map(|expression| (*expression).clone())
            .collect();
        for (source, copy) in originals.into_iter().zip(&copies) {
            crate::arithmetic::transfer(
                source,
                copy,
                &mut self.instances.expression_types.borrow_mut(),
            );
        }
        copies
    }

    pub(super) fn pipe_call(
        &self,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        match right {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.call(
                function,
                &self.receiver_first(left, arguments),
                named_arguments,
                scope,
                env,
            ),
            _ => self.call(right, std::slice::from_ref(left), &[], scope, env),
        }
    }

    pub(super) fn pipe_value(
        &self,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        match right {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.call_value(
                function,
                &self.receiver_first(left, arguments),
                named_arguments,
                scope,
                env,
            ),
            _ => self.call_value(right, std::slice::from_ref(left), &[], scope, env),
        }
    }
}
