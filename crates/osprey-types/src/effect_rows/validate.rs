//! Effect row validate.
use super::{
    requirement_name, validate_handler_arms, Analyzer, CallableEnv, Stmt, Summary, TypeError,
};

/// File initializers run before program entry installs its policies. [ARITH-EFFECT-CONST]
pub(super) fn validate_arithmetic_initializers(
    analyzer: &Analyzer<'_>,
    statements: &[Stmt],
    errors: &mut Vec<TypeError>,
) {
    let mut env = CallableEnv::default();
    for statement in statements {
        if let Stmt::Let {
            value, position, ..
        } = statement
        {
            let summary = analyzer.expression(value, &[], &env);
            if summary
                .required
                .iter()
                .any(|request| request.effect == osprey_ast::ARITH_EFFECT)
            {
                errors.push(TypeError::new("fallible arithmetic in a file-scope initializer; move it into a handled region").with_pos(*position));
            }
        }
        let _ = analyzer.statements(std::slice::from_ref(statement), &[], &mut env);
    }
}

pub(super) fn entry_errors(entry: &Summary, context: &str, show_dynamic: bool) -> Vec<TypeError> {
    let mut errors = Vec::new();
    if !entry.required.is_empty() {
        let operations = entry
            .required
            .iter()
            .map(requirement_name)
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(TypeError::new(format!(
            "unhandled effect operations at {context}: {operations}; add a matching `handle`"
        )));
    }
    if !entry.parameter_uses.is_empty() {
        errors.push(TypeError::new(format!(
            "{context} invokes an effect-polymorphic callback whose effects cannot be discharged"
        )));
    }
    if show_dynamic && entry.unresolved_dynamic_call {
        errors.push(TypeError::new(format!(
            "{context} invokes a dynamic callable whose effect provenance cannot be proven; preserve the callable through a statically tracked value path"
        )));
    }
    errors
}

pub(super) fn validate_statement_handlers(
    analyzer: &Analyzer<'_>,
    axis: &crate::multiplicity::OperationAxis,
    statements: &[Stmt],
    scope: &[String],
    env: &mut CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    for statement in statements {
        let value = match statement {
            Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
                Some(value)
            }
            _ => None,
        };
        if let Some(value) = value {
            validate_handler_arms(analyzer, axis, value, scope, env, errors);
        }
        match statement {
            Stmt::Let { name, value, .. } | Stmt::Assignment { name, value, .. } => {
                let _ = env.shadowed.insert(name.clone());
                if let Some(provenance) = analyzer.value(value, scope, env) {
                    let _ = env.values.insert(name.clone(), provenance);
                } else {
                    let _ = env.values.remove(name);
                }
            }
            _ => {}
        }
    }
}
