//! Numeric/boolean coercions between LLVM machine types. These bridge the few
//! places where an operand arrives wider/narrower than the instruction wants
//! (a bool used as an int, an int used as a condition, an int promoted to a
//! double for mixed arithmetic).

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};

/// Coerce to `i64`.
pub(crate) fn as_i64(cg: &mut Codegen, v: Value) -> Result<Value> {
    let reg = match v.ty {
        LType::I64 => return Ok(v),
        LType::I1 => cg.emit_reg(format!("zext i1 {} to i64", v.operand)),
        LType::I32 => cg.emit_reg(format!("sext i32 {} to i64", v.operand)),
        LType::Double => cg.emit_reg(format!("fptosi double {} to i64", v.operand)),
        LType::Str | LType::Ptr => {
            return Err(CodegenError::invalid(
                "expected an integer, found a string/handle",
            ))
        }
    };
    Ok(Value::new(reg, LType::I64))
}

/// Coerce to `i1` (truthiness: non-zero).
pub(crate) fn as_i1(cg: &mut Codegen, v: Value) -> Result<Value> {
    let reg = match v.ty {
        LType::I1 => return Ok(v),
        LType::I64 | LType::I32 => cg.emit_reg(format!("icmp ne {} {}, 0", v.ty, v.operand)),
        LType::Double | LType::Str | LType::Ptr => {
            return Err(CodegenError::invalid("expected a bool"))
        }
    };
    Ok(Value::new(reg, LType::I1))
}

/// Widen any value to the uniform `i64` collection-element ABI: pointers
/// `ptrtoint`, narrow ints `zext`, `double` `bitcast`.
pub(crate) fn box_to_i64(cg: &mut Codegen, v: Value) -> Value {
    // A Result's operand has a precise `{ payload, disc, errmsg }*` LLVM type
    // even though its broad `LType` is Ptr. Normalize that pointer to i8*
    // before the erased machine-word ABI; spelling it directly as `i8*` would
    // generate invalid IR and, more importantly, must never load the payload.
    if v.result_inner.is_some() {
        let ptr = cg.emit_reg(format!("bitcast {} {} to i8*", v.llvm_ty(), v.operand));
        let reg = cg.emit_reg(format!("ptrtoint i8* {ptr} to i64"));
        return Value::new(reg, LType::I64);
    }
    let reg = match v.ty {
        LType::I64 => return v,
        LType::Str | LType::Ptr => cg.emit_reg(format!("ptrtoint {} {} to i64", v.ty, v.operand)),
        LType::I1 | LType::I32 => cg.emit_reg(format!("zext {} {} to i64", v.ty, v.operand)),
        LType::Double => cg.emit_reg(format!("bitcast double {} to i64", v.operand)),
    };
    Value::new(reg, LType::I64)
}

/// Inverse of [`box_to_i64`]: recover a value of `ty` from the uniform `i64`
/// ABI (fiber results). Pointers `inttoptr`, narrow ints `trunc`, `double`
/// `bitcast`; an `i64` element passes through unchanged.
pub(crate) fn unbox_from_i64(cg: &mut Codegen, raw: &str, ty: LType) -> Value {
    let reg = match ty {
        LType::I64 => return Value::new(raw.to_string(), LType::I64),
        LType::Str | LType::Ptr => cg.emit_reg(format!("inttoptr i64 {raw} to i8*")),
        LType::I1 => cg.emit_reg(format!("trunc i64 {raw} to i1")),
        LType::I32 => cg.emit_reg(format!("trunc i64 {raw} to i32")),
        LType::Double => cg.emit_reg(format!("bitcast i64 {raw} to double")),
    };
    Value::new(reg, ty)
}

/// Coerce to `double` (promoting an integer operand for mixed arithmetic).
pub(crate) fn as_double(cg: &mut Codegen, v: Value) -> Result<Value> {
    match v.ty {
        LType::Double => Ok(v),
        LType::I64 => Ok(Value::new(
            cg.emit_reg(format!("sitofp i64 {} to double", v.operand)),
            LType::Double,
        )),
        LType::I1 => {
            let i = as_i64(cg, v)?;
            as_double(cg, i)
        }
        LType::I32 | LType::Str | LType::Ptr => Err(CodegenError::invalid("expected a number")),
    }
}
