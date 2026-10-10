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
            let local = analyzer.handler_arm_env(effect, arm, body, scope, env);
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
        let body_env = env.under_handler(effect, arms, *stage);
        validate_handler_arms(analyzer, axis, body, scope, &body_env, errors);
        if let Some(clause) = return_clause {
            validate_handler_arms(analyzer, axis, clause, scope, env, errors);
        }
        return;
    }
    walk_children(expression, |child| {
        validate_handler_arms(analyzer, axis, child, scope, env, errors);
    });
}
