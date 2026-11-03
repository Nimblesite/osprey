//! Storage width fixups for generic and inline GPU kernels [GPU-BUFFER-ELEM].
//! Lower the kernel once, then complete its allocation instruction at the
//! discovered width; no extra runtime blocks, loops, payloads or copies.
use super::{elem_owner, GPU_OWNER};
use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};

pub(super) fn width(owner: &str) -> String {
    let elem = owner
        .strip_prefix(super::GPU_TAG)
        .map(crate::llty::elem_of_spelling);
    match elem.map(|(ty, _)| ty) {
        Some(LType::Num(n)) => n.bits / 8,
        Some(LType::I1) => 1,
        _ => 8,
    }
    .to_string()
}

pub(super) fn raw(cg: &mut Codegen, len: &str, owner: &str) -> String {
    cg.call("i8*", "osprey_gpu_alloc", "i64, i64", &[len, &width(owner)])
}

#[must_use]
pub(super) struct Pending {
    pub(super) value: Value,
}

impl Pending {
    pub(super) fn new(cg: &mut Codegen, len: &str) -> Self {
        let value = Value::handle(raw(cg, len, GPU_OWNER), GPU_OWNER.to_string());
        crate::arc::own(cg, &value);
        Self { value }
    }

    pub(super) fn finish(self, cg: &mut Codegen, elem: Option<LType>) -> Result<Value> {
        let owner = elem_owner(elem);
        cg.fix_instruction(
            &self.value.operand,
            ", i64 8)",
            &format!(", i64 {})", width(&owner)),
        )?;
        Ok(self.value.with_owner(Some(owner)))
    }
}
