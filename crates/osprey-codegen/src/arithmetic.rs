//! Plain arithmetic values with cold value-operation recovery. [ARITH-EFFECT]

use crate::builder::Codegen;
use crate::conv::{as_i64, as_num};
use crate::error::Result;
use crate::llty::{LType, Value};
use osprey_types::numeric::Numeric;

mod math;
mod unary;
pub(crate) use math::{
    binary_float_function, bit_not, bitwise, float_bits_view, float_to_int,
    is_binary_float_function, is_float_bits_builtin, is_word_builtin,
};
pub(crate) use unary::{float_function, is_float_function};

/// The numeric type a two-operand operation runs at: a sized operand's own
/// type first, else `float` when either operand is a float, else `int`. The
/// checker has proven one type for both, so the other operand is that type
/// too, or a literal still carrying its default spelling
/// ([TYPE-NUMERIC-LITERAL]) that narrows exactly.
pub(crate) fn operand_numeric(left: &Value, right: &Value) -> Numeric {
    let numeric = |v: &Value| {
        if let LType::Num(n) = v.ty {
            Some(n)
        } else {
            None
        }
    };
    let sides = [numeric(left), numeric(right)];
    sides
        .into_iter()
        .flatten()
        .find(|n| n.is_sized())
        .or_else(|| sides.into_iter().flatten().find(|n| n.is_float()))
        .unwrap_or(Numeric::INT)
}

/// Widen a float operand pair to the width their operation computes at:
/// binary16 computes in binary32, every other width is itself
/// ([TYPE-NUMERIC-ARITH-WIDTH]).
pub(crate) fn float_operands(
    cg: &mut Codegen,
    n: Numeric,
    left: Value,
    right: Value,
) -> Result<(Numeric, Value, Value)> {
    let compute = n.float_compute();
    Ok((
        compute,
        as_num(cg, left, compute)?,
        as_num(cg, right, compute)?,
    ))
}

/// [TYPE-NUMERIC-ARITH-WIDTH] One two-operand float arithmetic instruction at
/// `n` — `fadd`/`fsub`/`fmul`/`fdiv`/`frem`. `float16` computes in binary32
/// and rounds the result back once, which for these is the correctly rounded
/// binary16 answer; every other width is its native `inst {n.llvm()}`. A
/// literal operand arrives as its `double` default spelling and narrows
/// exactly through the same `as_num`.
pub(crate) fn float_op(
    cg: &mut Codegen,
    inst: &str,
    n: Numeric,
    left: Value,
    right: Value,
) -> Result<Value> {
    let (compute, l, r) = float_operands(cg, n, left, right)?;
    let reg = cg.emit_reg(format!(
        "{inst} {} {}, {}",
        compute.llvm(),
        l.operand,
        r.operand
    ));
    as_num(cg, Value::new(reg, LType::Num(compute)), n)
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
/// The guard and the operation run at the operands' compute type — `float32`
/// natively, `float16` in binary32 — and an integer `/` computes in `double`,
/// as it always has.
pub(crate) fn division(cg: &mut Codegen, op: &str, left: Value, right: Value) -> Result<Value> {
    let sized = operand_numeric(&left, &right);
    let n = if sized.is_float() {
        sized
    } else {
        Numeric::FLOAT
    };
    // The zero guard reads the divisor at the compute type: a binary16 zero
    // widens to binary32 exactly, so the guard's zero is its own.
    let compute = n.float_compute();
    let r = as_num(cg, right, compute)?;
    let bad = cg.emit_reg(format!("fcmp oeq {} {}, 0.0", compute.llvm(), r.operand));
    let l = as_num(cg, left, compute)?;
    // The Arith operation's signature is `(string, float) -> float`, so the
    // fault's operand travels as a double whatever width the divide runs at.
    let fault = as_num(cg, l.clone(), Numeric::FLOAT)?;
    let spelling = cg.string_constant(op);
    let inst = if op == "/" { "fdiv" } else { "frem" };
    recover(cg, &bad, "divideByZero", &[spelling, fault], |cg| {
        float_op(cg, inst, n, l, r)
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
    if operand_numeric(&left, &right).is_float() {
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
    let n = single_numeric(&value);
    if n.is_float() {
        let x = as_num(cg, value, n)?;
        // A binary16 value is its `i16` bits, so the sign bit flips directly;
        // no NaN canonicalisation at any width [TYPE-NUMERIC-ARITH-WIDTH].
        let reg = if n == Numeric::FLOAT16 {
            cg.emit_reg(format!("xor i16 {}, -32768", x.operand))
        } else {
            cg.emit_reg(format!("fneg {} {}", n.llvm(), x.operand))
        };
        return Ok(Value::new(reg, LType::Num(n)));
    }
    let zero = Value::new("0", LType::Num(n));
    let (wrapped, bad) =
        crate::expr::emit_overflow_arith(cg, "sub", n, zero.clone(), value.clone())?;
    let wrapped = Value::new(cg.emit_reg(wrapped), LType::Num(n));
    overflow(cg, &bad, "neg", value, zero, wrapped)
}

pub(crate) fn absolute(cg: &mut Codegen, value: Value) -> Result<Value> {
    if single_numeric(&value).is_float() {
        return Ok(float_absolute(cg, &value));
    }
    integer_absolute(cg, value)
}

/// [BUILTIN-ABS] clears the sign of zero and infinity, preserving NaN. A
/// binary16 value is its `i16` bits, so the mask clears the sign bit directly.
fn float_absolute(cg: &mut Codegen, value: &Value) -> Value {
    let n = single_numeric(value);
    if n == Numeric::FLOAT16 {
        let cleared = cg.emit_reg(format!("and i16 {}, 32767", value.operand));
        return Value::new(cleared, LType::Num(n));
    }
    let (ty, bits) = (n.llvm(), n.bits);
    cg.add_extern(format!("declare {ty} @llvm.fabs.f{bits}({ty})"));
    let operand = cg.emit_reg(format!(
        "call {ty} @llvm.fabs.f{bits}({ty} {})",
        value.operand
    ));
    Value::new(operand, LType::Num(n))
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
