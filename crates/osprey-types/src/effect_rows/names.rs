//! Effect row names.
use super::{CallableEnv, Expr, HashMap, Index, Position, Requirement};

/// Method calls and piped calls are both plain calls whose receiver becomes the
/// first argument; every desugaring site builds that argument list here.
pub(super) fn expression_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::TypeApply { function, .. } => expression_name(function),
        Expr::Identifier(name) => Some(name),
        Expr::Path(path) => path.last(),
        _ => None,
    }
}

/// Only a declaration resolved at this call site supplies parameter names.
/// A tracked callback can be known while still being called through a slot.
pub(super) fn source_named_callee(
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
    index: &Index<'_>,
) -> bool {
    match expression {
        Expr::TypeApply { function, .. } => source_named_callee(function, scope, env, index),
        Expr::Identifier(name) => {
            !env.shadowed.contains(name)
                && !env.values.contains_key(name)
                && index.resolve(scope, name).is_some()
        }
        Expr::Path(path) => index.resolve(scope, &path.to_string()).is_some(),
        _ => false,
    }
}

/// Whether a callee this pass could not resolve is nonetheless known to be
/// effect-free, so the call may be skipped instead of failing closed.
///
/// Only a built-in earns that. Reaching here with an identifier means it named
/// neither a tracked value nor a function in the index, and the remaining
/// candidates are a built-in or a binding this pass cannot see — notably a
/// top-level `let`, which `function_body` never seeds into the environment.
/// Trusting every unshadowed name let `let siren = ring` / `fn relay() =
/// siren()` type-check with `Alarm.ring` never handled, because the call
/// contributed nothing at all, not even a provenance failure.
pub(super) fn statically_named_callee(expression: &Expr, env: &CallableEnv) -> bool {
    match expression {
        Expr::TypeApply { function, .. } => statically_named_callee(function, env),
        Expr::Identifier(name) => !env.shadowed.contains(name),
        Expr::Path(_) => true,
        _ => false,
    }
}

pub(super) fn type_can_call(ty: &crate::ty::Type) -> bool {
    matches!(ty, crate::ty::Type::Fun { .. } | crate::ty::Type::Var(_))
        || ty.is_named(crate::ty::names::ANY)
}

pub(super) fn expression_site(expression: &Expr) -> usize {
    std::ptr::from_ref(expression).addr()
}

pub(super) fn eager_callback_slots(name: &str) -> &'static [usize] {
    match name {
        "test" | "forEach" | "forEachList" | "httpListen" | "spawnProcess" | "mapList"
        | "filterList" => &[1],
        "fold" | "foldList" => &[2],
        _ => &[],
    }
}

pub(super) fn iterator_consumer(name: &str) -> bool {
    matches!(name, "forEach" | "fold")
}

pub(super) fn site_arguments(
    position: Option<Position>,
    sites: &HashMap<(u32, u32), Vec<String>>,
) -> Option<Vec<String>> {
    position
        .and_then(|position| sites.get(&(position.line, position.column)))
        .cloned()
}

pub(super) fn requirement_name(requirement: &Requirement) -> String {
    if requirement.arguments.is_empty() {
        format!("{}.{}", requirement.effect, requirement.operation)
    } else {
        format!(
            "{}<{}>.{}",
            requirement.effect,
            requirement.arguments.join(", "),
            requirement.operation
        )
    }
}
