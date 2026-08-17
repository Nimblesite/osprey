//! Value coercion to a wanted LLVM type (numeric promotion at call/return/store
//! boundaries). String/handle targets take the operand as-is, only re-tagging
//! the LLVM type.

use crate::builder::{Codegen, ParamSig};
use crate::conv::{as_double, as_i1, as_i64};
use crate::error::Result;
use crate::llty::{LType, Value};

/// Coerce a plain value to the wanted type, preserving its aggregate owner tag.
/// Result is never a coercion source: callers must preserve it or handle it —
/// there is no implicit `Result<T, E>` → `T`. Implements [FAILURE-EXPLICIT].
pub(crate) fn coerce_to(cg: &mut Codegen, v: Value, want: LType) -> Result<Value> {
    // An `any` destination is exempt from the Result guard: a `Result` erases
    // WHOLE — discriminant intact, rendered as `Success(…)`/`Error(…)` — which
    // is preservation, not unwrapping ([`crate::anybox`], [TYPE-ANY]).
    if v.result_inner.is_some() && want != LType::Any {
        return Err(crate::error::CodegenError::invalid(
            "cannot coerce an unhandled Result to a plain value",
        ));
    }
    if v.ty == want && v.result_inner.is_none() {
        return Ok(v);
    }
    let owner = v.osp_ty.clone();
    let out = match want {
        // The erasure boundary: box the value with its shape descriptor. The
        // reverse direction never appears here — the checker rejects every
        // recovery.
        LType::Any => return crate::anybox::box_any(cg, v),
        LType::Double => as_double(cg, v)?,
        // A string/handle/box reaching an `i64` boundary is a generic value
        // travelling in the uniform machine-word representation — `ptrtoint`
        // it (the inverse `inttoptr` is the `Str`/`Ptr` arm below). Genuine
        // type mismatches are already rejected by the checker.
        LType::I64 if matches!(v.ty, LType::Str | LType::Ptr | LType::Any) => {
            crate::conv::box_to_i64(cg, v)
        }
        LType::I64 => as_i64(cg, v)?,
        LType::I1 => as_i1(cg, v)?,
        // A pointer target. A boxed `i64` element (the uniform collection ABA)
        // must be `inttoptr`-converted back to a handle; an existing pointer
        // just retags (both are `i8*`). An erased box is NOT retagged — reading
        // it as a string/handle is the unchecked recovery [TYPE-ANY] deletes.
        LType::Str | LType::Ptr | LType::I32 => {
            if v.ty == LType::Any {
                return Err(crate::error::CodegenError::invalid(
                    "cannot read through an erased `any`: match its structure instead",
                ));
            }
            if v.ty == LType::I64 && matches!(want, LType::Str | LType::Ptr) {
                let reg = cg.fresh_reg();
                cg.emit(format!("{reg} = inttoptr i64 {} to i8*", v.operand));
                Value::new(reg, want)
            } else {
                Value::new(v.operand, want)
            }
        }
    };
    Ok(out.with_owner(owner))
}

/// Adapt an argument to a function parameter ABI slot. Result parameters use
/// an opaque pointer at the call boundary while retaining their complete block;
/// a plain value is safely promoted to Success, never the reverse.
pub(crate) fn coerce_param(cg: &mut Codegen, v: Value, want: ParamSig) -> Result<Value> {
    let semantic = coerce_semantic_param(cg, v, want)?;
    let Some(_) = want.result_inner else {
        return Ok(semantic);
    };
    let ptr = cg.emit_reg(format!(
        "bitcast {} {} to i8*",
        semantic.llvm_ty(),
        semantic.operand
    ));
    Ok(Value::new(ptr, LType::Ptr))
}

/// Adapt an inline argument to the semantic parameter shape while keeping a
/// Result as a typed block (there is no emitted ABI boundary to erase here).
pub(crate) fn coerce_semantic_param(cg: &mut Codegen, v: Value, want: ParamSig) -> Result<Value> {
    let value = match want.result_inner {
        Some(inner) => crate::result::fit_to_inner(cg, v, inner)?,
        None => coerce_to(cg, v, want.ty)?,
    };
    Ok(match want.fiber {
        Some(fiber) => fiber.restore(value),
        None => value,
    })
}

/// Reconstruct an incoming parameter value from its emitted ABI register.
pub(crate) fn incoming_param(
    cg: &mut Codegen,
    operand: String,
    sig: ParamSig,
    owner: Option<String>,
) -> Value {
    let value = if let Some(inner) = sig.result_inner {
        let struct_ty = crate::llty::result_struct_ty(inner);
        let typed = cg.emit_reg(format!("bitcast i8* {operand} to {struct_ty}*"));
        Value::result(typed, inner)
    } else {
        Value::new(operand, sig.ty).with_owner(owner)
    };
    match sig.fiber {
        Some(fiber) => fiber.restore(value),
        None => value,
    }
}
