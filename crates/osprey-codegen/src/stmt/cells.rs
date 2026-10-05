//! Initialization and mutation of shared cells and module storage.
use super::{fn_result_type, Codegen, CodegenError, Expr, LType, Result, Value};
use crate::expr::gen_expr;

/// Declare a handler-captured plain-value `mut` as a heap cell. Result-backed
/// cells require a discriminant-bearing slot and are rejected instead of being
/// silently unwrapped.
pub(super) fn gen_cell_define(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let v = gen_expr(cg, value)?;
    if v.result_inner.is_some() {
        return Err(CodegenError::invalid(
            "mutable Result state must be handled before storage",
        ));
    }
    let fn_ty = fn_result_type(cg, value);
    let pointee = v.ty;
    let ty = pointee.as_str();
    let meta = crate::meta::struct_meta(&[crate::meta::MetaField::of_lty(pointee)]);
    let cell = cg.malloc_struct(&format!("{{ {ty} }}"), meta);
    let ptr = cg.emit_reg(format!(
        "getelementptr {{ {ty} }}, {{ {ty} }}* {cell}, i32 0, i32 0"
    ));
    // The cell holds its own reference to the stored value [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, ty, &v.operand);
    cg.emit(format!("store {ty} {}, {ty}* {ptr}", v.operand));
    // The cell itself is a heap allocation owned by the region that declared
    // the `mut`. A handler env capturing it only DUPs it (`build_env`), so
    // without this the cell outlives every region and leaks — one per captured
    // `mut`. [GC-ARC-PERCEUS].
    let handle = if ty == "i8*" {
        ptr.clone()
    } else {
        cg.emit_reg(format!("bitcast {ty}* {ptr} to i8*"))
    };
    crate::arc::own_beyond_stmt(cg, &Value::new(handle, LType::Ptr));
    cg.forget_binding(name);
    cg.bind_cell(
        name,
        crate::builder::CellSlot {
            ptr,
            pointee,
            osp_ty: v.osp_ty,
            inferred_type: v.inferred_type,
        },
        true,
    );
    if let Some(ty) = fn_ty {
        cg.bind_fn_local(name, ty);
    }
    Ok(())
}

/// Reassign a cell-backed `mut`: the checker requires the exact plain cell type,
/// then codegen coerces only within that representation and stores it.
pub(super) fn gen_cell_store(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let Some(slot) = cg.cell_slots.get(name).cloned() else {
        return Err(CodegenError::unsupported(
            "reassignment of an unpromoted cell",
        ));
    };
    let v = gen_expr(cg, value)?;
    let v = crate::cast::coerce_to(cg, v, slot.pointee)?;
    let ty = slot.pointee.as_str();
    // Rebind order: dup the incoming value BEFORE dropping the old one, so a
    // self-assignment never frees the value it stores [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, ty, &v.operand);
    if slot.pointee.is_managed_ptr() {
        let old = cg.emit_reg(format!("load {ty}, {ty}* {}", slot.ptr));
        crate::arc::release_operand(cg, &old);
    }
    cg.emit(format!("store {ty} {}, {ty}* {}", v.operand, slot.ptr));
    Ok(())
}

/// Reassign a file-scope binding through its module global. The checker allows
/// the write only inside a handler arm, which is exactly where the enclosing
/// frame is out of reach.
pub(super) fn gen_global_store(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let v = gen_expr(cg, value)?;
    crate::globals::assign(cg, name, v)
}
