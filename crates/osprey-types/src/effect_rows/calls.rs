//! Effect row calls.
use super::{
    eager_callback_slots, expression_site, gpu_kernel_slot, iterator_consumer,
    statically_named_callee, type_can_call, Analyzer, CallableEnv, Expr, NamedArgument,
    Requirement, Summary,
};

impl Analyzer<'_> {
    pub(super) fn call(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = self.expression(function, scope, env);
        out.union(self.expressions(arguments, scope, env));
        out.union(self.named_expressions(named_arguments, scope, env));
        out.union(self.call_effects(function, arguments, named_arguments, scope, env));
        out
    }

    pub(super) fn call_effects(
        &self,
        function: &Expr,
        arguments: &[Expr],
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
        if let Some(callee) = self.callable(function, scope, env) {
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
        arguments: &[Expr],
        named: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        let name = match function {
            Expr::Path(path) => Some(path.to_string()),
            _ => super::expression_name(function).map(str::to_owned),
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
