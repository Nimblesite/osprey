//! Effect row call values.
use super::{
    builtin_return_shape, expression_name, merge_optional_value, ordered_values, project_element,
    source_named_callee, stabilize_merged_callables, Analyzer, BTreeSet, CallArguments, Callable,
    CallableEnv, Expr, ExprArgs, KnownCallable, NamedArgument, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn model_known_call(
        &self,
        known: KnownCallable,
        arguments: &CallArguments,
    ) -> (Summary, Option<Value>) {
        let values = ordered_values(&known.parameters, &arguments.positional, &arguments.named);
        let summary = self.substitute_summary_at(known.summary, 0, &values);
        (summary, self.project_bound_return(known.returned, &values))
    }

    pub(super) fn project_bound_return(
        &self,
        returned: Option<Box<Value>>,
        values: &[Option<Value>],
    ) -> Option<Value> {
        returned
            .map(|value| self.substitute_value_at(*value, 0, values))
            .or_else(|| Some(Value::unknown_value()))
    }

    pub(super) fn call_value(
        &self,
        function: &Expr,
        arguments: ExprArgs<'_>,
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        if let Some(name) = self.builtin_callee(function, scope, env) {
            if let Some(value) =
                self.builtin_call_value(name, arguments, named_arguments, scope, env)
            {
                return Some(value);
            }
        }
        // Preserve each application in a curried spine: its result can be a
        // returned closure or aggregate with different effects from its maker.
        let resolved = self.callable(function, scope, env);
        let arguments = self.callsite_arguments(function, arguments, named_arguments, scope, env);
        self.called_value(resolved, arguments)
            .map(|value| Self::resolve_handled_value(value, &env.handler_returns))
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
            && self.index.resolve(scope, name).is_none()
            && !self.foreign_callee(function, scope, env))
        .then_some(name)
    }

    pub(super) fn foreign_callee(
        &self,
        function: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> bool {
        match function {
            Expr::TypeApply { function, .. } => self.foreign_callee(function, scope, env),
            Expr::Identifier(name) => {
                !env.shadowed.contains(name)
                    && !env.values.contains_key(name)
                    && self.index.extern_parameters(scope, name).is_some()
            }
            Expr::Path(path) => self
                .index
                .extern_parameters(scope, &path.to_string())
                .is_some(),
            _ => false,
        }
    }

    pub(super) fn called_value(
        &self,
        resolved: Option<Callable>,
        arguments: CallArguments,
    ) -> Option<Value> {
        let Some(
            callee @ (Callable::Known(_) | Callable::Parameter { .. } | Callable::Alternatives(_)),
        ) = resolved
        else {
            // `value` uses the call's inferred type to distinguish a primitive
            // result from an aggregate whose contents remain unknown.
            return Some(Value::unknown_value());
        };
        self.project_call_return(Value::from_callable(callee), arguments)
    }

    pub(super) fn callsite_arguments(
        &self,
        function: &Expr,
        arguments: ExprArgs<'_>,
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> CallArguments {
        let arguments = self.call_arguments(arguments, named, scope, env);
        self.order_callsite_arguments(function, arguments, scope, env)
    }

    pub(super) fn order_callsite_arguments(
        &self,
        function: &Expr,
        arguments: CallArguments,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallArguments {
        if source_named_callee(function, scope, env, self.index) {
            arguments
        } else {
            arguments.in_written_order()
        }
    }

    pub(super) fn call_arguments(
        &self,
        arguments: ExprArgs<'_>,
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
        arguments: ExprArgs<'_>,
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        if matches!(name, "signalSnapshot" | "signalRefresh") {
            return Some(self.signal_snapshot_value(name, arguments, named, scope, env));
        }
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
                empty_case: (name != "range").then_some(super::EmptyCase::Collection),
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
                let mut merged = None;
                if let Some(collection) = arguments.first() {
                    self.merge_collection_elements(&mut merged, collection, scope, env);
                }
                merge_optional_value(
                    &mut merged,
                    arguments
                        .get(1)
                        .and_then(|argument| self.value(argument, scope, env))
                        .unwrap_or_default(),
                );
                aggregate(merged)
            }
            "listConcat" | "mapMerge" => {
                let mut merged = None;
                for collection in arguments.iter().take(2) {
                    self.merge_collection_elements(&mut merged, collection, scope, env);
                }
                let empty_case = merged.is_none().then_some(super::EmptyCase::Collection);
                Value {
                    empty_case,
                    ..aggregate(merged)
                }
            }
            "listReverse" | "listSortBy" | "mapRemove" => arguments
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
            "fold" | "foldList" => self.fold_value(arguments, scope, env),
            _ => return builtin_return_shape(name),
        })
    }

    /// `fold(list, acc, callback)` returns the final accumulator: the union of
    /// the initial accumulator and the callback's returned value, so callable
    /// provenance a callback stores into the accumulator — including through a
    /// recursive aggregate — flows into the fold result. The callback's own
    /// latent effects are invoked by `eager_callback_slots`. Implements
    /// [EFFECTS-PROVENANCE].
    fn fold_value(&self, arguments: ExprArgs<'_>, scope: &[String], env: &CallableEnv) -> Value {
        let mut acc = arguments
            .get(1)
            .and_then(|argument| self.value(argument, scope, env))
            .unwrap_or_default();
        self.merge_fold_return(&mut acc, arguments, scope, env);
        acc
    }

    /// One modelled iteration of `fold`: the callback sees the accumulator and
    /// the source's element, and whatever it returns joins the accumulator's
    /// provenance. Implements [EFFECTS-PROVENANCE].
    fn merge_fold_return(
        &self,
        acc: &mut Value,
        arguments: ExprArgs<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) {
        let Some(callback) = arguments
            .get(2)
            .and_then(|argument| self.callable(argument, scope, env))
        else {
            return;
        };
        let callback_arguments = self.fold_callback_arguments(acc, arguments, scope, env);
        if let Some(returned) =
            self.project_call_return(Value::from_callable(callback), callback_arguments)
        {
            acc.union(stabilize_merged_callables(&returned));
        }
    }

    /// What `fold` hands its callback: the accumulator so far and one element
    /// of the source list.
    fn fold_callback_arguments(
        &self,
        acc: &Value,
        arguments: ExprArgs<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallArguments {
        let source_element = arguments
            .first()
            .and_then(|argument| self.value(argument, scope, env))
            .and_then(project_element);
        CallArguments {
            positional: vec![Some(acc.clone()), source_element],
            named: Vec::new(),
        }
    }

    /// The arguments an eager callback slot passes its callback. Iterator and
    /// collection consumers hand over their element(s), so a callable stored
    /// in an element — and invoked through the callback's parameter — resolves
    /// against that element's provenance instead of failing closed as an
    /// unproven dynamic call. `fold` also hands back the accumulator, including
    /// whatever a previous iteration stored in it. Callbacks that take no
    /// data argument (`test`, `spawnProcess`) get none. Implements
    /// [EFFECTS-PROVENANCE].
    pub(super) fn eager_callback_arguments(
        &self,
        name: &str,
        arguments: ExprArgs<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Vec<Option<Value>> {
        let element = |arguments: ExprArgs<'_>| {
            arguments
                .first()
                .and_then(|argument| self.value(argument, scope, env))
                .and_then(project_element)
        };
        match name {
            "forEach" | "forEachList" | "mapList" | "filterList" => {
                vec![element(arguments)]
            }
            "listSortBy" => {
                let element = element(arguments);
                vec![element.clone(), element]
            }
            "fold" | "foldList" => {
                let accumulator = self.fold_value(arguments, scope, env);
                vec![Some(accumulator), element(arguments)]
            }
            _ => Vec::new(),
        }
    }

    /// Cached callable values keep their latent effects after the snapshot escapes.
    /// Implements [EFFECTS-PROVENANCE], [STAGE-SIGNALS-REBUILD].
    fn signal_snapshot_value(
        &self,
        name: &str,
        arguments: ExprArgs<'_>,
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Value {
        let slot = if name == "signalRefresh" { 2 } else { 0 };
        let mut value = super::names::signal_argument(slot, arguments, named)
            .and_then(|argument| self.callable(argument, scope, env))
            .and_then(|callback| self.called_value(Some(callback), CallArguments::default()));
        if name == "signalRefresh" {
            if let Some(old) = arguments
                .first()
                .or_else(|| {
                    named
                        .iter()
                        .find(|argument| argument.name == "cached")
                        .map(|argument| &argument.value)
                })
                .and_then(|argument| self.value(argument, scope, env))
                .and_then(|old| super::project_field(old, "value"))
            {
                merge_optional_value(&mut value, old);
            }
        }
        Value {
            field_names: Some(
                ["dependencies", "value", "revision"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            fields: [(
                "value".to_owned(),
                value.unwrap_or_else(Value::unknown_callable),
            )]
            .into_iter()
            .collect(),
            ..Value::default()
        }
    }

    pub(super) fn lazy_iterator_value(
        &self,
        name: &str,
        arguments: ExprArgs<'_>,
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
                callback @ Callable::Alternatives(_) => self.called_value(
                    Some(callback),
                    CallArguments {
                        positional: callback_arguments.clone(),
                        named: Vec::new(),
                    },
                ),
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

    pub(super) fn receiver_first<'b>(receiver: &'b Expr, arguments: &'b [Expr]) -> ExprArgs<'b> {
        ExprArgs::with_receiver(receiver, arguments)
    }

    /// `left |> right` answered by `apply` as the call it desugars to: the
    /// receiver leads a call's arguments, or is a bare callee's one argument.
    fn piped<'b, T>(
        left: &'b Expr,
        right: &'b Expr,
        apply: impl FnOnce(&Expr, ExprArgs<'b>, &[NamedArgument]) -> T,
    ) -> T {
        match right {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => apply(
                function,
                Self::receiver_first(left, arguments),
                named_arguments,
            ),
            _ => apply(right, ExprArgs::new(std::slice::from_ref(left)), &[]),
        }
    }

    pub(super) fn pipe_call(
        &self,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        Self::piped(left, right, |function, arguments, named| {
            self.call(function, arguments, named, scope, env)
        })
    }

    pub(super) fn pipe_value(
        &self,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        Self::piped(left, right, |function, arguments, named| {
            self.call_value(function, arguments, named, scope, env)
        })
    }
}
