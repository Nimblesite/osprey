//! Preserve application types and lexical bindings when selecting callbacks.
use super::{Callback, Codegen, CodegenError, Expr, Result};
use crate::closure::forwarding_body;
use crate::expr::gen_expr;

/// Resolve known local bodies; computed and module-stored values retain cells.
pub(crate) fn of(cg: &mut Codegen, expr: &Expr) -> Result<Callback> {
    applied(cg, expr, expr)
}

/// [`of`] for `expr` reached through the applications `callee` writes on the
/// way to it: a generic declaration's callback calls `callee` itself, so the
/// checker's bindings for this use specialise every element's call.
fn applied(cg: &mut Codegen, expr: &Expr, callee: &Expr) -> Result<Callback> {
    match expr {
        Expr::TypeApply {
            function, position, ..
        } if !file_lambda(cg, function) => {
            crate::expr::with_application(cg, function, *position, |cg| {
                applied(cg, function, callee)
            })
        }
        Expr::Identifier(_) if file_lambda(cg, expr) => value(cg, expr),
        Expr::Identifier(name) => identifier(cg, name, callee),
        Expr::Lambda { .. } => lambda(cg, expr),
        _ => value(cg, expr),
    }
}

fn lambda(cg: &Codegen, expr: &Expr) -> Result<Callback> {
    let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = expr
    else {
        return Err(CodegenError::invalid("callback lambda expected"));
    };
    let definition = (parameters.clone(), (**body).clone(), *position);
    Ok(known_lambda(cg, definition, None))
}

/// The concrete inferred type of a lambda callback's `index`-th parameter;
/// `None` for a named or computed callback, or a still-generic lambda.
pub(crate) fn param_type(
    cg: &Codegen,
    callback: &Callback,
    index: usize,
) -> Option<osprey_types::Type> {
    let (Callback::Lambda(.., position) | Callback::Closed(.., position)) = callback else {
        return None;
    };
    let declared = cg
        .prog
        .lambda_type(cg.source_function.as_deref(), *position)?;
    let osprey_types::Type::Fun { params, .. } = declared else {
        return None;
    };
    params
        .get(index)
        .filter(|ty| !osprey_types::has_type_var(ty))
        .cloned()
}

fn identifier(cg: &mut Codegen, name: &str, callee: &Expr) -> Result<Callback> {
    if let Some(sig) = cg.fn_ptr_locals.get(name) {
        return Ok(Callback::Local(name.to_string(), sig.clone()));
    }
    if let Some(target) = cg
        .call_aliases
        .get(name)
        .filter(|target| *target != name)
        .cloned()
    {
        return of(cg, &Expr::Identifier(target));
    }
    if let Some(definition) = cg.lambda_def(name).cloned() {
        return Ok(known_lambda(
            cg,
            definition,
            cg.lambda_envs.get(name).cloned(),
        ));
    }
    module_global(cg, name).unwrap_or_else(|| Ok(named(cg, name, callee)))
}

/// A function-valued module global is called through its closure cell: the
/// name has no emitted symbol of its own, so a named callback would link to
/// a `declare` no `define` ever produced. Implements
/// [MODULES-FILE-SCOPE-BINDING], [TYPE-GENERICS-FN].
fn module_global(cg: &mut Codegen, name: &str) -> Option<Result<Callback>> {
    let sig = crate::globals::fn_type(cg, name)
        .as_ref()
        .and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty))?;
    crate::globals::read(cg, name).map(|handle| Ok(Callback::Value(handle.operand, sig)))
}

fn known_lambda(
    cg: &Codegen,
    (params, body, position): crate::builder::LambdaDef,
    env: Option<crate::closure::Environment>,
) -> Callback {
    let sig = crate::expr::inline_sig(cg, position);
    match env {
        Some(env) => Callback::Closed(env, params, body, sig, position),
        None => Callback::Lambda(params, body, sig, position),
    }
}

/// A generic declaration named as a callback is called as `fn(a) => callee(a)`
/// would be, behind a lexical barrier: each element is an ordinary generic call
/// under the applications `callee` carries. Inlining the body directly lowered
/// it after those applications had ended, at its unresolved scheme, so a record
/// built there was read through another layout (#393). Implements
/// [BUILTIN-ITER-CALLBACK], [TYPE-GENERICS-FN].
fn named(cg: &Codegen, name: &str, callee: &Expr) -> Callback {
    match cg.fn_defs.get(name) {
        Some((params, ..)) if matches!(callee, Expr::TypeApply { .. }) => {
            let (parameters, body) =
                forwarding_body(&format!("{name}$argument"), params.len(), callee.clone());
            Callback::Closed(
                crate::closure::Environment::default(),
                parameters,
                body,
                None,
                None,
            )
        }
        // A bare name (an inlined parameter's alias) has no bindings to carry:
        // its body inlines at each element, typed by the element's own value.
        Some((params, body, position)) => Callback::Closed(
            crate::closure::Environment::default(),
            params.clone(),
            cg.cov_inline_body(body.clone(), *position),
            None,
            None,
        ),
        None => eta_builtin(cg, name).unwrap_or_else(|| Callback::Named(name.to_string())),
    }
}

/// A builtin with no value form has no emitted symbol either, so naming it as
/// a callback lowers it as `fn(a) => builtin(a)` would: an ordinary builtin
/// call per element. `None` for every other name.
fn eta_builtin(cg: &Codegen, name: &str) -> Option<Callback> {
    if crate::builtin_values::shadowed(cg, name) || crate::expr::has_value_form(name) {
        return None;
    }
    let osprey_types::Type::Fun { params, .. } = osprey_types::builtin_function_type(name)? else {
        return None;
    };
    let (parameters, body) = forwarding_body(
        &format!("{name}$argument"),
        params.len(),
        Expr::Identifier(name.to_string()),
    );
    Some(Callback::Lambda(parameters, body, None, None))
}

fn file_lambda(cg: &Codegen, expr: &Expr) -> bool {
    let Expr::Identifier(name) = expr else {
        return false;
    };
    cg.file_lambdas
        .get(name)
        .is_some_and(|file| cg.lambda_def(name) == Some(file))
}

fn value(cg: &mut Codegen, expr: &Expr) -> Result<Callback> {
    let sig = cg
        .callee_fn_type(expr)
        .as_ref()
        .and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty))
        .ok_or_else(|| {
            CodegenError::unsupported("iterator callback must be a function name or lambda")
        })?;
    let handle = gen_expr(cg, expr)?;
    Ok(Callback::Value(handle.operand, sig))
}
