//! Captured values and shared mutable-cell identity across function boundaries.
use super::Capture;
use crate::builder::Codegen;
use crate::llty::{LType, Value};
use osprey_ast::freevars::free_idents;
use osprey_ast::{Expr, Parameter};
use std::collections::BTreeSet;

/// The free identifiers of `body` (minus the lambda's own parameters) that are
/// bound to a value in the enclosing scope — the closure's captures, in stable
/// (sorted) order. Also used by `fiber::gen_spawn` (a spawn body is a
/// zero-parameter closure).
pub(crate) fn capture_list(cg: &Codegen, parameters: &[Parameter], body: &Expr) -> Vec<Capture> {
    reaching_names(cg, parameters, body)
        .into_iter()
        .filter_map(|name| {
            cg.lookup(&name).map(|val| Capture {
                name,
                val,
                cell: None,
            })
        })
        .collect()
}

/// Retain handler-owned state when its handler is bound, passed or returned.
/// Implements [EFFECTS-HANDLER-VALUE-STATE].
pub(super) fn closure_captures(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
) -> Vec<Capture> {
    reaching_names(cg, parameters, body)
        .into_iter()
        .filter_map(|name| capture(cg, name))
        .collect()
}

/// The names a nested function must find in its enclosing scope: its own free
/// names, and the environment of every inline lambda it applies
/// ([`super::environment`]).
fn reaching_names(cg: &Codegen, parameters: &[Parameter], body: &Expr) -> BTreeSet<String> {
    let mut names = free_names(parameters, body);
    names.extend(super::environment::reach(cg, &names));
    names
}

fn capture(cg: &mut Codegen, name: String) -> Option<Capture> {
    if let Some(cell) = cg.cell_slots.get(&name).cloned() {
        let operand = cg.emit_reg(format!("bitcast {}* {} to i8*", cell.pointee, cell.ptr));
        Some(Capture {
            name,
            val: Value::new(operand, LType::Ptr),
            cell: Some(cell),
        })
    } else {
        cg.lookup(&name).map(|val| Capture {
            name,
            val,
            cell: None,
        })
    }
}

/// The free identifiers of `body` minus the lambda's own parameters, in stable
/// (sorted) order — what [`capture_list`] narrows to the bound ones, and what
/// kernel extraction tests against the host state a lifted body cannot see
/// ([`crate::gpu_kernel`]).
pub(crate) fn free_names(parameters: &[Parameter], body: &Expr) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    free_idents(body, &mut names);
    names.retain(|n| !parameters.iter().any(|p| &p.name == n));
    names
}

/// Inside the closure function: cast `%__env` back to the cell type and load
/// each capture into scope (parameters bind after, so they shadow correctly).
pub(crate) fn reload_captures(cg: &mut Codegen, cell_ty: &str, caps: &[Capture]) {
    if caps.is_empty() {
        return;
    }
    let cell = cg.emit_reg(format!("bitcast i8* %__env to {cell_ty}*"));
    for (i, capture) in caps.iter().enumerate() {
        reload_capture(cg, cell_ty, &cell, i + 1, capture);
    }
}

fn reload_capture(cg: &mut Codegen, cell_ty: &str, cell: &str, slot: usize, capture: &Capture) {
    let p = cg.emit_reg(format!(
        "getelementptr {cell_ty}, {cell_ty}* {cell}, i32 0, i32 {slot}"
    ));
    let lty = capture.val.llvm_ty();
    let operand = cg.emit_reg(format!("load {lty}, {lty}* {p}"));
    if let Some(cell) = &capture.cell {
        let ptr = cg.emit_reg(format!("bitcast i8* {operand} to {}*", cell.pointee));
        cg.bind_cell(
            &capture.name,
            crate::builder::CellSlot {
                ptr,
                ..cell.clone()
            },
            false,
        );
    } else {
        bind_capture(cg, capture, operand);
    }
}

fn bind_capture(cg: &mut Codegen, capture: &Capture, operand: String) {
    let mut value = capture.val.clone();
    value.operand = operand;
    if let Some(ty) = &value.inferred_type {
        cg.bind_fn_local(&capture.name, ty.clone());
    }
    cg.emit_debug_local(&capture.name, &value);
    cg.bind(capture.name.clone(), value);
}
