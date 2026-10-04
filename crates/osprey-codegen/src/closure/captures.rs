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
    free_names(parameters, body)
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
    free_names(parameters, body)
        .into_iter()
        .filter_map(|name| {
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
        })
        .collect()
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
    for (i, c) in caps.iter().enumerate() {
        let slot = i + 1;
        let p = cg.emit_reg(format!(
            "getelementptr {cell_ty}, {cell_ty}* {cell}, i32 0, i32 {slot}"
        ));
        let lty = c.val.llvm_ty();
        let r = cg.emit_reg(format!("load {lty}, {lty}* {p}"));
        if let Some(cell) = &c.cell {
            let ptr = cg.emit_reg(format!("bitcast i8* {r} to {}*", cell.pointee));
            let _ = cg.cell_slots.insert(
                c.name.clone(),
                crate::builder::CellSlot {
                    ptr,
                    ..cell.clone()
                },
            );
            continue;
        }
        let mut v = c.val.clone();
        v.operand = r;
        if let Some(ty) = &v.inferred_type {
            cg.bind_fn_local(&c.name, ty.clone());
        }
        cg.emit_debug_local(&c.name, &v);
        cg.bind(c.name.clone(), v);
    }
}
