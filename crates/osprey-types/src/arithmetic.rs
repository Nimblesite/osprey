//! Arithmetic values and implicit requests. Implements [ARITH-EFFECT].

use crate::error::TypeError;
use crate::ty::{names, Type};
use osprey_ast::{AstNode, Expr, Position};
use std::collections::HashMap;

/// Evaluate literal arithmetic without executing a fallible instruction.
/// Implements [ARITH-EFFECT-CONST]; a zero divisor remains a runtime request.
///
/// # Errors
///
/// Returns the `constant arithmetic overflows: …` diagnostic when a folded
/// integer result is outside the `int` range.
pub fn fold_arithmetic(expr: &Expr) -> Result<Option<Expr>, String> {
    osprey_ast::with_stack(|| match expr {
        Expr::Integer(_) | Expr::Float(_) => Ok(Some(expr.clone())),
        Expr::Binary {
            op, left, right, ..
        } => match (fold_arithmetic(left)?, fold_arithmetic(right)?) {
            (Some(left), Some(right)) => fold_binary(op, &left, &right),
            _ => Ok(None),
        },
        Expr::Unary { op, operand } if op == "-" => fold_negation(operand),
        _ => Ok(None),
    })
}

/// The overflow a constant expression introduces at its own operator, located
/// there. An overflowing operand is reported at that operand instead, so one
/// fault is one diagnostic. Implements [ARITH-EFFECT-CONST].
pub(crate) fn constant_overflow(expr: &Expr) -> Option<TypeError> {
    let operands_fold = match expr {
        Expr::Binary { left, right, .. } => {
            fold_arithmetic(left).is_ok() && fold_arithmetic(right).is_ok()
        }
        Expr::Unary { op, operand } if op == "-" => fold_arithmetic(operand).is_ok(),
        _ => false,
    };
    let message = fold_arithmetic(expr).err().filter(|_| operands_fold)?;
    Some(TypeError::new(message).with_pos(operator_position(expr)))
}

/// A binary operator's own position; a negation borrows its operand's.
fn operator_position(expr: &Expr) -> Option<Position> {
    match expr {
        Expr::Binary { position, .. } => *position,
        Expr::Unary { operand, .. } => operator_position(operand),
        _ => None,
    }
}

fn fold_negation(operand: &Expr) -> Result<Option<Expr>, String> {
    match fold_arithmetic(operand)? {
        Some(Expr::Integer(value)) => value
            .checked_neg()
            .map(Expr::Integer)
            .map(Some)
            .ok_or_else(|| format!("constant arithmetic overflows: -({value})")),
        Some(Expr::Float(value)) => Ok(Some(Expr::Float(-value))),
        _ => Ok(None),
    }
}

fn fold_binary(op: &str, left: &Expr, right: &Expr) -> Result<Option<Expr>, String> {
    if let (Expr::Integer(left), Expr::Integer(right)) = (left, right) {
        return fold_integers(op, *left, *right);
    }
    Ok(None)
}

fn fold_integers(op: &str, left: i64, right: i64) -> Result<Option<Expr>, String> {
    let value = match op {
        "+" => left.checked_add(right),
        "-" => left.checked_sub(right),
        "*" => left.checked_mul(right),
        "%" if right != 0 => Some(if right == -1 { 0 } else { left % right }),
        _ => return Ok(None),
    };
    value
        .map(Expr::Integer)
        .map(Some)
        .ok_or_else(|| format!("constant arithmetic overflows: {left} {op} {right}"))
}

/// The row seed for an operator, after resolving its numeric overload.
pub(crate) fn operation(expr: &Expr, ty: Option<&Type>) -> Option<&'static str> {
    if !matches!(fold_arithmetic(expr), Ok(None)) {
        return None;
    }
    match expr {
        Expr::Binary { op, right, .. } if op == "/" || op == "%" => {
            if nonzero_literal(right) {
                return None;
            }
            Some(
                if op == "/" || ty.is_some_and(|ty| ty.is_named(names::FLOAT)) {
                    "divideByZero"
                } else {
                    "remainderByZero"
                },
            )
        }
        Expr::Binary { op, .. } if matches!(op.as_str(), "+" | "-" | "*") => {
            ty.filter(|ty| ty.is_named(names::INT)).map(|_| "overflow")
        }
        Expr::Unary { op, .. } if op == "-" => {
            ty.filter(|ty| ty.is_named(names::INT)).map(|_| "overflow")
        }
        _ => None,
    }
}

fn nonzero_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Integer(n) if *n != 0) || matches!(expr, Expr::Float(n) if *n != 0.0)
}

/// Discharge proven-total direct numeric builtin calls, retaining alias requirements.
pub(crate) fn total_builtin_operations(name: &str, arguments: &[Expr]) -> Vec<&'static str> {
    let integer = |index| {
        arguments
            .get(index)
            .and_then(|expr| match fold_arithmetic(expr) {
                Ok(Some(Expr::Integer(n))) => Some(n),
                _ => None,
            })
    };
    match name {
        "abs" if integer(0).is_some_and(|n| n != i64::MIN) => vec!["overflow"],
        "intDiv" => {
            let mut total = Vec::new();
            if integer(1).is_some_and(|n| n != 0) {
                total.push("remainderByZero");
            }
            if integer(1).is_some_and(|n| n != -1) || integer(0).is_some_and(|n| n != i64::MIN) {
                total.push("overflow");
            }
            total
        }
        _ => Vec::new(),
    }
}

/// Preserve inferred site identities when pipe inference borrows a cloned call.
pub(crate) fn transfer(source: &Expr, original: &Expr, types: &mut HashMap<usize, Type>) {
    let mut pending = vec![(AstNode::Expression(source), AstNode::Expression(original))];
    while let Some((source, original)) = pending.pop() {
        if let (AstNode::Expression(source), AstNode::Expression(original)) = (source, original) {
            if let Some(ty) = types.get(&std::ptr::from_ref(source).addr()).cloned() {
                let _ = types.insert(std::ptr::from_ref(original).addr(), ty);
            }
        }
        let mut source_children = Vec::new();
        let mut original_children = Vec::new();
        source.for_each_child(|child| source_children.push(child));
        original.for_each_child(|child| original_children.push(child));
        pending.extend(source_children.into_iter().zip(original_children));
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::typecheck;
    use osprey_ast::Position;
    use osprey_syntax::Flavor;

    /// [ARITH-EFFECT-CONST] One overflowing fold is one located diagnostic,
    /// however many constant expressions enclose it.
    #[test]
    fn a_nested_constant_overflow_is_reported_once_at_its_operator() {
        for (flavor, source, column) in [
            (
                Flavor::Default,
                "print(((9223372036854775807 + 1) * 2) - 3)\n",
                28,
            ),
            (
                Flavor::Ml,
                "print (((9223372036854775807 + 1) * 2) - 3)\n",
                29,
            ),
        ] {
            let errors = typecheck(flavor, source);
            let overflow: Vec<_> = errors
                .iter()
                .filter(|e| e.message == "constant arithmetic overflows: 9223372036854775807 + 1")
                .collect();
            assert_eq!(overflow.len(), 1, "{flavor}: {errors:?}");
            assert_eq!(
                overflow.first().and_then(|e| e.position),
                Some(Position { line: 1, column }),
                "{flavor}: {errors:?}"
            );
        }
    }
}
