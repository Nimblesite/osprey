//! Preserve application types and lexical bindings when selecting callbacks.
use super::{Callback, Codegen, CodegenError, Expr, Result};
use crate::expr::gen_expr;

/// Resolve known local bodies; computed and module-stored values retain cells.
pub(crate) fn of(cg: &mut Codegen, expr: &Expr) -> Result<Callback> {
    match expr {
        Expr::TypeApply {
            function, position, ..
        } if !file_lambda(cg, function) => {
            crate::expr::with_application(cg, *position, |cg| of(cg, function))
        }
        Expr::Identifier(_) if file_lambda(cg, expr) => value(cg, expr),
        Expr::Identifier(name) => identifier(cg, name),
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

fn identifier(cg: &mut Codegen, name: &str) -> Result<Callback> {
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
    Ok(match cg.lambda_def(name).cloned() {
        Some(definition) => known_lambda(cg, definition, cg.lambda_envs.get(name).cloned()),
        None => named(cg, name),
    })
}

fn known_lambda(
    cg: &Codegen,
    (params, body, position): crate::builder::LambdaDef,
    env: Option<crate::closure::Environment>,
) -> Callback {
    let sig = cg
        .prog
        .lambda_type(position)
        .and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty));
    match env {
        Some(env) => Callback::Closed(env, params, body, sig, position),
        None => Callback::Lambda(params, body, sig, position),
    }
}

/// Generic declarations inline behind a lexical barrier, never launch shadows.
/// Implements [BUILTIN-ITER-CALLBACK], [TYPE-GENERICS-FN].
fn named(cg: &Codegen, name: &str) -> Callback {
    match cg.fn_defs.get(name) {
        Some((params, body, _)) => Callback::Closed(
            crate::closure::Environment::default(),
            params.clone(),
            body.clone(),
            None,
            None,
        ),
        None => Callback::Named(name.to_string()),
    }
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
