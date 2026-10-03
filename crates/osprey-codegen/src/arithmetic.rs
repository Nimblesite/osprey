//! Plain arithmetic values with cold value-operation recovery. [ARITH-EFFECT]

use crate::builder::Codegen;
use crate::conv::{as_double, as_i64};
use crate::error::Result;
use crate::llty::{LType, Value};

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
