//! Binding initialization, callable transport and source-variable declarations.
use super::{
    alias_target, factory_lambda_abi, fn_result_type, generic_returned_lambda, lambda_cell,
};
use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter, Position};

pub(super) fn gen_bind(
    cg: &mut Codegen,
    name: &str,
    value: &Expr,
    position: Option<Position>,
    declaration: bool,
) -> Result<()> {
    if bind_lambda(cg, name, value, declaration)? || bind_returned_lambda(cg, name, value)? {
        return Ok(());
    }
    // Generic aliases specialize at each use; they have no single closure ABI.
    if let Some(target) = alias_target(cg, value) {
        cg.forget_binding(name);
        let _ = cg.call_aliases.insert(name.to_string(), target);
        return Ok(());
    }
    let value_bound = typed_initializer(cg, name, value, position, declaration)?;
    // Reassignment must discard stale inline lambdas and aliases.
    cg.forget_binding(name);
    finish_binding(cg, name, value, value_bound, declaration);
    Ok(())
}

/// A concrete lambda has a closure cell as well as its inline application path.
/// A still-generic one is only ever applied inline, so what it closes over is
/// fixed here, where it is defined ([`crate::closure::Environment`]).
fn bind_lambda(cg: &mut Codegen, name: &str, value: &Expr, declaration: bool) -> Result<bool> {
    let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = value
    else {
        return Ok(false);
    };
    let bound = if let Some((ty, sig)) = lambda_cell(cg, *position) {
        let mut bound = crate::closure::emit_closure(cg, parameters, body, &sig, *position)?;
        bound.inferred_type = Some(ty);
        Some(bound)
    } else {
        None
    };
    let definition = (parameters.clone(), (**body).clone(), *position);
    // A file-scope lambda other functions read resolves through module storage.
    let env = (cg.file_lambdas.get(name) != Some(&definition))
        .then(|| crate::closure::capture(cg, crate::closure::free_names(parameters, body)));
    cg.forget_binding(name);
    let _ = cg.lambdas.insert(name.to_string(), definition);
    if let Some(env) = env {
        let _ = cg.lambda_envs.insert(name.to_string(), env);
    }
    if let Some(bound) = bound {
        finish_binding(cg, name, value, bound, declaration);
    }
    Ok(true)
}

/// Generic returned lambdas evaluate their captured prefix exactly once.
/// Existing module slots must be filled through ordinary initialization.
fn bind_returned_lambda(cg: &mut Codegen, name: &str, value: &Expr) -> Result<bool> {
    let Some((callee_params, parameters, body, position)) =
        generic_returned_lambda(cg, value).filter(|_| !cg.module_globals.contains_key(name))
    else {
        return Ok(false);
    };
    let prefix = call_prefix(cg, value)?;
    cg.forget_binding(name);
    let env = store_prefix(cg, name, callee_params, prefix)?;
    let _ = cg
        .lambdas
        .insert(name.to_string(), (parameters, body, position));
    if let Some(env) = env {
        let _ = cg.lambda_envs.insert(name.to_string(), env);
    }
    Ok(true)
}

fn call_prefix(cg: &mut Codegen, value: &Expr) -> Result<Vec<Value>> {
    let Expr::Call {
        arguments,
        named_arguments,
        ..
    } = value
    else {
        return Ok(Vec::new());
    };
    arguments
        .iter()
        .chain(named_arguments.iter().map(|argument| &argument.value))
        .map(|argument| gen_expr(cg, argument))
        .collect()
}

/// Keep the producing call's arguments for the returned lambda to read: in the
/// module slots of a file-scope binding, else as the local binding's environment.
fn store_prefix(
    cg: &mut Codegen,
    name: &str,
    parameters: Vec<Parameter>,
    prefix: Vec<Value>,
) -> Result<Option<crate::closure::Environment>> {
    if let Some((_, slots)) = cg.file_lambda_prefix.get(name).cloned() {
        for (slot, captured) in slots.iter().zip(prefix) {
            crate::globals::publish(cg, slot, captured)?;
        }
        return Ok(None);
    }
    if prefix.len() != parameters.len() {
        return Ok(None);
    }
    let bound = parameters.into_iter().map(|p| p.name).zip(prefix).collect();
    Ok(Some(crate::closure::of_values(cg, bound)))
}

fn typed_initializer(
    cg: &mut Codegen,
    name: &str,
    value: &Expr,
    position: Option<Position>,
    declaration: bool,
) -> Result<Value> {
    let inner = binding_result_inner(cg, name, position, declaration);
    let generated = generate_initializer(cg, value, position)?;
    let fitted = fit_result(cg, generated, inner)?;
    let bound = tag_handle_element(cg, position, fitted);
    infer_binding(cg, position, bound)
}

fn binding_result_inner(
    cg: &Codegen,
    name: &str,
    position: Option<Position>,
    declaration: bool,
) -> Option<LType> {
    cg.prog
        .let_type(position)
        .and_then(crate::types::result_inner)
        .or_else(|| {
            if declaration {
                None
            } else {
                cg.lookup(name).and_then(|bound| bound.result_inner)
            }
        })
}

fn generate_initializer(
    cg: &mut Codegen,
    value: &Expr,
    position: Option<Position>,
) -> Result<Value> {
    let expected = factory_lambda_abi(cg, value, position);
    let prior = std::mem::replace(&mut cg.expected_lambda, expected);
    let generated = gen_expr(cg, value);
    cg.expected_lambda = prior;
    generated
}

/// Erase while the source still carries its concrete generic shape. [TYPE-ANY]
fn infer_binding(cg: &mut Codegen, position: Option<Position>, mut bound: Value) -> Result<Value> {
    let Some(ty) = cg
        .prog
        .let_type(position)
        .filter(|ty| !osprey_types::has_type_var(ty))
        .cloned()
    else {
        return Ok(bound);
    };
    if crate::types::ltype_of(&ty) == LType::Any {
        bound = crate::cast::coerce_to(cg, bound, LType::Any)?;
    }
    bound.inferred_type = Some(ty);
    Ok(bound)
}

fn fit_result(cg: &mut Codegen, value: Value, inner: Option<LType>) -> Result<Value> {
    match inner {
        Some(inner) => crate::result::fit_to_inner(cg, value, inner),
        None => Ok(value),
    }
}

fn finish_binding(cg: &mut Codegen, name: &str, source: &Expr, value: Value, declaration: bool) {
    if let Some(ty) = value
        .inferred_type
        .as_ref()
        .filter(|ty| matches!(ty, osprey_types::Type::Fun { .. }))
        .cloned()
        .or_else(|| fn_result_type(cg, source))
    {
        cg.bind_fn_local(name, ty);
    }
    cg.emit_debug_binding(name, &value, declaration);
    // Move ownership out of the statement region [GC-ARC-PERCEUS].
    crate::arc::bind_owned(cg, name, &value);
    cg.bind(name.to_string(), value);
}

/// Re-tag a bound handle with the element ABI inference resolved for it. Not a
/// handle, or a handle whose element is still polymorphic: unchanged.
fn tag_handle_element(cg: &Codegen, position: Option<Position>, value: Value) -> Value {
    let Some(ty) = cg.prog.let_type(position) else {
        return value;
    };
    let Some(sig) = crate::builder::FiberSig::of(&cg.prog, ty) else {
        return value;
    };
    let owner = match ty {
        osprey_types::Type::Con { args, .. } => crate::types::elem_tag(&cg.prog, args.first()),
        _ => None,
    };
    let mut tagged = sig.restore(value);
    tagged.fiber_elem_owner = owner;
    tagged
}
