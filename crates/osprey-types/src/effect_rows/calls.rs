//! Effect row calls.
use super::{
    eager_callback_slots, expression_name, expression_site, gpu_kernel_slot, iterator_consumer,
    statically_named_callee, type_can_call, Analyzer, Callable, CallableEnv, Expr, ExprArgs,
    NamedArgument, Requirement, Summary, Value,
};

#[derive(Clone, Copy)]
struct SourceCall<'a> {
    function: &'a Expr,
    arguments: ExprArgs<'a>,
    named: &'a [NamedArgument],
}

impl<'a> SourceCall<'a> {
    fn read(expression: &'a Expr) -> Option<Self> {
        match expression {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => Some(Self {
                function,
                arguments: ExprArgs::new(arguments),
                named: named_arguments,
            }),
            _ => None,
        }
    }
}

impl Analyzer<'_> {
    pub(super) fn call(
        &self,
        function: &Expr,
        arguments: ExprArgs<'_>,
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let source = SourceCall {
            function,
            arguments,
            named: named_arguments,
        };
        let mut out = self.call_children(source, scope, env);
        out.union(self.call_effects(function, arguments, named_arguments, scope, env));
        out
    }

    /// Share one argument model between a known call's immediate row and its
    /// returned value. Unsupported transports keep the complete original path.
    pub(super) fn known_call_model(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<(Summary, Value)> {
        let source = self.known_source_call(expression, scope, env)?;
        osprey_ast::with_stack(|| self.model_source_call(expression, source, scope, env))
    }

    fn known_source_call<'b>(
        &self,
        expression: &'b Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<SourceCall<'b>> {
        let source = SourceCall::read(expression)?;
        self.shared_call_eligible(expression, source.function, scope, env)
            .then_some(source)
    }

    fn shared_call_eligible(
        &self,
        expression: &Expr,
        function: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> bool {
        crate::methods::parts(expression).is_none()
            && (expression_name(function) != Some("Channel") || env.shadowed.contains("Channel"))
            && self.builtin_callee(function, scope, env).is_none()
            && !self.foreign_callee(function, scope, env)
            && self.known_callee_source(function, scope, env)
    }

    fn known_callee_source(&self, function: &Expr, scope: &[String], env: &CallableEnv) -> bool {
        match function {
            Expr::Lambda { .. } => true,
            Expr::Identifier(name) => self.known_identifier_source(name, scope, env),
            Expr::Path(path) => self.index.resolve(scope, &path.to_string()).is_some(),
            Expr::TypeApply { function, .. } => self.known_callee_source(function, scope, env),
            _ => false,
        }
    }

    fn known_identifier_source(&self, name: &str, scope: &[String], env: &CallableEnv) -> bool {
        match env.values.get(name) {
            Some(value) => {
                value.performed.is_empty()
                    && matches!(value.callable.as_ref(), Some(Callable::Known(_)))
            }
            None => !env.shadowed.contains(name) && self.index.resolve(scope, name).is_some(),
        }
    }

    fn call_children(
        &self,
        source: SourceCall<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut summary = self.expression(source.function, scope, env);
        summary.union(self.union_over(source.arguments.iter(), scope, env));
        summary.union(self.named_expressions(source.named, scope, env));
        summary
    }

    fn source_call_values(
        &self,
        source: SourceCall<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> super::CallArguments {
        self.callsite_arguments(source.function, source.arguments, source.named, scope, env)
    }

    fn model_source_call(
        &self,
        expression: &Expr,
        source: SourceCall<'_>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<(Summary, Value)> {
        let mut summary = self.call_children(source, scope, env);
        let Callable::Known(known) = self.callable(source.function, scope, env)? else {
            return None;
        };
        let arguments = self.source_call_values(source, scope, env);
        let (called, returned) = self.model_known_call(*known, &arguments);
        summary.union(called);
        self.finish_call_model(expression, summary, returned, env)
    }

    fn finish_call_model(
        &self,
        expression: &Expr,
        summary: Summary,
        returned: Option<Value>,
        env: &CallableEnv,
    ) -> Option<(Summary, Value)> {
        let summary = self.finish_expression_summary(expression, summary);
        returned.map(|value| {
            let value = Self::resolve_handled_value(value, &env.handler_returns);
            (summary, self.typed_value(expression, value, None))
        })
    }

    pub(super) fn call_effects(
        &self,
        function: &Expr,
        arguments: ExprArgs<'_>,
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        if self.foreign_callee(function, scope, env) {
            out.union(self.foreign_argument_effects(
                function,
                arguments,
                named_arguments,
                scope,
                env,
            ));
        }
        if let Some(callee) = self.call_effects_callable(function, scope, env) {
            let arguments =
                self.callsite_arguments(function, arguments, named_arguments, scope, env);
            out.union(self.invoke_with_arguments(callee, arguments));
        } else if !statically_named_callee(function, env)
            && self.may_be_callable(function)
            && !env.deferred_arm_parameter(function)
        {
            // A computed value that successfully type-checks as a function must
            // carry effect provenance. If an unsupported transport erased that
            // provenance, fail closed instead of assuming the call is pure. A
            // compile-time arm's parameter is the exception: discharge binds
            // it to the actual operand before the residual program is checked.
            out.unresolved_dynamic_call = true;
        }
        if let Some(name) = self.builtin_callee(function, scope, env) {
            if matches!(
                name,
                "signalDependencies" | "signalSnapshot" | "signalRefresh"
            ) {
                let slot = if name == "signalRefresh" { 2 } else { 0 };
                let query = super::names::signal_argument(slot, arguments, named_arguments)
                    .and_then(|argument| self.callable(argument, scope, env))
                    .map_or_else(
                        || Summary {
                            unresolved_dynamic_call: true,
                            ..Summary::default()
                        },
                        |callback| self.invoke_with_values(callback, &[]),
                    );
                let _ = self.instances.signal_queries.borrow_mut().insert(
                    expression_site(function),
                    super::reports::report(self.instances, query),
                );
            }
            for operation in crate::arithmetic::total_builtin_operations(name, arguments) {
                let _ = out.required.remove(&Requirement::new(
                    osprey_ast::ARITH_EFFECT,
                    operation,
                    Vec::new(),
                ));
            }
            if iterator_consumer(name) {
                if let Some(iterator) = arguments
                    .first()
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    out.union(iterator.deferred);
                }
            }
            // Stage legality permits host arithmetic but cannot discharge it.
            // GPU callbacks execute here like other eager callbacks. [ARITH-EFFECT-DISCHARGE]
            for index in eager_callback_slots(name)
                .iter()
                .copied()
                .chain(gpu_kernel_slot(name))
            {
                if let Some(argument) = arguments.get(index).or_else(|| {
                    matches!(name, "signalSnapshot" | "signalRefresh")
                        .then(|| {
                            named_arguments
                                .iter()
                                .find(|argument| argument.name == "work")
                                .map(|argument| &argument.value)
                        })
                        .flatten()
                }) {
                    if let Some(callback) = self.callable(argument, scope, env) {
                        let callback_arguments =
                            self.eager_callback_arguments(name, arguments, scope, env);
                        out.union(self.invoke_with_values(callback, &callback_arguments));
                    }
                }
            }
        }
        out
    }

    fn foreign_argument_effects(
        &self,
        function: &Expr,
        arguments: ExprArgs<'_>,
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        let name = match function {
            Expr::Path(path) => Some(path.to_string()),
            _ => expression_name(function).map(str::to_owned),
        };
        let signature = name
            .as_deref()
            .and_then(|name| self.extern_signature(scope, name));
        let parameters = name
            .as_deref()
            .and_then(|name| self.index.extern_parameters(scope, name));
        for (index, argument) in arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| (Some(index), argument))
            .chain(named.iter().map(|argument| {
                (
                    parameters.and_then(|parameters| {
                        parameters.iter().position(|name| name == &argument.name)
                    }),
                    &argument.value,
                )
            }))
        {
            let expected =
                index.and_then(|index| signature.and_then(|(parameters, _)| parameters.get(index)));
            if expected.map_or_else(|| self.may_be_callable(argument), type_can_call) {
                let effects = self.callable(argument, scope, env).map_or_else(
                    || Summary {
                        unresolved_dynamic_call: true,
                        ..Summary::default()
                    },
                    |callback| self.invoke_with_values(callback, &[]),
                );
                out.union(effects);
            }
        }
        out
    }

    pub(super) fn may_be_callable(&self, function: &Expr) -> bool {
        match self
            .instances
            .expression_types
            .borrow()
            .get(&expression_site(function))
        {
            Some(ty) => type_can_call(ty),
            None => true,
        }
    }

    pub(super) fn may_be_callable_field(&self, target: &Expr, field: &str) -> bool {
        match self
            .instances
            .expression_types
            .borrow()
            .get(&expression_site(target))
        {
            Some(crate::ty::Type::Record { fields, .. }) => {
                fields.get(field).is_none_or(type_can_call)
            }
            _ => true,
        }
    }
}
