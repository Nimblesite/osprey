//! Numeric/boolean coercions between LLVM machine types. These bridge the few
//! places where an operand arrives wider/narrower than the instruction wants
//! (a bool used as an int, an int used as a condition, an int promoted to a
//! double for mixed arithmetic).

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_types::numeric::Numeric;

/// Coerce to `i64`.
pub(crate) fn as_i64(cg: &mut Codegen, v: Value) -> Result<Value> {
    as_num(cg, v, Numeric::INT)
}

/// Coerce a number or a bool to the numeric type `to`, by the widening or
/// narrowing its kinds dictate [TYPE-NUMERIC-SIZED]: sign or zero extension and
/// truncation between integers, `sitofp`/`uitofp` into a float, the saturating
/// `fptosi.sat`/`fptoui.sat` out of one ([FLOAT-CONVERT]), `fpext`/`fptrunc`
/// between floats.
pub(crate) fn as_num(cg: &mut Codegen, v: Value, to: Numeric) -> Result<Value> {
    let from = match v.ty {
        LType::Num(n) if n == to => return Ok(v),
        LType::Num(n) => n,
        LType::I1 if to.is_integer() => {
            let reg = cg.emit_reg(format!("zext i1 {} to {}", v.operand, to.llvm()));
            return Ok(Value::new(reg, LType::Num(to)));
        }
        LType::I1 => {
            let i = as_num(cg, v, Numeric::INT)?;
            return as_num(cg, i, to);
        }
        LType::Str | LType::Ptr => {
            return Err(CodegenError::invalid(
                "expected a number, found a string/handle",
            ))
        }
        LType::Any => return Err(read_through_erasure()),
    };
    Ok(cast_num(cg, &v.operand, from, to))
}

/// One numeric value at another numeric type: total, since both are numbers.
/// `float16` goes through the runtime's correctly rounded conversions and
/// every other pair through one LLVM cast [TYPE-NUMERIC-ARITH-WIDTH].
pub(crate) fn cast_num(cg: &mut Codegen, operand: &str, from: Numeric, to: Numeric) -> Value {
    if from == to {
        return Value::new(operand, LType::Num(to));
    }
    if from == Numeric::FLOAT16 {
        cg.add_extern("declare float @osp_f16_to_f32(i16 zeroext)");
        let wide = cg.emit_reg(format!("call float @osp_f16_to_f32(i16 zeroext {operand})"));
        return cast_num(cg, &wide, Numeric::FLOAT32, to);
    }
    if to == Numeric::FLOAT16 {
        let wide = cast_num(cg, operand, from, Numeric::FLOAT);
        cg.add_extern("declare zeroext i16 @osp_float_to_f16(double)");
        let reg = cg.emit_reg(format!(
            "call zeroext i16 @osp_float_to_f16(double {})",
            wide.operand
        ));
        return Value::new(reg, LType::Num(to));
    }
    Value::new(numeric_cast(cg, operand, from, to), LType::Num(to))
}

/// The bits of an `int` word read as the integer type `to`: its wrap, which is
/// what `Arith.overflow` offers a handler as `wrapped` [BUILTIN-CONVERT-INT].
pub(crate) fn reinterpret_int(cg: &mut Codegen, word: &Value, to: Numeric) -> Value {
    let reg = numeric_cast(cg, &word.operand, Numeric::INT, to);
    Value::new(reg, LType::Num(to))
}

fn numeric_cast(cg: &mut Codegen, operand: &str, from: Numeric, to: Numeric) -> String {
    let (f, t) = (from.llvm(), to.llvm());
    let instruction = match (from.is_float(), to.is_float()) {
        (false, false) if to.bits > from.bits && from.is_signed() => "sext",
        (false, false) if to.bits > from.bits => "zext",
        (false, false) if to.bits < from.bits => "trunc",
        (false, false) => return operand.to_string(),
        (false, true) if from.is_signed() => "sitofp",
        (false, true) => "uitofp",
        (true, true) if to.bits > from.bits => "fpext",
        (true, true) => "fptrunc",
        (true, false) => return saturated_int(cg, operand, from, to),
    };
    cg.emit_reg(format!("{instruction} {f} {operand} to {t}"))
}

/// [FLOAT-CONVERT] The checker rejects implicit float narrowing. Protect this
/// internal coercion too: NaN becomes zero, extremes clamp, finite values truncate.
fn saturated_int(cg: &mut Codegen, operand: &str, from: Numeric, to: Numeric) -> String {
    let op = if to.is_signed() { "fptosi" } else { "fptoui" };
    let (f, t) = (from.llvm(), to.llvm());
    let suffix = if from.bits == 32 { "f32" } else { "f64" };
    let name = format!("llvm.{op}.sat.{t}.{suffix}");
    cg.add_extern(format!("declare {t} @{name}({f})"));
    cg.emit_reg(format!("call {t} @{name}({f} {operand})"))
}

fn read_through_erasure() -> CodegenError {
    CodegenError::invalid("cannot read through an erased `any`: match its structure instead")
}

/// Coerce to `i1` (truthiness: non-zero).
pub(crate) fn as_i1(cg: &mut Codegen, v: Value) -> Result<Value> {
    let reg = match v.ty {
        LType::I1 => return Ok(v),
        LType::Num(n) if n.is_integer() => {
            cg.emit_reg(format!("icmp ne {} {}, 0", v.ty, v.operand))
        }
        LType::Num(_) | LType::Str | LType::Ptr => {
            return Err(CodegenError::invalid("expected a bool"))
        }
        LType::Any => return Err(read_through_erasure()),
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
        // A box pointer travels the word ABI like any handle; the erasure
        // survives because the reader restores `Any` from its slot type.
        LType::Str | LType::Ptr | LType::Any => {
            cg.emit_reg(format!("ptrtoint {} {} to i64", v.ty, v.operand))
        }
        LType::I1 => cg.emit_reg(format!("zext i1 {} to i64", v.operand)),
        // [TYPE-NUMERIC-WORD]: a float is its bits, a signed integer sign-
        // extends, an unsigned one zero-extends. A binary16 value already IS
        // its `i16` bits [TYPE-NUMERIC-SIZED].
        LType::F64 => cg.emit_reg(format!("bitcast double {} to i64", v.operand)),
        LType::Num(Numeric::FLOAT16) => cg.emit_reg(format!("zext i16 {} to i64", v.operand)),
        LType::Num(n) if n.is_float() => {
            let bits = cg.emit_reg(format!("bitcast {} {} to i{}", n.llvm(), v.operand, n.bits));
            cg.emit_reg(format!("zext i{} {bits} to i64", n.bits))
        }
        LType::Num(n) => {
            let widen = if n.is_signed() { "sext" } else { "zext" };
            cg.emit_reg(format!("{widen} {} {} to i64", n.llvm(), v.operand))
        }
    };
    Value::new(reg, LType::I64)
}

/// [TYPE-NUMERIC-LITERAL] A literal still spelled at its default type — `int`
/// as `i64`, `float` as `double` — takes the sized type its context resolved,
/// exactly. The one fitting a let annotation, a match-arm join and a
/// container's element slot all route through.
pub(crate) fn fit_literal(
    cg: &mut Codegen,
    value: Value,
    wanted: Option<Numeric>,
) -> Result<Value> {
    let sized = wanted.filter(|n| {
        n.is_sized() && matches!(value.ty, LType::I64 | LType::F64) && LType::Num(*n) != value.ty
    });
    match sized {
        Some(to) => as_num(cg, value, to),
        None => Ok(value),
    }
}

/// Inverse of [`box_to_i64`]: recover a value of `ty` from the uniform `i64`
/// ABI (fiber results). Pointers `inttoptr`, narrow ints `trunc`, `double`
/// `bitcast`; an `i64` element passes through unchanged.
pub(crate) fn unbox_from_i64(cg: &mut Codegen, raw: &str, ty: LType) -> Value {
    let reg = match ty {
        LType::I64 => return Value::new(raw.to_string(), LType::I64),
        LType::Str | LType::Ptr | LType::Any => cg.emit_reg(format!("inttoptr i64 {raw} to i8*")),
        LType::I1 => cg.emit_reg(format!("trunc i64 {raw} to i1")),
        // A binary16 value IS its `i16` bits [TYPE-NUMERIC-SIZED].
        LType::Num(Numeric::FLOAT16) => cg.emit_reg(format!("trunc i64 {raw} to i16")),
        LType::F64 => cg.emit_reg(format!("bitcast i64 {raw} to double")),
        LType::Num(n) if n.is_float() => {
            let bits = cg.emit_reg(format!("trunc i64 {raw} to i{}", n.bits));
            cg.emit_reg(format!("bitcast i{} {bits} to {}", n.bits, n.llvm()))
        }
        LType::Num(n) => cg.emit_reg(format!("trunc i64 {raw} to {}", n.llvm())),
    };
    Value::new(reg, ty)
}

/// A container's uniform `i64` element word recovered at the type its owner tag
/// records ([`crate::llty::elem_of_tag`]): a `float` element's raw bits become a
/// `double` operand — never an integer conversion — a `bool` becomes `i1`, a
/// nested container or record keeps its own owner so it stays usable as one,
/// and an untagged element stays the raw word.
pub(crate) fn from_word(cg: &mut Codegen, raw: impl Into<String>, elem: Option<&str>) -> Value {
    let raw = raw.into();
    let Some(tag) = elem else {
        return Value::new(raw, LType::I64);
    };
    let (ty, owner) = crate::llty::elem_of_spelling(tag);
    unbox_from_i64(cg, &raw, ty).with_owner(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_IR: [&str; 8] = [
        "zext i1 true to i64",
        "sext i32 -7 to i64",
        "call i64 @llvm.fptosi.sat.i64.f64(double 2.5)",
        "icmp ne i32 -1, 0",
        "trunc i64 1 to i1",
        "trunc i64 9 to i32",
        "bitcast i64 123 to double",
        "sitofp i64",
    ];

    fn emitted_conversions(cg: &mut Codegen) {
        let _ = as_i64(cg, Value::new("true", LType::I1));
        let _ = as_i64(cg, Value::new("-7", LType::I32));
        let _ = as_i64(cg, Value::new("2.5", LType::F64));
        let _ = as_i1(cg, Value::new("-1", LType::I32));
        let _ = unbox_from_i64(cg, "1", LType::I1);
        let _ = unbox_from_i64(cg, "9", LType::I32);
        let _ = unbox_from_i64(cg, "123", LType::F64);
        let _ = as_num(cg, Value::new("false", LType::I1), Numeric::FLOAT);
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

    const SATURATION_OPERANDS: &[&str] = &[
        "0x7FF8000000000001",
        "0xFFF8000000000001", // signed NaNs
        "0x7FF0000000000000",
        "0xFFF0000000000000", // infinities
        "0x43E0000000000000",
        "0xC3E0000000000001", // outside i64
        "0x43DFFFFFFFFFFFFF",
        "0xC3E0000000000000", // inside i64
        "0.0",
        "-0.0",
        "2.5",
        "-2.5",
    ];

    /// [FLOAT-CONVERT] Even a future checker bypass must never emit poison.
    #[test]
    fn float_coercions_use_defined_saturation_for_extreme_inputs() {
        let ir = saturation_ir(SATURATION_OPERANDS);
        assert_eq!(
            ir.matches("declare i64 @llvm.fptosi.sat.i64.f64(double)")
                .count(),
            1
        );
        crate::testkit::contents(&ir, &[], &[" = fptosi double"]);
        for operand in SATURATION_OPERANDS {
            assert_saturation_call(&ir, operand);
        }
    }

    fn assert_saturation_call(ir: &str, operand: &str) {
        assert!(
            ir.contains(&format!(
                "call i64 @llvm.fptosi.sat.i64.f64(double {operand})"
            )),
            "unprotected {operand}: {ir}"
        );
    }

    fn saturation_ir(operands: &[&str]) -> String {
        let mut cg = Codegen::new();
        cg.begin_function("saturation", None);
        for operand in operands {
            let value =
                as_i64(&mut cg, Value::new(*operand, LType::F64)).expect("numeric conversion");
            assert_eq!(value.ty, LType::I64);
        }
        cg.emit("ret i64 0");
        cg.finish_function("i64", "saturation", &[]);
        cg.render()
    }

    #[test]
    fn invalid_numeric_coercions_return_precise_diagnostics() {
        let mut cg = Codegen::new();
        let integer = as_i64(&mut cg, Value::new("null", LType::Str));
        let boolean = as_i1(&mut cg, Value::new("0.0", LType::F64));
        let number = as_num(&mut cg, Value::new("null", LType::Ptr), Numeric::FLOAT);
        let widened = as_num(&mut cg, Value::new("0", LType::I32), Numeric::FLOAT)
            .expect("int32 is a number");
        assert_eq!(widened.ty, LType::F64);
        assert_eq!(
            integer.unwrap_err(),
            CodegenError::invalid("expected a number, found a string/handle")
        );
        assert_eq!(
            boolean.unwrap_err(),
            CodegenError::invalid("expected a bool")
        );
        assert_eq!(
            number.unwrap_err(),
            CodegenError::invalid("expected a number, found a string/handle")
        );
    }
}
