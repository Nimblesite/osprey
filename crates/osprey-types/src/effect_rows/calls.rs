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
        if let Some(callee) = self.callable(function, scope, env) {
            let arguments =
                self.callsite_arguments(function, arguments, named_arguments, scope, env);
            out.union(self.invoke_with_arguments(callee, arguments));
        } else if !statically_named_callee(function, env) && self.may_be_callable(function) {
            // A computed value that successfully type-checks as a function must
            // carry effect provenance. If an unsupported transport erased that
            // provenance, fail closed instead of assuming the call is pure.
            out.unresolved_dynamic_call = true;
        }
        if let Some(name) = self.builtin_callee(function, scope, env) {
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
                if let Some(argument) = arguments.get(index) {
                    if let Some(callback) = self.callable(argument, scope, env) {
                        out.union(self.invoke_with_values(callback, &[]));
                    }
                }
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
