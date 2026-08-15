//! The resumable-operation mailbox ABI, codegen half.
//!
//! A `perform` that suspends hands its operands to the C runtime as three
//! things: a word array, a parallel array of per-slot ownership kinds, and the
//! operation's real arity. The runtime half is `compiler/runtime/effects_coro.c`
//! and the kind numbering is shared through
//! `compiler/runtime/effects_runtime.h` — the two must change together, so they
//! are kept greppable as one unit. Implements [EFFECTS-OPERATION-MAILBOX].

use crate::builder::Codegen;
use crate::effects::{box_codegen_value, OpSig};
use crate::llty::{LType, Value};
use crate::types::{ltype_of, result_inner};

/// Mailbox operand kinds, mirroring `OSP_OP_ARG_*` in
/// `compiler/runtime/effects_runtime.h`. The runtime releases exactly the
/// MANAGED slots when it retires a mailbox, so a slot tagged here but not
/// retained drops a reference nobody took, and one retained but tagged SCALAR
/// leaks. Implements [EFFECTS-OPERATION-MAILBOX].
const OP_ARG_SCALAR: &str = "0";
const OP_ARG_MANAGED: &str = "1";

/// Whether this slot's word is a managed pointer the mailbox must own. A
/// declared slot is decided by its own LLVM type; an erased (generic) slot
/// travels as a bare `i64` whose real shape only the site's resolved
/// instantiation knows. Implements [EFFECTS-OPERATION-MAILBOX].
fn slot_is_managed(sig: &OpSig, resolved: Option<&osprey_types::OpType>, i: usize) -> bool {
    if sig.param_erased.get(i).copied().unwrap_or(false) {
        return resolved.and_then(|r| r.params.get(i)).is_some_and(|t| {
            result_inner(t).is_some() || matches!(ltype_of(t), LType::Ptr | LType::Str | LType::Any)
        });
    }
    let param = sig.param(i);
    param.result_inner.is_some() || matches!(param.ty, LType::Ptr | LType::Str | LType::Any)
}

fn store_slot(cg: &mut Codegen, arr_ty: &str, arr: &str, i: usize, ty: &str, operand: &str) {
    let slot = cg.emit_reg(format!(
        "getelementptr {arr_ty}, {arr_ty}* {arr}, i64 0, i64 {i}"
    ));
    cg.emit(format!("store {ty} {operand}, {ty}* {slot}"));
}

fn first_slot(cg: &mut Codegen, arr_ty: &str, arr: &str) -> String {
    cg.emit_reg(format!(
        "getelementptr {arr_ty}, {arr_ty}* {arr}, i64 0, i64 0"
    ))
}

/// Retain a word already boxed by the perform site, through its pointer form —
/// the erased path boxes without retaining, so this is where a generic
/// operation's managed operand gains the +1 the mailbox owns.
fn retain_boxed_word(cg: &mut Codegen, word: &str) {
    let ptr = cg.emit_reg(format!("inttoptr i64 {word} to i8*"));
    crate::arc::escape_retain(cg, &Value::new(ptr, LType::Ptr));
}

/// Build the operation's word array and the parallel kind array beside it.
/// Every managed slot leaves here at +1 and the mailbox owns that reference
/// until the dispatcher retires it, so an operand can neither be freed while a
/// handler arm still holds it nor outlive the perform that sent it.
/// Implements [EFFECTS-OPERATION-MAILBOX].
pub(crate) fn emit_mailbox_arrays(
    cg: &mut Codegen,
    sig: &OpSig,
    resolved: Option<&osprey_types::OpType>,
) -> (String, String) {
    let arr_ty = format!("[{} x i64]", sig.params.len());
    let kinds_ty = format!("[{} x i8]", sig.params.len());
    let arr = cg.emit_reg(format!("alloca {arr_ty}"));
    let kinds = cg.emit_reg(format!("alloca {kinds_ty}"));
    for (i, param) in sig.params.iter().copied().enumerate() {
        let managed = slot_is_managed(sig, resolved, i);
        let value = crate::cast::incoming_param(cg, format!("%__arg{i}"), param, None);
        let word = if sig.param_erased.get(i).copied().unwrap_or(false) {
            if managed {
                retain_boxed_word(cg, &value.operand);
            }
            value.operand
        } else {
            box_codegen_value(cg, value).operand
        };
        store_slot(cg, &arr_ty, &arr, i, "i64", &word);
        let kind = if managed {
            OP_ARG_MANAGED
        } else {
            OP_ARG_SCALAR
        };
        store_slot(cg, &kinds_ty, &kinds, i, "i8", kind);
    }
    (
        first_slot(cg, &arr_ty, &arr),
        first_slot(cg, &kinds_ty, &kinds),
    )
}
