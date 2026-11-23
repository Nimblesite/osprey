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
            errors.push(escape_error(effect, &escaped, *position));
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

/// A reusable handler may receive its computation only at a later call site.
/// Carry the handler's boundary through symbolic return projections until
/// substitution reveals the actual closure, iterator, or aggregate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct StaticEscape {
    effect: String,
    location: (u32, u32),
    pub(super) operations: super::Requirements,
}

impl Analyzer<'_> {
    pub(super) fn record_replay_bounds(
        &self,
        mut row: super::Summary,
        effect: &str,
        arms: &[osprey_ast::HandlerArm],
    ) -> super::Summary {
        if row.parameter_uses.is_empty() {
            return row;
        }
        for arm in arms {
            if self
                .index
                .operations
                .multiplicity_of(effect, &arm.operation)
                == osprey_ast::Multiplicity::Many
            {
                let at = arm.position.map_or((0, 0), |p| (p.line, p.column));
                row = row.under(&super::Scope::Replay {
                    effect: effect.into(),
                    operation: arm.operation.clone(),
                    at,
                });
            }
        }
        row
    }

    pub(super) fn validate_callback_scopes(
        &self,
        row: super::Summary,
        scopes: &[super::Scope],
    ) -> super::Summary {
        scopes.iter().fold(row, |row, scope| {
            if let super::Scope::Replay {
                effect,
                operation,
                at,
            } = scope
            {
                if let Some(message) = crate::multiplicity::non_replayable_entry(
                    &self.index.operations,
                    effect,
                    operation,
                    &operation_pairs(&row),
                ) {
                    self.record_deferred_error(*at, TypeError::new(message));
                }
            }
            row.in_scope(scope)
        })
    }

    fn record_deferred_error(&self, (line, column): (u32, u32), error: TypeError) {
        let key = (line, column, error.message.clone());
        let _ = self
            .index
            .deferred_errors
            .borrow_mut()
            .insert(key, error.with_pos(Some(super::Position { line, column })));
    }

    pub(super) fn mark_static_result(
        &self,
        site: &super::handlers::HandlerSite<'_>,
        value: &mut super::Value,
    ) {
        let Some(pos) = site.position else { return };
        let args = self.instance_arguments(
            site.effect,
            site.position,
            &self.instances.handlers,
            "handler",
        );
        let operations = site
            .arms
            .iter()
            .map(|arm| super::Requirement::new(site.effect, &arm.operation, args.clone()))
            .collect();
        mark_symbolic_result(
            value,
            &StaticEscape {
                effect: site.effect.to_owned(),
                location: (pos.line, pos.column),
                operations,
            },
        );
    }

    pub(super) fn resolve_static_escape(
        &self,
        mut value: super::Value,
        boundary: &StaticEscape,
    ) -> super::Value {
        let escaped: Vec<_> = latent_operations(&value)
            .iter()
            .filter(|required| boundary.operations.contains(&required.unmasked()))
            .map(super::requirement_name)
            .collect();
        if !escaped.is_empty() {
            self.record_deferred_error(
                boundary.location,
                escape_error(&boundary.effect, &escaped, None),
            );
        }
        mark_symbolic_result(&mut value, boundary);
        value
    }
}

fn mark_symbolic_result(value: &mut super::Value, boundary: &StaticEscape) {
    for child in value.container_children_mut() {
        mark_symbolic_result(child, boundary);
    }
    match &mut value.callable {
        Some(super::Callable::Parameter { projection, .. }) => {
            let step = super::Projection::StaticEscape(Box::new(boundary.clone()));
            if projection.last() != Some(&step) {
                projection.push(step);
            }
        }
        Some(super::Callable::Alternatives(choices)) => {
            super::provenance::map_alternatives(choices, |branch| {
                mark_symbolic_result(branch, boundary);
            });
        }
        Some(super::Callable::Known(known)) => {
            if let Some(returned) = &mut known.returned {
                mark_symbolic_result(returned, boundary);
            }
        }
        _ => {}
    }
}

fn escape_error(effect: &str, escaped: &[String], position: Option<super::Position>) -> TypeError {
    TypeError::new(format!("static handler `{effect}` returns deferred work requiring {}; escaping static work requires invocation-time specialization, which is not implemented", escaped.join(", "))).with_pos(position)
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
    for nested in value.nested() {
        operations.extend(latent_operations(nested));
    }
    operations
}
