//! Plain arithmetic values with cold value-operation recovery. [ARITH-EFFECT]

use crate::builder::Codegen;
use crate::conv::{as_double, as_i64};
use crate::error::Result;
use crate::llty::{LType, Value};

mod math;
pub(crate) use math::{
    binary_float_function, bit_not, bitwise, float32_bits, float32_from_bits, float32_round,
    float_bits, float_from_bits, float_to_int, is_binary_float_function, is_word_builtin,
};

pub(crate) fn binary(
    cg: &mut Codegen,
    op: &str,
    intrinsic: &str,
    left: Value,
    right: Value,
) -> Result<Value> {
    let (wrapped, bad) =
        crate::expr::emit_overflow_arith(cg, intrinsic, left.clone(), right.clone())?;
    let wrapped = Value::new(cg.emit_reg(wrapped), LType::I64);
    overflow(cg, &bad, op, left, right, wrapped)
}

fn overflow(
    cg: &mut Codegen,
    bad: &str,
    op: &str,
    left: Value,
    right: Value,
    wrapped: Value,
) -> Result<Value> {
    let spelling = cg.string_constant(op);
    let args = [spelling, left, right, wrapped.clone()];
    recover(cg, bad, "overflow", &args, |_: &mut Codegen| Ok(wrapped))
}

fn recover(
    cg: &mut Codegen,
    bad: &str,
    operation: &str,
    args: &[Value],
    good: impl FnOnce(&mut Codegen) -> Result<Value>,
) -> Result<Value> {
    let mark = crate::arc::frame_mark(cg);
    let (fault, healthy, join) = cg.diamond(bad);
    crate::arc::open_conditional(cg, mark);
    cg.start_block(&healthy);
    let value = good(cg)?;
    let from_good = cg.snapshot_to(&join);
    cg.start_block(&fault);
    let replacement = crate::effects::gen_arithmetic_request(cg, operation, args)?;
    let from_fault = cg.snapshot_to(&join);
    cg.start_block(&join);
    crate::arc::close_conditional(cg);
    let result = cg.emit_reg(format!(
        "phi {} [ {}, %{from_good} ], [ {}, %{from_fault} ]",
        value.ty.as_str(),
        value.operand,
        replacement.operand
    ));
    Ok(Value::new(result, value.ty))
}

/// Implements [FLOAT-IEEE-RESULTS]: ordered zero guard, then strict IEEE IR.
pub(crate) fn division(cg: &mut Codegen, op: &str, left: Value, right: Value) -> Result<Value> {
    let left = as_double(cg, left)?;
    let right = as_double(cg, right)?;
    let bad = cg.emit_reg(format!("fcmp oeq double {}, 0.0", right.operand));
    let spelling = cg.string_constant(op);
    let instruction = if op == "/" { "fdiv" } else { "frem" };
    recover(cg, &bad, "divideByZero", &[spelling, left.clone()], |cg| {
        Ok(Value::new(
            cg.emit_reg(format!(
                "{instruction} double {}, {}",
                left.operand, right.operand
            )),
            LType::Double,
        ))
    })
}

fn division_guards(cg: &mut Codegen, left: &Value, right: &Value) -> (String, String) {
    let zero = cg.emit_reg(format!("icmp eq i64 {}, 0", right.operand));
    let minimum = cg.emit_reg(format!(
        "icmp eq i64 {}, -9223372036854775808",
        left.operand
    ));
    let negative_one = cg.emit_reg(format!("icmp eq i64 {}, -1", right.operand));
    let overflow = cg.emit_reg(format!("and i1 {minimum}, {negative_one}"));
    (zero, overflow)
}

pub(crate) fn remainder(cg: &mut Codegen, left: Value, right: Value) -> Result<Value> {
    if left.ty == LType::Double || right.ty == LType::Double {
        return division(cg, "%", left, right);
    }
    let left = as_i64(cg, left)?;
    let right = as_i64(cg, right)?;
    let (zero, overflow) = division_guards(cg, &left, &right);
    let divisor = cg.emit_reg(format!(
        "select i1 {overflow}, i64 1, i64 {}",
        right.operand
    ));
    recover(
        cg,
        &zero,
        "remainderByZero",
        std::slice::from_ref(&left),
        |cg| {
            Ok(Value::new(
                cg.emit_reg(format!("srem i64 {}, {divisor}", left.operand)),
                LType::I64,
            ))
        },
    )
}

pub(crate) fn int_division(cg: &mut Codegen, left: &Value, right: &Value) -> Result<Value> {
    let (zero, bad) = division_guards(cg, left, right);
    recover(
        cg,
        &zero,
        "remainderByZero",
        std::slice::from_ref(left),
        |cg| {
            let spelling = cg.string_constant("intDiv");
            let args = [
                spelling,
                left.clone(),
                right.clone(),
                Value::new("-9223372036854775808", LType::I64),
            ];
            recover(cg, &bad, "overflow", &args, |cg| {
                Ok(Value::new(
                    cg.emit_reg(format!("sdiv i64 {}, {}", left.operand, right.operand)),
                    LType::I64,
                ))
            })
        },
    )
}

pub(crate) fn negation(cg: &mut Codegen, value: Value) -> Result<Value> {
    if value.ty == LType::Double {
        return Ok(Value::new(
            cg.emit_reg(format!("fneg double {}", value.operand)),
            LType::Double,
        ));
    }
    let zero = Value::new("0", LType::I64);
    let (wrapped, bad) = crate::expr::emit_overflow_arith(cg, "ssub", zero.clone(), value.clone())?;
    let wrapped = Value::new(cg.emit_reg(wrapped), LType::I64);
    overflow(cg, &bad, "neg", value, zero, wrapped)
}

pub(crate) fn absolute(cg: &mut Codegen, value: Value) -> Result<Value> {
    if value.ty == LType::Double {
        return Ok(float_absolute(cg, &value));
    }
    integer_absolute(cg, value)
}

/// The unary float functions [BUILTIN-MATH] and its sections. `sqrt`, `exp`,
/// `ln`, `sin`, `cos`, `log2`, `log10`, `floor`, `ceil`, `trunc` and `round`
/// are LLVM intrinsics (`ln` is `llvm.log`; the name `log` is left to programs,
/// whose loggers want it; `round` is `llvm.roundeven`, which rounds halfway
/// cases to even — Python's `round` and numpy's — never C's half-away `round`);
/// the rest are the C library's symbols (their intrinsics are younger than
/// the oldest toolchain the targets build with). Each is one `double ->
/// double` call a device emitter maps to its own math library.
const FLOAT_FUNCTIONS: [(&str, &str); 25] = [
    ("sqrt", "llvm.sqrt.f64"),
    ("exp", "llvm.exp.f64"),
    ("ln", "llvm.log.f64"),
    ("tanh", "tanh"),
    ("sin", "llvm.sin.f64"),
    ("cos", "llvm.cos.f64"),
    ("tan", "tan"),
    ("asin", "asin"),
    ("acos", "acos"),
    ("atan", "atan"),
    ("sinh", "sinh"),
    ("cosh", "cosh"),
    ("asinh", "asinh"),
    ("acosh", "acosh"),
    ("atanh", "atanh"),
    ("log2", "llvm.log2.f64"),
    ("log10", "llvm.log10.f64"),
    ("log1p", "log1p"),
    ("expm1", "expm1"),
    ("erf", "erf"),
    ("erfc", "erfc"),
    ("floor", "llvm.floor.f64"),
    ("ceil", "llvm.ceil.f64"),
    ("trunc", "llvm.trunc.f64"),
    ("round", "llvm.roundeven.f64"),
];

pub(crate) fn is_float_function(name: &str) -> bool {
    FLOAT_FUNCTIONS.iter().any(|(n, _)| *n == name)
}

/// Apply the float function `name` to `value`. Inference admits only a `float`
/// argument (`exp(3)` is a type error, never a widening), so `as_double` is a
/// no-op on every program that reaches codegen. Total: domain errors are NaN,
/// overflow is infinity. Implements [BUILTIN-MATH].
pub(crate) fn float_function(cg: &mut Codegen, name: &str, value: Value) -> Result<Value> {
    let Some((_, symbol)) = FLOAT_FUNCTIONS.iter().find(|(n, _)| *n == name) else {
        return Err(crate::error::CodegenError::invalid(format!(
            "{name} is not a float function"
        )));
    };
    let x = as_double(cg, value)?;
    cg.add_extern(format!("declare double @{symbol}(double)"));
    let operand = cg.emit_reg(format!("call double @{symbol}(double {})", x.operand));
    Ok(Value::new(operand, LType::Double))
}

/// [BUILTIN-ABS] clears the sign of zero and infinity, preserving NaN.
fn float_absolute(cg: &mut Codegen, value: &Value) -> Value {
    cg.add_extern("declare double @llvm.fabs.f64(double)");
    let operand = cg.emit_reg(format!(
        "call double @llvm.fabs.f64(double {})",
        value.operand
    ));
    Value::new(operand, LType::Double)
}

fn integer_absolute(cg: &mut Codegen, value: Value) -> Result<Value> {
    let value = as_i64(cg, value)?;
    let zero = Value::new("0", LType::I64);
    let (negated, bad) = crate::expr::emit_overflow_arith(cg, "ssub", zero.clone(), value.clone())?;
    let negated = cg.emit_reg(negated);
    let negative = cg.emit_reg(format!("icmp slt i64 {}, 0", value.operand));
    let wrapped = cg.emit_reg(format!(
        "select i1 {negative}, i64 {negated}, i64 {}",
        value.operand
    ));
    overflow(
        cg,
        &bad,
        "abs",
        value,
        zero,
        Value::new(wrapped, LType::I64),
    )
}

/// Total modular or saturating arithmetic. [ARITH-EFFECT-TOTAL-HELPERS]
pub(crate) fn total(cg: &mut Codegen, name: &str, left: Value, right: Value) -> Result<Value> {
    let op = if name.ends_with("Sub") {
        "sub"
    } else if name.ends_with("Mul") {
        "mul"
    } else {
        "add"
    };
    let left = as_i64(cg, left)?;
    let right = as_i64(cg, right)?;
    if name.starts_with("wrap") {
        return Ok(Value::new(
            cg.emit_reg(format!("{op} i64 {}, {}", left.operand, right.operand)),
            LType::I64,
        ));
    }
    let lhs = cg.emit_reg(format!("sext i64 {} to i128", left.operand));
    let rhs = cg.emit_reg(format!("sext i64 {} to i128", right.operand));
    let wide = cg.emit_reg(format!("{op} i128 {lhs}, {rhs}"));
    let low = cg.emit_reg(format!("icmp slt i128 {wide}, -9223372036854775808"));
    let high = cg.emit_reg(format!("icmp sgt i128 {wide}, 9223372036854775807"));
    let floor = cg.emit_reg(format!(
        "select i1 {low}, i128 -9223372036854775808, i128 {wide}"
    ));
    let clamped = cg.emit_reg(format!(
        "select i1 {high}, i128 9223372036854775807, i128 {floor}"
    ));
    Ok(Value::new(
        cg.emit_reg(format!("trunc i128 {clamped} to i64")),
        LType::I64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unary_float_function_emits_one_double_call() {
        let mut cg = Codegen::new();
        cg.begin_function("math", None);
        for (name, _) in FLOAT_FUNCTIONS {
            let _ = float_function(&mut cg, name, Value::new("1.0", LType::Double)).unwrap();
        }
        cg.emit("ret i64 0");
        cg.finish_function("i64", "math", &[]);
        let ir = cg.render();
        // Each intrinsic or libm symbol is declared and called once per name.
        for (name, symbol) in FLOAT_FUNCTIONS {
            assert!(
                ir.contains(&format!("declare double @{symbol}(double)")),
                "{name}: missing declaration in\n{ir}"
            );
            assert!(
                ir.contains(&format!("call double @{symbol}(double ")),
                "{name}: missing call in\n{ir}"
            );
        }
    }
}
