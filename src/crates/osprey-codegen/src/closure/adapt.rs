//! Adapt a function value where equal pointer ABIs hide semantic conversions.
use super::{cell_struct_ty, cell_value, reload_captures, ret_as_sig, spelling_with_env, Capture};
use crate::builder::{Codegen, FnSig, ParamSig};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::Expr;

const SOURCE: &str = "__adapter_source";

pub(crate) fn adapt_to(cg: &mut Codegen, value: Value, want: &ParamSig) -> Result<Value> {
    let Some((source, target)) = signatures(cg, &value, want) else {
        return Ok(value);
    };
    let id = cg.next_lambda_id();
    let name = format!("__function_adapter_{id}");
    let caps = [Capture {
        name: SOURCE.into(),
        val: value,
        cell: None,
    }];
    let cell_ty = cell_struct_ty(&caps);
    define(cg, &name, &cell_ty, &caps, &source, &target)?;
    let mut adapted = cell_value(cg, id, &name, &cell_ty, &caps, &target);
    adapted.inferred_type.clone_from(&want.inferred_type);
    crate::arc::own(cg, &adapted);
    Ok(adapted)
}

fn signatures(cg: &Codegen, value: &Value, want: &ParamSig) -> Option<(FnSig, FnSig)> {
    let source = value.inferred_type.as_ref()?;
    let target = want.inferred_type.as_ref()?;
    if !crate::types::fn_value_concrete(source)
        || !crate::types::fn_value_concrete(target)
        || crate::types::abi_spelling(source) == crate::types::abi_spelling(target)
    {
        return None;
    }
    Some((
        Codegen::fn_value_sig(&cg.prog, source)?,
        Codegen::fn_value_sig(&cg.prog, target)?,
    ))
}

fn define(
    cg: &mut Codegen,
    name: &str,
    cell_ty: &str,
    caps: &[Capture],
    source: &FnSig,
    target: &FnSig,
) -> Result<()> {
    let (ret, _) = spelling_with_env(target);
    let saved = cg.enter_nested_fn();
    reload_captures(cg, cell_ty, caps);
    let (params, args) = parameters(cg, target);
    let emitted = body(cg, args, source, target);
    cg.exit_nested_fn(saved, &ret, name, &params);
    emitted
}

fn body(cg: &mut Codegen, args: Vec<Value>, source: &FnSig, target: &FnSig) -> Result<()> {
    let handle = cg
        .lookup(SOURCE)
        .ok_or_else(|| CodegenError::unknown(SOURCE))?;
    let value = super::cell_call_values(cg, &handle.operand, source, args)?;
    ret_as_sig(cg, value, target)
}

fn parameters(cg: &mut Codegen, target: &FnSig) -> (Vec<(LType, String)>, Vec<Value>) {
    let mut params = vec![(LType::Ptr, "__env".into())];
    let mut args = Vec::new();
    for (index, sig) in target.0.iter().enumerate() {
        let register = crate::llty::param_register(index);
        let value = crate::cast::incoming_param(cg, format!("%{register}"), sig.clone(), None);
        params.push((sig.ty, register));
        args.push(value);
    }
    (params, args)
}

pub(crate) fn argument(
    cg: &mut Codegen,
    argument: &Expr,
    declared: Option<&osprey_types::Type>,
) -> Result<Option<crate::genfn::InlineArg>> {
    let Some(target) = argument_target(cg, argument, declared) else {
        return Ok(None);
    };
    let value = crate::expr::gen_expr(cg, argument)?;
    let want = ParamSig::of(&cg.prog, &target);
    let adapted = crate::cast::coerce_semantic_param(cg, value, &want)?;
    Ok(Some(crate::genfn::InlineArg::Value(
        Box::new(adapted),
        Some(target),
    )))
}

fn argument_target(
    cg: &Codegen,
    argument: &Expr,
    declared: Option<&osprey_types::Type>,
) -> Option<osprey_types::Type> {
    let target = declared.filter(|ty| crate::types::fn_value_concrete(ty))?;
    let source = cg.callee_fn_type(argument)?;
    (crate::types::fn_value_concrete(&source)
        && crate::types::abi_spelling(&source) != crate::types::abi_spelling(target))
    .then(|| target.clone())
}
