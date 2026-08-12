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

/// A container's uniform `i64` element word recovered at the type its owner tag
/// records ([`crate::llty::elem_of_tag`]): a `float` element's raw bits become a
/// `double` operand — never an integer conversion — a `bool` becomes `i1`, and
/// an untagged element stays the raw word.
pub(crate) fn from_word(cg: &mut Codegen, raw: impl Into<String>, elem: Option<LType>) -> Value {
    let raw = raw.into();
    match elem {
        Some(ty) => unbox_from_i64(cg, &raw, ty),
        None => Value::new(raw, LType::I64),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_IR: [&str; 8] = [
        "zext i1 true to i64",
        "sext i32 -7 to i64",
        "fptosi double 2.5 to i64",
        "icmp ne i32 -1, 0",
        "trunc i64 1 to i1",
        "trunc i64 9 to i32",
        "bitcast i64 123 to double",
        "sitofp i64",
    ];

    fn emitted_conversions(cg: &mut Codegen) {
        let _ = as_i64(cg, Value::new("true", LType::I1));
        let _ = as_i64(cg, Value::new("-7", LType::I32));
        let _ = as_i64(cg, Value::new("2.5", LType::Double));
        let _ = as_i1(cg, Value::new("-1", LType::I32));
        let _ = unbox_from_i64(cg, "1", LType::I1);
        let _ = unbox_from_i64(cg, "9", LType::I32);
        let _ = unbox_from_i64(cg, "123", LType::Double);
        let _ = as_double(cg, Value::new("false", LType::I1));
    }

    #[test]
    fn numeric_coercions_emit_width_and_representation_preserving_ir() {
        let mut cg = Codegen::new();
        cg.begin_function("coercions", None);
        emitted_conversions(&mut cg);
        cg.emit("ret i64 0");
        cg.finish_function("i64", "coercions", &[]);
        let ir = cg.render();
        for instruction in EXPECTED_IR {
            assert!(ir.contains(instruction), "missing `{instruction}`:\n{ir}");
        }
    }

    #[test]
    fn invalid_numeric_coercions_return_precise_diagnostics() {
        let mut cg = Codegen::new();
        let integer = as_i64(&mut cg, Value::new("null", LType::Str));
        let boolean = as_i1(&mut cg, Value::new("0.0", LType::Double));
        let number = as_double(&mut cg, Value::new("0", LType::I32));
        assert_eq!(
            integer.unwrap_err(),
            CodegenError::invalid("expected an integer, found a string/handle")
        );
        assert_eq!(
            boolean.unwrap_err(),
            CodegenError::invalid("expected a bool")
        );
        assert_eq!(
            number.unwrap_err(),
            CodegenError::invalid("expected a number")
        );
    }
}
