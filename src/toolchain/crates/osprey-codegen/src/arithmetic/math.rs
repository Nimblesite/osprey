//! Total math and bit-level builtins beyond the unary float functions:
//! the binary float functions, the bit-level views of the three float
//! formats, checked float→int truncation and the bitwise integer operations.
//! All are pure and total — no `Arith` operation, no `Result` except
//! `floatToInt`. Implements [BUILTIN-MATH-TRIG], [BUILTIN-MATH-LOG],
//! [BUILTIN-MATH-ROUNDING], [BUILTIN-FLOAT-BITS], [BUILTIN-FLOATTOINT] and
//! [BUILTIN-BITWISE].

use crate::builder::Codegen;
use crate::conv::{as_i64, as_num, box_to_i64, unbox_from_i64};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::result::make_result_if_err;
use osprey_types::numeric::Numeric;

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
        || name == "floatDiv"
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
    let sized = super::operand_numeric(&left, &right);
    let n = if sized.is_float() {
        sized
    } else {
        Numeric::FLOAT
    };
    if name == "fmod" {
        // Total: fmod(x, 0.0) is NaN, like any domain result.
        return super::float_op(cg, "frem", n, left, right);
    }
    if name == "floatDiv" {
        // [BUILTIN-MATH-FLOATDIV] The bare IEEE quotient: a zero divisor is
        // an infinity or NaN, never `Arith.divideByZero`.
        return super::float_op(cg, "fdiv", n, left, right);
    }
    let symbol = BINARY_INTRINSICS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .or_else(|| BINARY_LIBM.contains(&name).then_some(name))
        .ok_or_else(|| CodegenError::invalid(format!("{name} is not a binary float function")))?;
    // A binary16 pair computes in binary32 and rounds back once
    // [TYPE-NUMERIC-ARITH-WIDTH].
    let compute = n.float_compute();
    let x = as_num(cg, left, compute)?;
    let y = as_num(cg, right, compute)?;
    let sym = super::unary::float_symbol_at(symbol, compute);
    let ty = compute.llvm();
    cg.add_extern(format!("declare {ty} @{sym}({ty}, {ty})"));
    let operand = cg.emit_reg(format!(
        "call {ty} @{sym}({ty} {}, {ty} {})",
        x.operand, y.operand
    ));
    as_num(cg, Value::new(operand, LType::Num(compute)), n)
}

/// [BUILTIN-FLOAT-BITS] The six bit-level float views, paired by name.
pub(crate) fn is_float_bits_builtin(name: &str) -> bool {
    matches!(
        name,
        "floatBits"
            | "floatFromBits"
            | "float32Bits"
            | "float32FromBits"
            | "float16Bits"
            | "float16FromBits"
    )
}

/// [BUILTIN-FLOAT-BITS] One bit-level view by name: total and pure bitcasts
/// that preserve NaN payloads, signed zeros, infinities and subnormals, each
/// pair inverting at its width.
pub(crate) fn float_bits_view(cg: &mut Codegen, name: &str, value: Value) -> Result<Value> {
    match name {
        "floatBits" => float_bits(cg, value),
        "floatFromBits" => float_from_bits(cg, value),
        "float32Bits" => float32_view(cg, value, Numeric::FLOAT32, Numeric::INT32),
        "float32FromBits" => float32_view(cg, value, Numeric::INT32, Numeric::FLOAT32),
        "float16Bits" => float16_view(cg, value, Numeric::FLOAT16, Numeric::INT16),
        "float16FromBits" => float16_view(cg, value, Numeric::INT16, Numeric::FLOAT16),
        _ => Err(CodegenError::invalid(format!(
            "{name} is not a float bit view"
        ))),
    }
}

/// [BUILTIN-FLOAT-BITS] The binary64 word of `x`: a bitcast, preserving NaN
/// payloads, signed zeros and infinities exactly.
fn float_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_num(cg, value, Numeric::FLOAT)?;
    Ok(box_to_i64(cg, x))
}

/// [BUILTIN-FLOAT-BITS] The binary64 the word `bits` holds.
fn float_from_bits(cg: &mut Codegen, value: Value) -> Result<Value> {
    let b = as_i64(cg, value)?;
    Ok(unbox_from_i64(cg, &b.operand, LType::F64))
}

/// [BUILTIN-FLOAT-BITS] One `bitcast` between the binary32 value and its
/// `int32` word.
fn float32_view(cg: &mut Codegen, value: Value, from: Numeric, to: Numeric) -> Result<Value> {
    let x = as_num(cg, value, from)?;
    let word = cg.emit_reg(format!(
        "bitcast {} {} to {}",
        from.llvm(),
        x.operand,
        to.llvm()
    ));
    Ok(Value::new(word, LType::Num(to)))
}

/// [BUILTIN-FLOAT-BITS] A binary16 value already travels as its `i16` bits
/// ([TYPE-NUMERIC-SIZED]), so the view only converts and retags the word it is
/// given.
fn float16_view(cg: &mut Codegen, value: Value, from: Numeric, to: Numeric) -> Result<Value> {
    let x = as_num(cg, value, from)?;
    Ok(Value::new(x.operand, LType::Num(to)))
}

/// [BUILTIN-FLOATTOINT] Checked truncation toward zero from any float width
/// (`as_num` widens `float32` and `float16` exactly first). `fptosi` is poison
/// outside the i64 range, so NaN, ±inf and out-of-range magnitudes are
/// decided by comparisons first and reported as `Error`; the bounds ±2^63
/// are exactly representable doubles, so one comparison each is decisive.
pub(crate) fn float_to_int(cg: &mut Codegen, value: Value) -> Result<Value> {
    let x = as_num(cg, value, Numeric::FLOAT)?;
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

/// [BUILTIN-BITWISE] `and`/`or`/`xor` and the three total shifts at the
/// operands' width. LLVM shifts are poison for a count outside the width, so
/// the count is clamped to 0 and the out-of-range answer selected instead: the
/// limit of repeated shifting — 0 for `shl`/`lshr`, and 0 or -1 for the
/// arithmetic `ashr` by the sign. `shiftRight` on an unsigned type is logical.
pub(crate) fn bitwise(cg: &mut Codegen, name: &str, left: Value, right: Value) -> Result<Value> {
    let n = super::operand_numeric(&left, &right);
    let a = as_num(cg, left, n)?;
    let b = as_num(cg, right, n)?;
    let op = word_instruction(name)?;
    match op {
        "and" | "or" | "xor" => {
            let ty = n.llvm();
            Ok(word(
                cg,
                n,
                format!("{op} {ty} {}, {}", a.operand, b.operand),
            ))
        }
        "ashr" if !n.is_signed() => Ok(total_shift(cg, "lshr", n, &a.operand, &b.operand)),
        _ => Ok(total_shift(cg, op, n, &a.operand, &b.operand)),
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

fn total_shift(cg: &mut Codegen, op: &str, n: Numeric, value: &str, by: &str) -> Value {
    let (ty, bits) = (n.llvm(), n.bits);
    let ok = cg.emit_reg(format!("icmp ult {ty} {by}, {bits}"));
    let count = cg.emit_reg(format!("select i1 {ok}, {ty} {by}, {ty} 0"));
    let shifted = cg.emit_reg(format!("{op} {ty} {value}, {count}"));
    let limit = if op == "ashr" {
        let negative = cg.emit_reg(format!("icmp slt {ty} {value}, 0"));
        cg.emit_reg(format!("select i1 {negative}, {ty} -1, {ty} 0"))
    } else {
        "0".into()
    };
    word(
        cg,
        n,
        format!("select i1 {ok}, {ty} {shifted}, {ty} {limit}"),
    )
}

/// [BUILTIN-BITWISE] One's complement: `xor` with -1.
pub(crate) fn bit_not(cg: &mut Codegen, value: Value) -> Result<Value> {
    let n = super::single_numeric(&value);
    let a = as_num(cg, value, n)?;
    let ty = n.llvm();
    Ok(word(cg, n, format!("xor {ty} {}, -1", a.operand)))
}

fn word(cg: &mut Codegen, n: Numeric, instruction: String) -> Value {
    Value::new(cg.emit_reg(instruction), LType::Num(n))
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

    /// `render` a single call whose result the test only inspects as IR.
    fn render_call(mut build: impl FnMut(&mut Codegen) -> Result<Value>) -> String {
        render(&mut |cg| {
            let _ = build(cg).unwrap();
        })
    }

    /// `bitwise` over the `int` operands `left` and `right`, once per `name`.
    fn bitwise_ir(left: &str, right: &str, names: &[&str]) -> String {
        render(&mut |cg| {
            let (a, b) = (Value::new(left, LType::I64), Value::new(right, LType::I64));
            for name in names {
                let _ = bitwise(cg, name, a.clone(), b.clone()).unwrap();
            }
        })
    }

    #[test]
    fn float_bits_bitcast_preserve_the_word() {
        let ir = render(&mut |cg| {
            let _ = float_bits_view(cg, "floatBits", Value::new("1.0", LType::F64)).unwrap();
            let _ = float_bits_view(cg, "floatFromBits", Value::new("1", LType::I64)).unwrap();
        });
        shows(
            &ir,
            &["bitcast double 1.0 to i64", "bitcast i64 1 to double"],
        );
    }

    #[test]
    fn float32_and_float16_bit_views_retag_their_words() {
        let ir = render(&mut |cg| {
            let _ = float_bits_view(
                cg,
                "float32Bits",
                Value::new("0.5", LType::Num(Numeric::FLOAT32)),
            )
            .unwrap();
            let _ = float_bits_view(cg, "float32FromBits", Value::new("7", LType::I32)).unwrap();
            let _ = float_bits_view(
                cg,
                "float16Bits",
                Value::new("15360", LType::Num(Numeric::FLOAT16)),
            )
            .unwrap();
            let _ =
                float_bits_view(cg, "float16FromBits", Value::new("15360", LType::I32)).unwrap();
        });
        shows(
            &ir,
            &[
                "bitcast float 0.5 to i32",
                "bitcast i32 7 to float",
                "trunc i32 15360 to i16",
            ],
        );
    }

    #[test]
    fn float_to_int_checks_the_domain_before_fptosi() {
        let ir = render_call(|cg| float_to_int(cg, Value::new("-2.9", LType::F64)));
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
        let ir = bitwise_ir("12", "10", &["bitAnd", "bitOr", "bitXor"]);
        shows(&ir, &["and i64 12, 10", "or i64 12, 10", "xor i64 12, 10"]);
    }

    #[test]
    fn shifts_clamp_out_of_range_counts() {
        let ir = bitwise_ir(
            "-16",
            "2",
            &["shiftLeft", "shiftRight", "shiftRightLogical"],
        );
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
        let ir = render_call(|cg| bit_not(cg, Value::new("12", LType::I64)));
        shows(&ir, &["xor i64 12, -1"]);
    }

    #[test]
    fn binary_float_functions_declare_their_symbol() {
        let ir = render(&mut |cg| {
            for (name, left, right) in [
                ("atan2", "1.0", "-1.0"),
                ("pow", "2.0", "10.0"),
                ("fmod", "-7.5", "2.0"),
            ] {
                let _ = binary_float_function(
                    cg,
                    name,
                    Value::new(left, LType::F64),
                    Value::new(right, LType::F64),
                )
                .unwrap();
            }
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
