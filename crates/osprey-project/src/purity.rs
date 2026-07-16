//! Conservative compile-time initializer purity classification.

use osprey_ast::{Expr, InterpolatedPart};

pub(crate) fn is_pure(expression: &Expr) -> bool {
    match expression {
        Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => true,
        Expr::InterpolatedStr(parts) => parts.iter().all(|part| match part {
            InterpolatedPart::Text(_) => true,
            InterpolatedPart::Expr(value) => is_pure(value),
        }),
        Expr::List(items) => items.iter().all(is_pure),
        Expr::Map(entries) => entries
            .iter()
            .all(|entry| is_pure(&entry.key) && is_pure(&entry.value)),
        Expr::Object(fields) | Expr::TypeConstructor { fields, .. } => {
            fields.iter().all(|field| is_pure(&field.value))
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            is_pure(left) && is_pure(right)
        }
        Expr::Unary { operand, .. }
        | Expr::FieldAccess {
            target: operand, ..
        } => is_pure(operand),
        Expr::Index { target, index } => is_pure(target) && is_pure(index),
        _ => false,
    }
}
