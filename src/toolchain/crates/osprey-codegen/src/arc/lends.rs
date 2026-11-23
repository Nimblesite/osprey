//! Lent closures [MEM-OWNERSHIP-STATIC-CLOSURES]
//! (docs/specs/compiler/0018-MemoryManagement.md). Under `--memory=ownership`
//! a closure written as an argument of a call to an Osprey function borrows
//! what it captures. Its cell lives until the calling statement ends, the
//! captured bindings outlive that statement, and a checked callee cannot keep
//! a parameter, so the cell owns none of its captures and frees none.
use super::Codegen;
use crate::llty::Value;
use osprey_ast::{AstNode, Expr, Position};

/// Whether a call names an Osprey function or a local function value: code
/// this compilation checks, which therefore keeps no parameter.
fn checked_callee(cg: &Codegen, function: &Expr) -> bool {
    match function {
        Expr::Identifier(name) => cg.fn_params.contains_key(name) || cg.lookup(name).is_some(),
        Expr::TypeApply { function, .. } => checked_callee(cg, function),
        _ => false,
    }
}

/// `argument` is passed to a checked callee: a closure literal lends, and so do
/// a list literal and every closure literal in it.
fn note_argument(cg: &mut Codegen, argument: &Expr) {
    match argument {
        Expr::Lambda {
            position: Some(at), ..
        } => {
            let _ = cg.ownership.lent_closures.insert((at.line, at.column));
        }
        Expr::List(items, Some(at)) => {
            let _ = cg.ownership.lent_lists.insert((at.line, at.column));
            for item in items
                .iter()
                .filter(|item| matches!(item, Expr::Lambda { .. }))
            {
                note_argument(cg, item);
            }
        }
        _ => {}
    }
}

/// Note every closure literal under `node` that is an argument of a call to a
/// checked callee.
fn note(cg: &mut Codegen, node: AstNode<'_>) {
    if let AstNode::Expression(Expr::Call {
        function,
        arguments,
        named_arguments,
    }) = node
    {
        if checked_callee(cg, function) {
            let named = named_arguments.iter().map(|argument| &argument.value);
            for argument in arguments.iter().chain(named) {
                note_argument(cg, argument);
            }
        }
    }
    node.for_each_child(|child| note(cg, child));
}

/// `value` is about to be lowered: find the closures in it that only lend.
pub(super) fn note_lent_closures(cg: &mut Codegen, value: &Expr) {
    if cg.ownership.on {
        note(cg, AstNode::Expression(value));
    }
}

/// Whether the closure written at `position` lends its captures.
pub(crate) fn lends(cg: &Codegen, position: Option<Position>) -> bool {
    position.is_some_and(|at| cg.ownership.lent_closures.contains(&(at.line, at.column)))
}

/// Whether the list literal written at `position` only lends: it is passed
/// straight to a checked callee, so it dies with the statement and holds its
/// elements without owning them. Their owners all outlive it.
pub(crate) fn lends_list(cg: &Codegen, position: Option<Position>) -> bool {
    cg.ownership.on
        && position.is_some_and(|at| cg.ownership.lent_lists.contains(&(at.line, at.column)))
}

/// Whether the innermost region holds `v` as something that only lends.
pub(crate) fn is_lent(cg: &Codegen, v: &Value) -> bool {
    let frame = cg.arc.frames.last();
    frame.is_some_and(|frame| frame.iter().any(|e| e.lent && e.operand == v.operand))
}

/// Build the next closure cell without owning its captures, or as usual.
pub(crate) fn set_lending(cg: &mut Codegen, lending: bool) {
    cg.arc.lending = lending;
}

/// Whether the closure cell being built lends its captures.
pub(crate) fn lending(cg: &Codegen) -> bool {
    cg.arc.lending
}

/// The just-owned cell `v` holds borrowed captures: it must die with the
/// statement that made it, so nothing may take it over.
pub(crate) fn mark_lent(cg: &mut Codegen, v: &Value) {
    let newest = cg.arc.frames.last_mut().and_then(|frame| frame.last_mut());
    if let Some(entry) = newest.filter(|entry| entry.operand == v.operand) {
        entry.lent = true;
    }
}
