//! Expression lambdas lowering.
use super::{gen_expr, Codegen, CodegenError, Expr, FnSig, Parameter, Position, Result, Value};

/// Beta-reduce a lambda at its application site: bind each parameter to its
/// argument and lower the body in a fresh scope. The returned value keeps its
/// complete inferred representation, including a Result wrapper.
/// Apply a let-bound lambda that was recorded for inlining rather than
/// materialized as a cell.
///
/// A lambda a GENERIC function returned also closes over that call's
/// parameters, evaluated once at the binding
/// ([`crate::closure::Environment`], [`crate::stmt`]). Captures are bindings,
/// not lambda parameters: prepending them shifts callback slots away from the
/// lambda's inferred signature and emits calls to nonexistent function names.
pub(super) fn apply_bound_lambda(
    cg: &mut Codegen,
    name: &str,
    params: &[Parameter],
    body: &Expr,
    position: Option<Position>,
    arguments: &[&Expr],
) -> Result<Value> {
    let mut values = Vec::with_capacity(arguments.len());
    for a in arguments {
        values.push(gen_expr(cg, a)?);
    }
    with_lambda_captures(cg, name, |cg| {
        let sig = inline_sig(cg, position);
        apply_lambda_values(cg, params, body, values, sig.as_ref(), position)
    })
}

/// Bind the file-scope names of an inlined lambda from their lexical scope.
/// Its caller may have locals with the same names.
fn file_lambda_globals(cg: &Codegen, name: &str) -> Vec<String> {
    let Some((parameters, body, _)) = cg.file_lambdas.get(name) else {
        return Vec::new();
    };
    let mut names = std::collections::BTreeSet::new();
    osprey_ast::freevars::free_idents(body, &mut names);
    for parameter in parameters {
        let _ = names.remove(&parameter.name);
    }
    names
        .into_iter()
        .filter(|name| cg.module_globals.contains_key(name))
        .collect()
}

fn is_file_lambda_binding(cg: &Codegen, name: &str) -> bool {
    cg.file_lambdas
        .get(name)
        .is_some_and(|file| cg.lambdas.get(name).is_none_or(|local| local == file))
}

/// Lower `emit` in the lexical scope of the inline lambda `name`: module
/// storage for a file-scope binding, else the environment fixed where the
/// lambda was defined ([`crate::closure::within`]).
pub(super) fn with_lambda_captures<T>(
    cg: &mut Codegen,
    name: &str,
    emit: impl FnOnce(&mut Codegen) -> Result<T>,
) -> Result<T> {
    if !is_file_lambda_binding(cg, name) {
        return crate::closure::within(cg, name, emit);
    }
    let globals = file_lambda_globals(cg, name);
    let prefix = file_prefix_values(cg, name)?;
    cg.with_file_scope(|cg| bind_and_emit(cg, globals, prefix, emit))
}

fn bind_and_emit<T>(
    cg: &mut Codegen,
    globals: Vec<String>,
    prefix: Option<(Vec<Parameter>, Vec<Value>)>,
    emit: impl FnOnce(&mut Codegen) -> Result<T>,
) -> Result<T> {
    if globals.is_empty() && prefix.is_none() {
        return emit(cg);
    }
    cg.push_scope();
    let saved_fn_ptrs = cg.fn_ptr_locals.clone();
    for global in globals {
        if let Some(value) = crate::globals::read(cg, &global) {
            cg.bind(global, value);
        }
    }
    bind_lambda_prefix(cg, prefix);
    let result = emit(cg);
    cg.fn_ptr_locals = saved_fn_ptrs;
    cg.pop_scope();
    result
}

fn file_prefix_values(
    cg: &mut Codegen,
    name: &str,
) -> Result<Option<(Vec<Parameter>, Vec<Value>)>> {
    let Some((parameters, slots)) = cg.file_lambda_prefix.get(name).cloned() else {
        return Ok(None);
    };
    let values = slots
        .iter()
        .map(|slot| crate::globals::read(cg, slot).ok_or_else(|| CodegenError::unknown(slot)))
        .collect::<Result<Vec<_>>>()?;
    Ok(Some((parameters, values)))
}

fn bind_lambda_prefix(cg: &mut Codegen, prefix: Option<(Vec<Parameter>, Vec<Value>)>) {
    let Some((parameters, values)) = prefix else {
        return;
    };
    for (parameter, value) in parameters.iter().zip(values) {
        if let Some(ty @ osprey_types::Type::Fun { .. }) = &value.inferred_type {
            cg.bind_fn_local(&parameter.name, ty.clone());
        }
        cg.bind(parameter.name.clone(), value);
    }
}

pub(super) fn apply_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    position: Option<Position>,
    arguments: &[&Expr],
) -> Result<Value> {
    let mut values = Vec::with_capacity(arguments.len());
    for a in arguments {
        values.push(gen_expr(cg, a)?);
    }
    apply_lambda_values(
        cg,
        parameters,
        body,
        values,
        inline_sig(cg, position).as_ref(),
        position,
    )
}

/// The signature to fit an INLINED lambda application to, or `None` when the
/// lambda is still generic.
///
/// Inference records ONE type per lambda position, so fitting a generic lambda
/// to it used whichever instantiation happened to be recorded: `let idl = |x|
/// => x` applied to an `int` and then to a `string` coerced the string into the
/// int slot, and the second call printed a pointer as a number. A generic
/// lambda is specialised by its ARGUMENTS at each call site instead, exactly as
/// a generic function is ([`crate::genfn`], [TYPE-GENERICS-FN]).
pub(crate) fn inline_sig(cg: &Codegen, position: Option<Position>) -> Option<FnSig> {
    cg.prog
        .lambda_type(position)
        .filter(|t| crate::types::fn_value_concrete(t))
        .and_then(|t| Codegen::fn_value_sig(&cg.prog, t))
}

/// [`apply_lambda`] over already-evaluated argument values — shared with the
/// iterator builtins, which produce loop elements as values.
pub(crate) fn apply_lambda_values(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
    sig: Option<&FnSig>,
    position: Option<Position>,
) -> Result<Value> {
    reduce_lambda(
        cg,
        parameters,
        body,
        values,
        sig,
        &mut crate::curry::Groups::default(),
        position,
    )
}

/// [`apply_lambda_values`] with the groups of a curried application spine that
/// this lambda does not itself consume ([`crate::curry`]). While groups remain
/// the value produced is the spine's result rather than this lambda's return,
/// so this lambda's return adaptation is skipped until the last group.
pub(crate) fn reduce_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
    sig: Option<&FnSig>,
    rest: &mut crate::curry::Groups<'_>,
    position: Option<Position>,
) -> Result<Value> {
    cg.push_scope();
    // `fn_ptr_locals` is per-FUNCTION, not per-scope ([`Codegen::begin_function`]),
    // so a function-typed lambda parameter registered below must be unwound by
    // hand — exactly as an inlined call does ([`crate::genfn`]).
    let saved_fn_ptrs = cg.fn_ptr_locals.clone();
    let saved_fn_types = cg.fn_value_types.clone();
    let lowered = (|| {
        bind_lambda_params(cg, parameters, values, sig, position)?;
        let final_group = rest.is_empty();
        let value = crate::curry::apply_groups(cg, body, rest)?;
        if final_group {
            fit_lambda_return(cg, value, sig)
        } else {
            Ok(value)
        }
    })();
    cg.fn_ptr_locals = saved_fn_ptrs;
    cg.fn_value_types = saved_fn_types;
    cg.pop_scope();
    lowered
}

/// Bind a lambda's parameters to its argument values, coercing each to the
/// parameter type its signature declares.
fn bind_lambda_params(
    cg: &mut Codegen,
    parameters: &[Parameter],
    values: Vec<Value>,
    sig: Option<&FnSig>,
    position: Option<Position>,
) -> Result<()> {
    let declared = cg.prog.lambda_type(position).cloned();
    for (index, (p, v)) in parameters.iter().zip(values).enumerate() {
        bind_lambda_fn_param(cg, &p.name, declared.as_ref(), index);
        let v = match sig.and_then(|s| s.0.get(index)) {
            Some(want) => crate::cast::coerce_semantic_param(cg, v, want)?,
            None => v,
        };
        cg.bind(p.name.clone(), v);
    }
    Ok(())
}

/// Register a function-typed lambda parameter so a call through it lowers to an
/// indirect call, the way a top-level function's own higher-order parameters are
/// registered ([`crate::lower`]).
///
/// Without this the curried ML head `feeding reading body = handle … in body ()`
/// — whose `body` is a LAMBDA parameter, not a function parameter — lowered
/// `body ()` to a direct call on an `@body` symbol nothing defines, and the
/// program failed at the LINKER. The tupled head `feeding (reading, body)` put
/// the same parameter on the function itself and so always worked
/// [FLAVOR-ML-CURRY], [FLAVOR-IR-EQUIV].
fn bind_lambda_fn_param(
    cg: &mut Codegen,
    name: &str,
    declared: Option<&osprey_types::Type>,
    index: usize,
) {
    let Some(osprey_types::Type::Fun { params, .. }) = declared else {
        return;
    };
    if let Some(ty @ osprey_types::Type::Fun { .. }) = params.get(index) {
        cg.bind_fn_local(name, ty.clone());
    }
}

/// Adapt a lambda body's value to the lambda's own inferred signature — the
/// shared tail of beta-reduction ([`apply_lambda_values`]) and kernel
/// extraction ([`crate::gpu_kernel`]), so the two lowerings of one lambda
/// produce the same value by construction [GPU-KERNEL-EXTRACT].
pub(crate) fn fit_lambda_return(
    cg: &mut Codegen,
    value: Value,
    sig: Option<&FnSig>,
) -> Result<Value> {
    // A lambda's return slot is typed by its signature, so a list literal
    // returned through a cell loses the flat tag [`crate::listlit::escaping`].
    let value = crate::listlit::escaping(cg, value);
    let value = match sig {
        Some((_, _, Some(inner), _, _)) if value.result_inner.is_some() => {
            crate::result::repack_to_inner(cg, value, *inner)?
        }
        Some((_, _, Some(inner), _, _)) => crate::result::make_ok(cg, value, *inner)?,
        Some((_, ret, None, _, _)) => crate::cast::coerce_to(cg, value, *ret)?,
        None => value,
    };
    Ok(match sig.and_then(|signature| signature.3.clone()) {
        Some(fiber) => fiber.restore(value),
        None => value,
    })
}
