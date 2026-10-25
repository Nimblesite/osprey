//! Total math and bit-level builtins beyond the unary float functions:
//! the binary float functions, the binary32 emulation, the bit-level views
//! of both float formats, checked float→int truncation and the bitwise
//! integer operations. All are pure and total — no `Arith` operation, no
//! `Result` except `floatToInt`. Implements [BUILTIN-MATH-TRIG],
//! [BUILTIN-MATH-LOG], [BUILTIN-MATH-ROUNDING], [BUILTIN-FLOAT32],
//! [BUILTIN-FLOAT-BITS], [BUILTIN-FLOATTOINT] and [BUILTIN-BITWISE].

use crate::builder::Codegen;
use crate::conv::{as_double, as_i64, box_to_i64, unbox_from_i64};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::result::make_result_if_err;

/// The binary float functions lowered as one `double x double -> double`
/// call [BUILTIN-MATH-TRIG] [BUILTIN-MATH-LOG] [BUILTIN-MATH-ROUNDING].
/// `pow` and `copysign` are LLVM intrinsics; `atan2` and `hypot` are the C
/// library's symbols, exactly like `tanh`. `fmod` is the `frem` instruction
/// itself — for doubles it *is* C `fmod` — so it needs no declaration.
const BINARY_INTRINSICS: [(&str, &str); 2] =
    [("pow", "llvm.pow.f64"), ("copysign", "llvm.copysign.f64")];
const BINARY_LIBM: [&str; 2] = ["atan2", "hypot"];

pub(crate) fn is_binary_float_function(name: &str) -> bool {
    name == "fmod"
        || BINARY_LIBM.contains(&name)
        || BINARY_INTRINSICS.iter().any(|(n, _)| *n == name)
}

/// The bitwise and shift builtins over `int` words [BUILTIN-BITWISE].
pub(crate) fn is_word_builtin(name: &str) -> bool {
    matches!(
        name,
        "bitAnd" | "bitOr" | "bitXor" | "bitNot" | "shiftLeft" | "shiftRight" | "shiftRightLogical"
    )
}

pub(crate) fn binary_float_function(
    cg: &mut Codegen,
    name: &str,
    left: Value,
    right: Value,
) -> Result<Value> {
    let x = as_double(cg, left)?;
    let y = as_double(cg, right)?;
    if name == "fmod" {
        // Total: fmod(x, 0.0) is NaN, like any domain result.
        return Ok(Value::new(
            cg.emit_reg(format!("frem double {}, {}", x.operand, y.operand)),
            LType::Double,
        ));
    }
    let symbol = BINARY_INTRINSICS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .or_else(|| BINARY_LIBM.contains(&name).then_some(name))
        .ok_or_else(|| CodegenError::invalid(format!("{name} is not a binary float function")))?;
    cg.add_extern(format!("declare double @{symbol}(double, double)"));
    let operand = cg.emit_reg(format!(
        "call double @{symbol}(double {}, double {})",
        x.operand, y.operand
    ));
    Ok(Value::new(operand, LType::Double))
}

/// [BUILTIN-FLOAT32] Round to the nearest binary32 (ties to even) and widen
/// back: one `fptrunc` + one `fpext`, both correctly rounded, so the result
/// is the value a float32 pipeline would hold.
pub(crate) fn float32_round(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_double(cg, value)?;
    let f = cg.emit_reg(format!("fptrunc double {} to float", x.operand));
    Ok(Value::new(
        cg.emit_reg(format!("fpext float {f} to double")),
        LType::Double,
    ))
}

/// [BUILTIN-FLOAT-BITS] The binary64 word of `x`: a bitcast, preserving NaN
/// payloads, signed zeros and infinities exactly.
pub(crate) fn float_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_double(cg, value)?;
    Ok(box_to_i64(cg, x))
}

/// [BUILTIN-FLOAT-BITS] The binary64 the word `bits` holds.
pub(crate) fn float_from_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let b = as_i64(cg, value)?;
    Ok(unbox_from_i64(cg, &b.operand, LType::Double))
}

/// [BUILTIN-FLOAT-BITS] The binary32 pattern of the nearest float32 to `x`
/// (the low 32 bits of the float32 word), zero-extended to an int.
pub(crate) fn float32_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_double(cg, value)?;
    let f = cg.emit_reg(format!("fptrunc double {} to float", x.operand));
    let w = cg.emit_reg(format!("bitcast float {f} to i32"));
    Ok(Value::new(
        cg.emit_reg(format!("zext i32 {w} to i64")),
        LType::I64,
    ))
}

/// [BUILTIN-FLOAT-BITS] The double the binary32 pattern `bits` holds,
/// widened exactly.
pub(crate) fn float32_from_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let b = as_i64(cg, value)?;
    let w = cg.emit_reg(format!("trunc i64 {} to i32", b.operand));
    let f = cg.emit_reg(format!("bitcast i32 {w} to float"));
    Ok(Value::new(
        cg.emit_reg(format!("fpext float {f} to double")),
        LType::Double,
    ))
}

/// [BUILTIN-FLOATTOINT] Checked truncation toward zero. `fptosi` is poison
/// outside the i64 range, so NaN, ±inf and out-of-range magnitudes are
/// decided by comparisons first and reported as `Error`; the bounds ±2^63
/// are exactly representable doubles, so one comparison each is decisive.
pub(crate) fn float_to_int(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_double(cg, value)?;
    let not_nan = cg.emit_reg(format!("fcmp ord double {}, {}", x.operand, x.operand));
    let low = cg.emit_reg(format!(
        "fcmp oge double {}, -9223372036854775808.0",
        x.operand
    ));
    let high = cg.emit_reg(format!(
        "fcmp olt double {}, 9223372036854775808.0",
        x.operand
    ));
    let in_range = cg.emit_reg(format!("and i1 {low}, {high}"));
    let ok = cg.emit_reg(format!("and i1 {not_nan}, {in_range}"));
    let err = cg.emit_reg(format!("xor i1 {ok}, true"));
    let n = cg.emit_reg(format!("fptosi double {} to i64", x.operand));
    make_result_if_err(
        cg,
        Value::new(n, LType::I64),
        LType::I64,
        &err,
        Some("floatToInt: value is NaN, infinite or outside the int range"),
    )
}

/// [BUILTIN-BITWISE] `and`/`or`/`xor` and the three total shifts. LLVM shifts
/// are poison for a count outside 0..63, so the count is clamped to 0 and the
/// out-of-range answer selected instead: the limit of repeated shifting — 0
/// for `shl`/`lshr`, and 0 or -1 for the arithmetic `ashr` by the sign.
pub(crate) fn bitwise(cg: &mut Codegen, name: &str, left: Value, right: Value) -> Result<Value> {
    let a = as_i64(cg, left)?;
    let b = as_i64(cg, right)?;
    let op = word_instruction(name)?;
    match op {
        "and" | "or" | "xor" => Ok(word(cg, format!("{op} i64 {}, {}", a.operand, b.operand))),
        _ => Ok(total_shift(cg, op, &a.operand, &b.operand)),
    }
}

fn word_instruction(name: &str) -> Result<&str> {
    match name {
        "bitAnd" => Ok("and"),
        "bitOr" => Ok("or"),
        "bitXor" => Ok("xor"),
        "shiftLeft" => Ok("shl"),
        "shiftRight" => Ok("ashr"),
        "shiftRightLogical" => Ok("lshr"),
        _ => Err(CodegenError::invalid(format!(
            "{name} is not a bitwise builtin"
        ))),
    }
}

fn total_shift(cg: &mut Codegen, op: &str, value: &str, by: &str) -> Value {
    let ok = cg.emit_reg(format!("icmp ult i64 {by}, 64"));
    let count = cg.emit_reg(format!("select i1 {ok}, i64 {by}, i64 0"));
    let shifted = cg.emit_reg(format!("{op} i64 {value}, {count}"));
    let limit = if op == "ashr" {
        let negative = cg.emit_reg(format!("icmp slt i64 {value}, 0"));
        cg.emit_reg(format!("select i1 {negative}, i64 -1, i64 0"))
    } else {
        "0".into()
    };
    word(cg, format!("select i1 {ok}, i64 {shifted}, i64 {limit}"))
}

/// [BUILTIN-BITWISE] One's complement: `xor` with -1.
pub(crate) fn bit_not(cg: &mut Codegen, value: Value) -> Result<Value> {
    let a = as_i64(cg, value)?;
    Ok(word(cg, format!("xor i64 {}, -1", a.operand)))
}

fn word(cg: &mut Codegen, instruction: String) -> Value {
    Value::new(cg.emit_reg(instruction), LType::I64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shows;

    fn render(build: &mut dyn FnMut(&mut Codegen)) -> String {
        let mut cg = Codegen::new();
        cg.begin_function("math", None);
        build(&mut cg);
        cg.emit("ret i64 0");
        cg.finish_function("i64", "math", &[]);
        cg.render()
    }

    #[test]
    fn float32_round_narrows_and_widens() {
        let ir = render(&mut |cg| {
            let _ = float32_round(cg, Value::new("0.1", LType::Double)).unwrap();
        });
        shows(&ir, &["fptrunc double 0.1 to float", "fpext float "]);
    }

    #[test]
    fn float_bits_bitcast_preserve_the_word() {
        let ir = render(&mut |cg| {
            let _ = float_bits(cg, Value::new("1.0", LType::Double)).unwrap();
            let _ = float_from_bits(cg, Value::new("1", LType::I64)).unwrap();
        });
        shows(
            &ir,
            &["bitcast double 1.0 to i64", "bitcast i64 1 to double"],
        );
    }

    #[test]
    fn float32_bits_use_the_low_32_bits() {
        let ir = render(&mut |cg| {
            let _ = float32_bits(cg, Value::new("0.1", LType::Double)).unwrap();
            let _ = float32_from_bits(cg, Value::new("7", LType::I64)).unwrap();
        });
        shows(
            &ir,
            &[
                "fptrunc double 0.1 to float",
                "bitcast float ",
                "zext i32 ",
                "trunc i64 7 to i32",
            ],
        );
    }

    #[test]
    fn float_to_int_checks_the_domain_before_fptosi() {
        let ir = render(&mut |cg| {
            let _ = float_to_int(cg, Value::new("-2.9", LType::Double)).unwrap();
        });
        shows(
            &ir,
            &[
                "fcmp ord double -2.9, -2.9",
                "fcmp oge double -2.9, -9223372036854775808.0",
                "fcmp olt double -2.9, 9223372036854775808.0",
                "fptosi double -2.9 to i64",
                "floatToInt: value is NaN, infinite or outside the int range",
            ],
        );
    }

    #[test]
    fn bitwise_operators_emit_the_word_instruction() {
        let ir = render(&mut |cg| {
            let a = Value::new("12", LType::I64);
            let b = Value::new("10", LType::I64);
            for name in ["bitAnd", "bitOr", "bitXor"] {
                let _ = bitwise(cg, name, a.clone(), b.clone()).unwrap();
            }
        });
        shows(&ir, &["and i64 12, 10", "or i64 12, 10", "xor i64 12, 10"]);
    }

    #[test]
    fn shifts_clamp_out_of_range_counts() {
        let ir = render(&mut |cg| {
            let a = Value::new("-16", LType::I64);
            let b = Value::new("2", LType::I64);
            for name in ["shiftLeft", "shiftRight", "shiftRightLogical"] {
                let _ = bitwise(cg, name, a.clone(), b.clone()).unwrap();
            }
        });
        shows(
            &ir,
            &[
                "shl i64 -16, ",
                "ashr i64 -16, ",
                "lshr i64 -16, ",
                "icmp ult i64 2, 64",
                "select i1 ",
            ],
        );
    }

    #[test]
    fn bit_not_inverts_with_xor() {
        let ir = render(&mut |cg| {
            let _ = bit_not(cg, Value::new("12", LType::I64)).unwrap();
        });
        assert!(ir.contains("xor i64 12, -1"), "{ir}");
    }

    #[test]
    fn binary_float_functions_declare_their_symbol() {
        let ir = render(&mut |cg| {
            let _ = binary_float_function(
                cg,
                "atan2",
                Value::new("1.0", LType::Double),
                Value::new("-1.0", LType::Double),
            )
            .unwrap();
            let _ = binary_float_function(
                cg,
                "pow",
                Value::new("2.0", LType::Double),
                Value::new("10.0", LType::Double),
            )
            .unwrap();
            let _ = binary_float_function(
                cg,
                "fmod",
                Value::new("-7.5", LType::Double),
                Value::new("2.0", LType::Double),
            )
            .unwrap();
        });
        shows(
            &ir,
            &[
                "call double @atan2(double 1.0, double -1.0)",
                "declare double @atan2(double, double)",
                "call double @llvm.pow.f64(double 2.0, double 10.0)",
                "frem double -7.5, 2.0",
            ],
        );
    }
}
