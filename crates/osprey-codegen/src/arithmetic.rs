//! Plain arithmetic values with cold value-operation recovery. [ARITH-EFFECT]

use crate::builder::Codegen;
use crate::conv::{as_double, as_i64, as_num};
use crate::error::Result;
use crate::llty::{LType, Value};
use osprey_types::numeric::Numeric;

mod math;
pub(crate) use math::{
    binary_float_function, bit_not, bitwise, float32_bits, float32_from_bits, float32_round,
    float_bits, float_from_bits, float_to_int, is_binary_float_function, is_word_builtin,
};

/// The integer type a two-operand operation runs at: the operand with a sized
/// type. The checker has proven one type for both, so the other operand is that
/// type too, or a literal still carrying its `int` spelling
/// ([TYPE-NUMERIC-LITERAL]) that narrows exactly.
pub(crate) fn operand_numeric(left: &Value, right: &Value) -> Numeric {
    [left, right]
        .into_iter()
        .find_map(|value| match value.ty {
            LType::Num(n) if n.is_sized() => Some(n),
            _ => None,
        })
        .unwrap_or(Numeric::INT)
}

pub(crate) fn binary(
    cg: &mut Codegen,
    op: &str,
    stem: &str,
    left: Value,
    right: Value,
) -> Result<Value> {
    let numeric = operand_numeric(&left, &right);
    let (wrapped, bad) =
        crate::expr::emit_overflow_arith(cg, stem, numeric, left.clone(), right.clone())?;
    let wrapped = Value::new(cg.emit_reg(wrapped), LType::Num(numeric));
    overflow(cg, &bad, op, left, right, wrapped)
}

/// `Arith.overflow` carries `int` words whatever the operand's width: every
/// sized integer fits one exactly ([TYPE-NUMERIC-ARITH-WIDTH]).
fn overflow(
    cg: &mut Codegen,
    bad: &str,
    op: &str,
    left: Value,
    right: Value,
    wrapped: Value,
) -> Result<Value> {
    let spelling = cg.string_constant(op);
    let (fault_left, fault_right) = (as_i64(cg, left)?, as_i64(cg, right)?);
    let fault_wrapped = as_i64(cg, wrapped.clone())?;
    let args = [spelling, fault_left, fault_right, fault_wrapped];
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
    // A handler answers with a word; at a sized type it is taken modulo the
    // width, which is the type's own wrap ([TYPE-NUMERIC-ARITH-WIDTH]).
    let replacement = crate::cast::coerce_to(cg, replacement, value.ty)?;
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
            LType::F64,
        ))
    })
}

/// The zero test and, for a signed type, the minimum-over-minus-one test: the
/// two inputs `sdiv` and `srem` fault on. An unsigned division has only the first.
fn division_guards(cg: &mut Codegen, n: Numeric, left: &Value, right: &Value) -> (String, String) {
    let ty = n.llvm();
    let zero = cg.emit_reg(format!("icmp eq {ty} {}, 0", right.operand));
    if !n.is_signed() {
        return (zero, "false".to_owned());
    }
    let minimum = n.range().map_or(0, |(low, _)| low);
    let least = cg.emit_reg(format!("icmp eq {ty} {}, {minimum}", left.operand));
    let negative_one = cg.emit_reg(format!("icmp eq {ty} {}, -1", right.operand));
    let overflow = cg.emit_reg(format!("and i1 {least}, {negative_one}"));
    (zero, overflow)
}

pub(crate) fn remainder(cg: &mut Codegen, left: Value, right: Value) -> Result<Value> {
    if left.ty == LType::F64 || right.ty == LType::F64 {
        return division(cg, "%", left, right);
    }
    let n = operand_numeric(&left, &right);
    let left = as_num(cg, left, n)?;
    let right = as_num(cg, right, n)?;
    let (zero, overflow) = division_guards(cg, n, &left, &right);
    let ty = n.llvm();
    let divisor = cg.emit_reg(format!(
        "select i1 {overflow}, {ty} 1, {ty} {}",
        right.operand
    ));
    let instruction = if n.is_signed() { "srem" } else { "urem" };
    let fault = as_i64(cg, left.clone())?;
    recover(cg, &zero, "remainderByZero", &[fault], |cg| {
        Ok(Value::new(
            cg.emit_reg(format!("{instruction} {ty} {}, {divisor}", left.operand)),
            LType::Num(n),
        ))
    })
}

pub(crate) fn int_division(cg: &mut Codegen, left: Value, right: Value) -> Result<Value> {
    let n = operand_numeric(&left, &right);
    let left = as_num(cg, left, n)?;
    let right = as_num(cg, right, n)?;
    let (zero, bad) = division_guards(cg, n, &left, &right);
    let (fault_left, fault_right) = (as_i64(cg, left.clone())?, as_i64(cg, right.clone())?);
    let wrapped = n.range().map_or(0, |(low, _)| low).to_string();
    let instruction = if n.is_signed() { "sdiv" } else { "udiv" };
    let ty = n.llvm();
    recover(
        cg,
        &zero,
        "remainderByZero",
        std::slice::from_ref(&fault_left),
        |cg| {
            let spelling = cg.string_constant("intDiv");
            let args = [
                spelling,
                fault_left.clone(),
                fault_right,
                Value::new(wrapped, LType::I64),
            ];
            recover(cg, &bad, "overflow", &args, |cg| {
                Ok(Value::new(
                    cg.emit_reg(format!(
                        "{instruction} {ty} {}, {}",
                        left.operand, right.operand
                    )),
                    LType::Num(n),
                ))
            })
        },
    )
}

/// The integer type of a one-operand operation.
fn single_numeric(value: &Value) -> Numeric {
    operand_numeric(value, value)
}

pub(crate) fn negation(cg: &mut Codegen, value: Value) -> Result<Value> {
    if value.ty == LType::F64 {
        return Ok(Value::new(
            cg.emit_reg(format!("fneg double {}", value.operand)),
            LType::F64,
        ));
    }
    let n = single_numeric(&value);
    let zero = Value::new("0", LType::Num(n));
    let (wrapped, bad) =
        crate::expr::emit_overflow_arith(cg, "sub", n, zero.clone(), value.clone())?;
    let wrapped = Value::new(cg.emit_reg(wrapped), LType::Num(n));
    overflow(cg, &bad, "neg", value, zero, wrapped)
}

pub(crate) fn absolute(cg: &mut Codegen, value: Value) -> Result<Value> {
    if value.ty == LType::F64 {
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
    Ok(Value::new(operand, LType::F64))
}

/// [BUILTIN-ABS] clears the sign of zero and infinity, preserving NaN.
fn float_absolute(cg: &mut Codegen, value: &Value) -> Value {
    cg.add_extern("declare double @llvm.fabs.f64(double)");
    let operand = cg.emit_reg(format!(
        "call double @llvm.fabs.f64(double {})",
        value.operand
    ));
    Value::new(operand, LType::F64)
}

/// Every unsigned value is its own absolute value; a signed type's minimum is
/// the one input that overflows ([TYPE-NUMERIC-ARITH-WIDTH]).
fn integer_absolute(cg: &mut Codegen, value: Value) -> Result<Value> {
    let n = single_numeric(&value);
    if !n.is_signed() {
        return Ok(value);
    }
    let value = as_num(cg, value, n)?;
    let ty = n.llvm();
    let zero = Value::new("0", LType::Num(n));
    let (negated, bad) =
        crate::expr::emit_overflow_arith(cg, "sub", n, zero.clone(), value.clone())?;
    let negated = cg.emit_reg(negated);
    let negative = cg.emit_reg(format!("icmp slt {ty} {}, 0", value.operand));
    let wrapped = cg.emit_reg(format!(
        "select i1 {negative}, {ty} {negated}, {ty} {}",
        value.operand
    ));
    overflow(
        cg,
        &bad,
        "abs",
        value,
        zero,
        Value::new(wrapped, LType::Num(n)),
    )
}

/// Total modular or saturating arithmetic at the operands' width.
/// [ARITH-EFFECT-TOTAL-HELPERS]
pub(crate) fn total(cg: &mut Codegen, name: &str, left: Value, right: Value) -> Result<Value> {
    let op = if name.ends_with("Sub") {
        "sub"
    } else if name.ends_with("Mul") {
        "mul"
    } else {
        "add"
    };
    let n = operand_numeric(&left, &right);
    let left = as_num(cg, left, n)?;
    let right = as_num(cg, right, n)?;
    let ty = n.llvm();
    if name.starts_with("wrap") {
        return Ok(Value::new(
            cg.emit_reg(format!("{op} {ty} {}, {}", left.operand, right.operand)),
            LType::Num(n),
        ));
    }
    let extend = if n.is_signed() { "sext" } else { "zext" };
    let lhs = cg.emit_reg(format!("{extend} {ty} {} to i128", left.operand));
    let rhs = cg.emit_reg(format!("{extend} {ty} {} to i128", right.operand));
    let wide = cg.emit_reg(format!("{op} i128 {lhs}, {rhs}"));
    let (minimum, maximum) = n.range().unwrap_or((0, 0));
    let low = cg.emit_reg(format!("icmp slt i128 {wide}, {minimum}"));
    let high = cg.emit_reg(format!("icmp sgt i128 {wide}, {maximum}"));
    let floor = cg.emit_reg(format!("select i1 {low}, i128 {minimum}, i128 {wide}"));
    let clamped = cg.emit_reg(format!("select i1 {high}, i128 {maximum}, i128 {floor}"));
    Ok(Value::new(
        cg.emit_reg(format!("trunc i128 {clamped} to {ty}")),
        LType::Num(n),
    ))
}

/// The integer type a conversion builtin is named for ([BUILTIN-CONVERT-INT]).
pub(crate) fn integer_conversion_target(name: &str) -> Option<Numeric> {
    Numeric::conversion_target(name).filter(|n| n.is_integer())
}

/// Convert an integer to `to`. Every source fits an `int` word exactly, so the
/// range test runs there; a value outside the target performs `Arith.overflow`
/// with the source as its operand and the target's wrap as `wrapped`.
/// Implements [BUILTIN-CONVERT-INT].
pub(crate) fn integer_conversion(
    cg: &mut Codegen,
    name: &str,
    to: Numeric,
    value: Value,
) -> Result<Value> {
    let from = single_numeric(&value);
    if from == to {
        return as_num(cg, value, to);
    }
    let word = as_i64(cg, value)?;
    let narrowed = crate::conv::reinterpret_int(cg, &word, to);
    let (minimum, maximum) = to.range().unwrap_or((0, 0));
    let low = cg.emit_reg(format!("icmp slt i64 {}, {minimum}", word.operand));
    let high = cg.emit_reg(format!("icmp sgt i64 {}, {maximum}", word.operand));
    let bad = cg.emit_reg(format!("or i1 {low}, {high}"));
    let zero = Value::new("0", LType::I64);
    overflow(cg, &bad, name, word, zero, narrowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unary_float_function_emits_one_double_call() {
        let mut cg = Codegen::new();
        cg.begin_function("math", None);
        for (name, _) in FLOAT_FUNCTIONS {
            let _ = float_function(&mut cg, name, Value::new("1.0", LType::F64)).unwrap();
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
