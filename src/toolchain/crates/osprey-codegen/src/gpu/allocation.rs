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

/// [FAILURE-EXPLICIT] One failure exit for allocation, dispatch and input reads.
/// The reason is taken only after the join, so ARC ownership dominates every
/// exit and no failed branch continues through a host loop or output consumer.
pub(super) struct Operation {
    error: String,
    join: String,
    failed: String,
}

impl Operation {
    pub(super) fn new(cg: &mut Codegen) -> Self {
        crate::extern_call::clear_io_error(cg);
        let failed = cg.hoist_slot("i1");
        cg.emit(format!("store i1 false, i1* {failed}"));
        Self {
            error: cg.fresh_label(),
            join: cg.fresh_label(),
            failed,
        }
    }

    pub(super) fn reject(&self, cg: &mut Codegen, condition: &str) {
        let ready = cg.fresh_label();
        cg.emit(format!(
            "br i1 {condition}, label %{}, label %{ready}",
            self.error
        ));
        cg.start_block(&ready);
    }

    pub(super) fn allocated(&self, cg: &mut Codegen, buffer: &Value) {
        let failed = cg.emit_reg(format!("icmp eq i8* {}, null", buffer.operand));
        self.reject(cg, &failed);
    }

    pub(super) fn status(&self, cg: &mut Codegen, status: &str) {
        let failed = cg.emit_reg(format!("icmp slt i32 {status}, 0"));
        self.reject(cg, &failed);
    }

    pub(super) fn readable(&self, cg: &mut Codegen, source: &Value) {
        let status = cg.call(
            "i32",
            "osprey_gpu_readable",
            "i8*, i64",
            &[&source.operand, "0"],
        );
        self.status(cg, &status);
    }

    pub(super) fn finish(
        self,
        cg: &mut Codegen,
        value: impl FnOnce(&mut Codegen) -> Result<Value>,
    ) -> Result<Value> {
        cg.emit(format!("br label %{}", self.join));
        cg.start_block(&self.error);
        cg.emit(format!("store i1 true, i1* {}", self.failed));
        cg.emit(format!("br label %{}", self.join));
        cg.start_block(&self.join);
        let reason = crate::extern_call::take_io_error(cg);
        let failed = cg.emit_reg(format!("load i1, i1* {}", self.failed));
        let value = value(cg)?;
        let inner = value.ty;
        crate::result::make_result_if_err_because(
            cg,
            value,
            inner,
            &failed,
            Some("GPU operation failed"),
            Some(&reason),
        )
    }
}

/// A checked launch returns 1 for device success and 0 for the host loop.
pub(super) fn host_fallback(cg: &mut Codegen, done: &str) -> String {
    let (host, join) = (cg.fresh_label(), cg.fresh_label());
    let ran = cg.emit_reg(format!("icmp ne i32 {done}, 0"));
    cg.emit(format!("br i1 {ran}, label %{join}, label %{host}"));
    cg.start_block(&host);
    join
}

pub(super) fn rejoin(cg: &mut Codegen, join: &str) {
    cg.emit(format!("br label %{join}"));
    cg.start_block(join);
}
