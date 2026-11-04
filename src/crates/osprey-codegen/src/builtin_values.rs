//! Intrinsic function values open over a numeric type use the inferred
//! application ABI [BUILTIN-ABS], [TYPE-NUMERIC-ARITH-WIDTH].
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
    if shadowed(cg, name) || !crate::expr::has_value_form(name) {
        return None;
    }
    let signature = osprey_types::builtin_function_type(name)?;
    if !osprey_types::has_type_var(&signature) {
        return None;
    }
    let ty = cg.prog.application_type(position, &signature);
    Some(open_builtin_cell(cg, name, ty))
}

/// A name a local binding shadows, so the intrinsic must not claim it —
/// shared with the GPU kernel path, which also declines behind a fn-ptr local.
pub(crate) fn shadowed(cg: &Codegen, name: &str) -> bool {
    cg.lookup(name).is_some()
        || cg.lambda_def(name).is_some()
        || cg.cell_slots.contains_key(name)
        || cg.fn_ptr_locals.contains_key(name)
        || cg.call_aliases.contains_key(name)
        || cg.module_globals.contains_key(name)
        || cg.fn_params.contains_key(name)
        || cg.extern_params.contains_key(name)
}

/// One closure per builtin and resolved ABI, over the builtin open at that type.
fn open_builtin_cell(cg: &mut Codegen, name: &str, ty: osprey_types::Type) -> Result<Value> {
    let sig = crate::types::fn_value_concrete(&ty)
        .then(|| Codegen::fn_value_sig(&cg.prog, &ty))
        .flatten()
        .ok_or_else(|| {
            CodegenError::invalid(format!(
                "{name} function value needs a resolved numeric type"
            ))
        })?;
    let arity = match &ty {
        osprey_types::Type::Fun { params, .. } => params.len(),
        _ => 0,
    };
    let parameters: Vec<Parameter> = (0..arity)
        .map(|index| Parameter {
            name: format!("value{index}"),
            ty: None,
            inline_constraint: false,
        })
        .collect();
    let body = Expr::Call {
        function: Box::new(Expr::Identifier(name.into())),
        arguments: parameters
            .iter()
            .map(|p| Expr::Identifier(p.name.clone()))
            .collect(),
        named_arguments: Vec::new(),
    };
    let key = crate::closure::specialisation_key(name, &sig);
    let mut value =
        crate::closure::emit_closure_keyed(cg, &parameters, &body, &sig, Some(key), None, None)?;
    value.inferred_type = Some(ty);
    Ok(value)
}
