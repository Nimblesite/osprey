//! Intrinsic function values use the inferred application ABI [BUILTIN-ABS].
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::Value;
use osprey_ast::{Expr, Parameter, Position};

pub(crate) fn intrinsic_value(
    cg: &mut Codegen,
    function: &Expr,
    position: Option<Position>,
) -> Option<Result<Value>> {
    let Expr::Identifier(name) = crate::expr::unapplied(function) else {
        return None;
    };
    if name != "abs" || shadowed(cg, name) {
        return None;
    }
    let signature = osprey_types::builtin_function_type(name)?;
    let ty = cg.prog.application_type(position, &signature);
    Some(absolute_cell(cg, ty))
}

fn shadowed(cg: &Codegen, name: &str) -> bool {
    cg.lookup(name).is_some()
        || cg.cell_slots.contains_key(name)
        || cg.module_globals.contains_key(name)
        || cg.fn_params.contains_key(name)
        || cg.call_aliases.contains_key(name)
        || cg.lambda_def(name).is_some()
}

fn absolute_cell(cg: &mut Codegen, ty: osprey_types::Type) -> Result<Value> {
    let sig = crate::types::fn_value_concrete(&ty)
        .then(|| Codegen::fn_value_sig(&cg.prog, &ty))
        .flatten()
        .ok_or_else(|| CodegenError::invalid("abs function value needs a resolved numeric type"))?;
    let parameter = Parameter {
        name: "value".into(),
        ty: None,
        inline_constraint: false,
    };
    let key = crate::closure::specialisation_key("abs", &sig);
    let mut value = crate::closure::emit_closure_keyed(
        cg,
        &[parameter],
        &absolute_body(),
        &sig,
        Some(key),
        None,
    )?;
    value.inferred_type = Some(ty);
    Ok(value)
}

fn absolute_body() -> Expr {
    Expr::Call {
        function: Box::new(Expr::Identifier("abs".into())),
        arguments: vec![Expr::Identifier("value".into())],
        named_arguments: Vec::new(),
    }
}
