//! Obligation keys for built-in constraints: which constraint a key names and
//! the generic function it travelled through [TYPE-CONSTRAINTS].

use super::{operation_name, source_position, ASSERTION, NUMERIC_OPERAND_PREFIX};

/// The built-in constraint an obligation names, and whether it reaches every
/// variable inside the constrained type (`printable` does: a list is
/// printable when its elements are) or only a bare variable.
/// [TYPE-CONSTRAINTS]
pub(crate) fn constraint_of(key: &str) -> Option<(&'static str, bool)> {
    let (operation, _) = split_via(operation_name(key));
    match operation {
        "print" | "toString" | "interpolation" | ASSERTION => Some(("printable", true)),
        "listSort" => Some(("sortable", true)),
        "length" | "isEmpty" => Some(("sized", false)),
        op if op.starts_with(crate::comparison::COMPARED_OPERAND_PREFIX) => {
            Some(("comparable", true))
        }
        op if op.starts_with(NUMERIC_OPERAND_PREFIX) => Some(("number", false)),
        _ => None,
    }
}

/// Joins an operation to the generic function a constraint travelled
/// through: `toString via show`.
const VIA: &str = " via ";

/// The constraint key `operation`, carried through a call to `callee`.
pub(crate) fn via(operation: &str, callee: &str) -> String {
    format!("{operation}{VIA}{callee}")
}

/// A located representation constraint's operation, unlocated, with any
/// earlier callee dropped: the one a caller re-locates at its own call.
/// [TYPE-CONSTRAINTS]
pub(crate) fn carried_constraint(name: &str) -> Option<String> {
    let _ = source_position(name)?;
    let (operation, _) = split_via(operation_name(name));
    is_carried(operation).then(|| operation.to_owned())
}

/// A bare representation constraint from `callee`'s scheme, named for the
/// generic function it came through; `None` when the call is the constrained
/// built-in itself. [TYPE-CONSTRAINTS]
pub(crate) fn carried_by(name: &str, callee: &str) -> Option<String> {
    (is_carried(name) && name != callee).then(|| via(name, callee))
}

/// A constraint that follows its operand out of a generic function: a
/// rendering, a size, an order or an assertion operand.
fn is_carried(operation: &str) -> bool {
    operation == "interpolation"
        || operation == ASSERTION
        || operation.starts_with(crate::comparison::COMPARED_OPERAND_PREFIX)
        || crate::expr::constrained_builtin(operation)
}

/// An operation and the outermost generic function it travelled through.
pub(crate) fn split_via(operation: &str) -> (&str, Option<&str>) {
    match operation.split_once(VIA) {
        Some((operation, callee)) => (operation, Some(callee)),
        None => (operation, None),
    }
}
