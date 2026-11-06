//! Exclude callback representation adaptation before considering a rewrite.

use osprey_ast::Position;

use crate::redundant::Snapshot;
use crate::{has_type_var, Type};

pub(super) fn exact_callback(snapshot: &Snapshot, name: &str, position: Position) -> bool {
    let Some(function) = snapshot.types.get(&format!("fn {name}")) else {
        return false;
    };
    let lambda = format!("lambda {}:{}", position.line, position.column);
    let Type::Fun { ret, .. } = function else {
        return false;
    };
    !has_type_var(function)
        && snapshot.types.get(&lambda) == Some(function)
        && !has_adapting_context(snapshot, ret)
}

fn has_adapting_context(snapshot: &Snapshot, result: &Type) -> bool {
    snapshot.types.iter().any(|(label, ty)| {
        if label.starts_with("fn ") {
            nested_adaptation(ty, result)
        } else {
            adapting_callback(ty, result)
        }
    })
}

fn adapting_callback(expected: &Type, actual: &Type) -> bool {
    let adapts = match expected {
        Type::Fun { params, ret } => {
            params.is_empty() && !matches!(ret.as_ref(), Type::Var(_)) && ret.as_ref() != actual
        }
        _ => false,
    };
    adapts || nested_adaptation(expected, actual)
}

// Be conservative across the whole checked program: callable contexts that
// demand another return representation can introduce a thunk conversion.
// Published lambda types do not record that conversion (e.g. int -> Result),
// so identical inferred types alone cannot prove a safe replacement there.
fn nested_adaptation(ty: &Type, actual: &Type) -> bool {
    match ty {
        Type::Fun { params, ret } => {
            params.iter().any(|param| adapting_callback(param, actual))
                || adapting_callback(ret, actual)
        }
        Type::Con { args, .. } | Type::Union { variants: args, .. } => {
            args.iter().any(|arg| adapting_callback(arg, actual))
        }
        Type::Record { fields, .. } => fields
            .values()
            .any(|field| adapting_callback(field, actual)),
        Type::Var(_) => false,
    }
}
