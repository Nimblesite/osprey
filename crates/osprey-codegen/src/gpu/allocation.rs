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
        Some(LType::Num(n)) => n.bytes(),
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
    /// `i1` that is 1 when the allocation returned NULL; the literal `0` when
    /// the allocation is infallible.
    pub(super) failed: String,
    /// The reason the allocation recorded (`i8*`; `null` when none).
    pub(super) reason: String,
}

impl Pending {
    pub(super) fn new(cg: &mut Codegen, len: &str) -> Self {
        Self::build(cg, len, false)
    }

    /// A fallible allocation bracketed by the io_error channel: NULL is an
    /// Error carrying the recorded reason ([FAILURE-EXPLICIT]).
    pub(super) fn fallible(cg: &mut Codegen, len: &str) -> Self {
        Self::build(cg, len, true)
    }

    fn build(cg: &mut Codegen, len: &str, fallible: bool) -> Self {
        if fallible {
            crate::extern_call::clear_io_error(cg);
        }
        let allocated = raw(cg, len, GPU_OWNER);
        let reason = if fallible {
            crate::extern_call::take_io_error(cg)
        } else {
            "null".to_string()
        };
        let value = Value::handle(allocated.clone(), GPU_OWNER.to_string());
        crate::arc::own(cg, &value);
        let failed = if fallible {
            cg.emit_reg(format!("icmp eq i8* {allocated}, null"))
        } else {
            "0".to_string()
        };
        Self { value, failed, reason }
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
