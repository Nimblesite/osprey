//! Materialize source closures with their inferred ABI and native debug scope.
use super::captures::closure_captures;
use super::Capture;
use super::{
    bind_params_from, cell_struct_ty, cell_value, closure_return, reload_captures, spelling,
};
use crate::builder::{Codegen, FnSig};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter, Position};

struct Definition<'a> {
    parameters: &'a [Parameter],
    body: &'a Expr,
    sig: &'a FnSig,
    position: Option<Position>,
}

/// Lower a lambda in plain expression position (returned, block tail, stored)
/// using its HM-inferred type as the ABI. A lambda whose recorded type is
/// still generic (inside an inlined generic function, where one source
/// position serves several instantiations) is rejected loudly — a
/// variables-as-`i64` ABI would silently corrupt string/float instantiations.
pub(crate) fn lambda_value(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    position: Option<Position>,
) -> Result<Value> {
    let ty = lambda_type(cg, position)?;
    let sig = Codegen::fn_value_sig(&cg.prog, &ty)
        .ok_or_else(|| CodegenError::invalid("lambda has no inferred function type"))?;
    let mut value = emit_closure(cg, parameters, body, &sig, position)?;
    value.inferred_type = Some(ty);
    Ok(value)
}

fn lambda_type(cg: &Codegen, position: Option<Position>) -> Result<osprey_types::Type> {
    let inferred = cg
        .prog
        .lambda_type(position)
        .ok_or_else(|| CodegenError::invalid("lambda has no inferred function type"))?;
    if crate::types::fn_value_concrete(inferred) {
        Ok(inferred.clone())
    } else {
        specialized_lambda_type(cg, position)
    }
}

fn specialized_lambda_type(cg: &Codegen, position: Option<Position>) -> Result<osprey_types::Type> {
    cg.expected_lambda.as_ref()
        .filter(|(sites, ty)| position.is_some_and(|position| sites.contains(&position))
            && crate::types::fn_value_concrete(ty))
        .map(|(_, ty)| ty.clone())
        .ok_or_else(|| CodegenError::unsupported(
            "a closure value with a still-generic type (wrap it in a function with concrete parameter/return types)",
        ))
}

/// Emit a lambda as a closure value with the given signature (the consuming
/// slot's ABI when known, else the lambda's own inferred type).
pub(crate) fn emit_closure(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
    position: Option<Position>,
) -> Result<Value> {
    emit_closure_keyed(cg, parameters, body, sig, None, position)
}

/// [`emit_closure`] with an optional **emit-once key** naming a (function, ABI)
/// pair whose lowering is identical every time — a generic function specialised
/// into the same slot ABI at several call sites. The first use emits the body
/// and its cell; later ones re-point at that cell, so the module carries one
/// body per instantiation instead of one per call site. Implements
/// [TYPE-GENERICS-FN].
///
/// Only a **capture-free** cell is shareable, and the caller's captures are
/// recomputed here rather than assumed: a capturing cell snapshots the values
/// live at *its* evaluation, so two evaluations are two different closures.
pub(crate) fn emit_closure_keyed(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
    key: Option<String>,
    position: Option<Position>,
) -> Result<Value> {
    let definition = Definition {
        parameters,
        body,
        sig,
        position,
    };
    owned_closure(cg, &definition, key)
}

fn owned_closure(
    cg: &mut Codegen,
    definition: &Definition<'_>,
    key: Option<String>,
) -> Result<Value> {
    let caps = closure_captures(cg, definition.parameters, definition.body);
    let key = key.filter(|_| caps.is_empty());
    let v = match key.as_deref().and_then(|k| cg.fnval_cells.get(k).cloned()) {
        Some(cell) => Value::new(
            cg.emit_reg(format!("bitcast {{ i8* }}* {cell} to i8*")),
            LType::Ptr,
        ),
        None => emit_fresh_closure(cg, &caps, definition, key)?,
    };
    // The cell is a fresh +1 producer here; a spawn's cell instead transfers
    // to the fiber runtime (fiber.rs calls `cell_value` directly).
    crate::arc::own(cg, &v);
    Ok(v)
}

/// Lower a brand-new closure body and cell, registering the cell under `key`
/// when one was supplied so the next same-ABI use can share it.
fn emit_fresh_closure(
    cg: &mut Codegen,
    caps: &[Capture],
    definition: &Definition<'_>,
    key: Option<String>,
) -> Result<Value> {
    let id = cg.next_lambda_id();
    let fn_name = format!("__closure_fn_{id}");
    let cell_ty = cell_struct_ty(caps);
    emit_closure_fn(cg, &fn_name, &cell_ty, caps, definition)?;
    if let Some(k) = key {
        let _ = cg.fnval_cells.insert(k, format!("@__closure_cell_{id}"));
    }
    Ok(cell_value(cg, id, &fn_name, &cell_ty, caps, definition.sig))
}

/// Emit the lifted closure function: `define {ret} @{fn_name}(i8* %__env, …)`,
/// reloading each capture from the cell before lowering the body.
fn emit_closure_fn(
    cg: &mut Codegen,
    fn_name: &str,
    cell_ty: &str,
    caps: &[Capture],
    definition: &Definition<'_>,
) -> Result<()> {
    let (_, ret_ty, ret_inner, _, _) = definition.sig;
    let (ret_spelling, _) = spelling(definition.sig);
    let saved = cg.enter_nested_fn();
    begin_source(cg, fn_name, definition.position);
    let params = prepare_body(cg, cell_ty, caps, definition);
    let emitted = closure_return(cg, definition.body, *ret_ty, *ret_inner);
    cg.exit_nested_fn(saved, &ret_spelling, fn_name, &params);
    emitted
}

fn prepare_body(
    cg: &mut Codegen,
    cell_ty: &str,
    caps: &[Capture],
    definition: &Definition<'_>,
) -> Vec<(LType, String)> {
    // Promotion belongs to this body: a handler must update its shared cell,
    // not a private copy of a local cleared by enter_nested_fn.
    // Implements [EFFECTS-HANDLER-STATE].
    cg.cell_vars = crate::effects::captured_mut_vars(definition.body);
    reload_captures(cg, cell_ty, caps);
    let mut params = vec![(LType::Ptr, String::from("__env"))];
    params.extend(bind_params_from(
        cg,
        definition.parameters,
        &definition.sig.0,
        0,
    ));
    source_parameters(cg, definition.parameters, 1);
    params
}

/// Source lambdas retain their own scope; synthetic adapters have no source.
/// Implements [DEBUGGER-LAMBDA-SCOPES].
pub(super) fn begin_source(cg: &mut Codegen, name: &str, position: Option<Position>) {
    if position.is_some() {
        cg.begin_nested_debug(name, position);
    }
}

/// Account for the hidden environment when numbering source arguments in DWARF.
pub(super) fn source_parameters(cg: &mut Codegen, parameters: &[Parameter], offset: usize) {
    for (index, parameter) in parameters.iter().enumerate() {
        if let Some(value) = cg.lookup(&parameter.name) {
            cg.emit_debug_param(&parameter.name, &value, index + offset);
        }
    }
}
