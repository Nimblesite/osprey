//! Normal completion transforms A into B outside its handler activation.
//! Implements [EFFECTS-HANDLER-ARMS] and [EFFECTS-RESUME].

use super::{box_codegen_value, reload_env, AnswerShape, ArmCap};
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::Expr;

pub(super) fn apply(cg: &mut Codegen, clause: Option<&Expr>, value: Value) -> Result<Value> {
    let Some(clause) = clause else {
        return Ok(value);
    };
    let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = clause
    else {
        return Err(CodegenError::invalid(
            "handler return clause must be a unary lambda",
        ));
    };
    if parameters.len() != 1 {
        return Err(CodegenError::invalid(
            "handler return clause requires one parameter",
        ));
    }
    let outer_resume = cg.resume_ctx.take();
    let result =
        crate::expr::apply_lambda_values(cg, parameters, body, vec![value], None, *position);
    cg.resume_ctx = outer_resume;
    result
}

pub(super) fn emit(
    cg: &mut Codegen,
    name: &str,
    clause: &Expr,
    input: &AnswerShape,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<AnswerShape> {
    let saved = cg.enter_nested_fn();
    reload_env(cg, caps, env_ty);
    let value = input.restore(cg, "%__value");
    crate::arc::own(cg, &value);
    let result = apply(cg, Some(clause), value)?;
    let answer = AnswerShape::of(&result);
    let boxed = box_codegen_value(cg, result);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", boxed.operand));
    cg.exit_nested_fn(
        saved,
        "i64",
        name,
        &[(LType::Ptr, "__env".into()), (LType::I64, "__value".into())],
    );
    Ok(answer)
}
