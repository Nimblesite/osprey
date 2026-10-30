//! Effect row validate handlers.
use super::{
    fibered_operation, operation_pairs, validate_gpu_kernel, validate_statement_handlers,
    walk_children, Analyzer, CallableEnv, Expr, TypeError,
};

pub(super) fn validate_handler_arms(
    analyzer: &Analyzer<'_>,
    axis: &crate::multiplicity::OperationAxis,
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    validate_gpu_kernel(analyzer, expression, scope, env, errors);
    if let Expr::Block {
        statements, value, ..
    } = expression
    {
        let mut local = env.clone();
        validate_statement_handlers(analyzer, axis, statements, scope, &mut local, errors);
        if let Some(value) = value {
            validate_handler_arms(analyzer, axis, value, scope, &local, errors);
        }
        return;
    }
    if let Expr::Handler {
        effect,
        arms,
        body,
        return_clause,
        finally,
        stage,
        ..
    } = expression
    {
        // The row [MULTI-REPLAY-COARSE] reads: every operation the handled
        // expression requires, before this handler discharges any of them.
        let handled_row = operation_pairs(&analyzer.expression(body, scope, env));
        let fibered = fibered_operation(analyzer, effect, body, scope, env);
        for arm in arms {
            errors.extend(crate::multiplicity::arm_errors(
                axis,
                effect,
                arm,
                &handled_row,
                fibered.as_deref(),
            ));
            let local = analyzer.handler_arm_env(effect, arm, body, *stage, scope, env);
            let row = analyzer.expression(&arm.body, scope, &local);
            if stage.is_compile_time() && !row.runtime_builtins.is_empty() {
                errors.push(
                    TypeError::new(format!(
                        "static handler arm `{effect}.{}` requires runtime builtins: {}",
                        arm.operation,
                        row.runtime_builtins
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .with_pos(arm.position),
                );
            }
            // An arm performing its OWN operation forwards the request to the
            // enclosing handler — the arm's requirements stay obligations of
            // the region around this handler, which is exactly how a handler
            // delegates part of an interface outward. With no outer handler the
            // requirement survives to program entry and is reported there as an
            // unhandled operation. Implements [EFFECTS-STATIC-DISCHARGE].
            validate_handler_arms(analyzer, axis, &arm.body, scope, &local, errors);
        }
        if stage.is_compile_time() {
            validate_static_escape(analyzer, expression, scope, env, errors);
        }
        let body_env = env.under_handler(effect, arms, *stage);
        validate_handler_arms(analyzer, axis, body, scope, &body_env, errors);
        for clause in return_clause.iter().chain(finally) {
            validate_handler_arms(analyzer, axis, clause, scope, env, errors);
        }
        return;
    }
    walk_children(expression, |child| {
        validate_handler_arms(analyzer, axis, child, scope, env, errors);
    });
}

// Fail closed rather than bind future operations to the construction handler.
// The lowering pass cannot yet re-specialize an escaping callable at invocation.
// Implements [EFFECTS-STATIC-DISCHARGE], [STAGE-LOWER-ORDER].
fn validate_static_escape(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    let escaped = escaping_operations(analyzer, expression, scope, env);
    if let Expr::Handler {
        effect, position, ..
    } = expression
    {
        if !escaped.is_empty() {
            errors.push(TypeError::new(format!("static handler `{effect}` returns deferred work requiring {}; escaping static work requires invocation-time specialization, which is not implemented", escaped.join(", "))).with_pos(*position));
        }
    }
}

fn escaping_operations(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
) -> Vec<String> {
    let Expr::Handler {
        effect,
        arms,
        position,
        ..
    } = expression
    else {
        return Vec::new();
    };
    let Some(value) = analyzer.value(expression, scope, env) else {
        return Vec::new();
    };
    let arguments =
        analyzer.instance_arguments(effect, *position, &analyzer.instances.handlers, "handler");
    latent_operations(&value)
        .into_iter()
        .filter(|required| {
            required.effect == osprey_ast::effect_name::base(effect)
                && required.arguments == arguments
                && arms.iter().any(|arm| arm.operation == required.operation)
        })
        .map(|required| super::requirement_name(&required))
        .collect()
}

fn latent_operations(value: &super::Value) -> super::Requirements {
    // Lazy iterator callbacks run when consumed, after this handler may have
    // ended. Quarantine their escape as well as ordinary closures until
    // invocation-time specialization preserves [STAGE-LOWER-ORDER].
    let mut operations = value.deferred.required.clone();
    if let Some(super::Callable::Alternatives(choices)) = &value.callable {
        for choice in choices {
            operations.extend(latent_operations(&super::Value::from_callable(
                choice.clone(),
            )));
        }
    }
    if let Some(super::Callable::Known(callable)) = &value.callable {
        operations.extend(callable.summary.required.iter().cloned());
        if let Some(returned) = &callable.returned {
            operations.extend(latent_operations(returned));
        }
    }
    for nested in value
        .fields
        .values()
        .chain(value.element.as_deref())
        .chain(value.result_payload.as_deref())
        .chain(value.fiber_payload.as_deref())
    {
        operations.extend(latent_operations(nested));
    }
    operations
}
