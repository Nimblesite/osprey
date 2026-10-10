//! Shared cell identity retains the stored value's semantic type.
use super::{Codegen, LType, Type, Value};

/// A mutable variable promoted to a heap cell so an effect handler can own it.
/// `ptr` is a `{pointee}*` operand; reads `load` it, writes `store` to it.
#[derive(Clone)]
pub(crate) struct CellSlot {
    pub ptr: String,
    pub pointee: LType,
    pub osp_ty: Option<String>,
    pub inferred_type: Option<Type>,
}

impl CellSlot {
    pub(crate) fn value(&self, operand: impl Into<String>) -> Value {
        let mut value = Value::new(operand, self.pointee).with_owner(self.osp_ty.clone());
        value.inferred_type.clone_from(&self.inferred_type);
        value
    }
}

impl Codegen {
    pub(crate) fn bind_cell(&mut self, name: &str, cell: CellSlot, declaration: bool) {
        self.emit_debug_cell(name, &cell, declaration);
        let _ = self.cell_slots.insert(name.to_string(), cell);
    }
}
