//! Polymorphism lowering: specialise a generic user function at each call site
//! by inlining its body with the concrete argument types bound to its
//! parameters, and lower a call through a function-typed parameter (`f(x)` where
//! `f: (int) -> int`) to an indirect call. Inlining + indirect calls reach the
//! same runtime behaviour as emitting a name-mangled monomorphic copy per
//! instantiation (`identity_i64_i64`, `applyInt_fn_i64_i64`) would — without
//! the mangling.

use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::llty::Value;
use osprey_ast::{Expr, NamedArgument, Parameter};

/// Specialize a generic call without exposing its caller's lexical bindings.
/// Implements [TYPE-GENERICS-FN].
pub(crate) fn try_inline(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
    rest: &[crate::curry::ArgGroup<'_>],
) -> Result<Option<Value>> {
    let Some((params, body, _)) = inline_definition(cg, name)? else {
        return Ok(None);
    };
    if crate::monofn::calls_itself(name, &body) {
        if !rest.is_empty() {
            return Ok(None);
        }
        let exprs = pair_args(&params, args, named)
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>();
        return crate::monofn::specialize(cg, name, &params, &body, &exprs).map(Some);
    }
    cg.cov_hit_inline_fn(name);
    // Caller arguments precede both the lexical barrier and recursion guard.
    let lowered = lower_inline_args(cg, name, &params, args, named)?;
    inline_body(cg, name, &params, &body, lowered, rest).map(Some)
}

fn inline_definition(cg: &Codegen, name: &str) -> Result<Option<crate::builder::LambdaDef>> {
    if cg.inlining.contains(name) {
        return Err(crate::error::CodegenError::unsupported(format!(
            "`{name}` is recursive but its signature is not fully inferred; \
             annotate its parameters and return type so it is emitted as a real function"
        )));
    }
    Ok(cg.fn_defs.get(name).cloned())
}

fn inline_body(
    cg: &mut Codegen,
    name: &str,
    params: &[Parameter],
    body: &Expr,
    lowered: Vec<InlineArg>,
    rest: &[crate::curry::ArgGroup<'_>],
) -> Result<Value> {
    let mut groups = crate::curry::Groups::file_scoped(cg, rest);
    cg.cell_vars = crate::effects::captured_mut_vars(body);
    let _ = cg.inlining.insert(name.to_string());
    for (parameter, argument) in params.iter().zip(lowered) {
        bind_inline_arg(cg, parameter, argument);
    }
    let result = crate::curry::apply_groups(cg, body, &mut groups);
    groups.restore(cg);
    result.and_then(|value| inline_result(cg, name, value, rest.is_empty()))
}

fn inline_result(cg: &mut Codegen, name: &str, value: Value, final_group: bool) -> Result<Value> {
    let returns_result = cg.prog.return_type(name).is_some_and(|ty| {
        matches!(ty, osprey_types::Type::Con { name, .. } if name == osprey_types::names::RESULT)
    });
    if returns_result && final_group && value.result_inner.is_none() {
        let inner = value.ty;
        crate::result::make_ok(cg, value, inner)
    } else {
        Ok(value)
    }
}

/// Re-lay a PLACEHOLDER `Result` argument onto the success slot inference gave
/// the parameter.
///
/// A bare `Error { message: m }` has no success value, so its block is built
/// with `m`'s own type in slot 0 and marked a placeholder
/// ([`crate::aggregate`]). Handed straight to an inlined generic body, the
/// `Success { value }` arm then binds that arbitrary slot: `resultScore(Error {
/// message: "x" })` matched a `string` success against an `int` error arm, the
/// arms disagreed, and the whole `match` collapsed to a discarded value that
/// printed `0` — a silently wrong answer, not a diagnostic. Implements
/// [ERR-PAYLOAD].
fn relaid_placeholder(
    cg: &mut Codegen,
    v: Value,
    declared: Option<&osprey_types::Type>,
) -> Result<Value> {
    let Some(inner) = declared.and_then(crate::types::result_inner) else {
        return Ok(v);
    };
    if !v.result_inner_is_placeholder || v.result_inner == Some(inner) {
        return Ok(v);
    }
    crate::result::repack_to_inner(cg, v, inner)
}

/// One inlined-call argument, already lowered in the caller's scope: either the
/// callee a bare name stands for (so the parameter stays callable) or a value
/// paired with the function type to register for it.
enum InlineArg {
    Alias(String),
    Value(Box<Value>, Option<osprey_types::Type>),
}

/// Lower every argument of an inlined call in the CALLER's scope, in call
/// order.
fn lower_inline_args(
    cg: &mut Codegen,
    name: &str,
    params: &[Parameter],
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Vec<InlineArg>> {
    let declared = cg.prog.param_types(name).map(<[_]>::to_vec);
    cg.with_caller_types(|cg| {
        pair_args(params, args, named)
            .into_iter()
            .enumerate()
            .map(|(i, (p, a))| lower_inline_arg(cg, p, a, declared.as_ref().and_then(|d| d.get(i))))
            .collect()
    })
}

/// Lower one inlined-call argument: a bare callee name becomes a call alias;
/// anything else lowers to a value. A function-valued argument (a lambda, a
/// function-typed local, a call returning a function) also carries the
/// parameter's signature, so the inlined body's `f(x)` dispatches through the
/// closure cell instead of emitting a call to a symbol that does not exist.
fn lower_inline_arg(
    cg: &mut Codegen,
    p: &Parameter,
    a: &Expr,
    declared: Option<&osprey_types::Type>,
) -> Result<InlineArg> {
    if let Some(callee) = alias_target(cg, a) {
        return Ok(InlineArg::Alias(callee));
    }
    let v = gen_expr(cg, a)?;
    let v = if p
        .ty
        .as_ref()
        .is_some_and(|ty| ty.name == osprey_types::names::RESULT)
        && v.result_inner.is_none()
    {
        let inner = v.ty;
        crate::result::make_ok(cg, v, inner)?
    } else {
        relaid_placeholder(cg, v, declared)?
    };
    let function_type = v
        .inferred_type
        .as_ref()
        .filter(|ty| matches!(ty, osprey_types::Type::Fun { .. }))
        .cloned()
        .or_else(|| crate::stmt::fn_result_type(cg, a));
    Ok(InlineArg::Value(Box::new(v), function_type))
}

/// Bind one already-lowered argument to its parameter inside the inlined body's
/// scope.
fn bind_inline_arg(cg: &mut Codegen, p: &Parameter, arg: InlineArg) {
    match arg {
        InlineArg::Alias(callee) => {
            let _ = cg.call_aliases.insert(p.name.clone(), callee);
        }
        InlineArg::Value(v, fn_ty) => {
            if let Some(ty) = fn_ty {
                cg.bind_fn_local(&p.name, ty);
            }
            cg.bind(p.name.clone(), *v);
        }
    }
}

/// Pair parameters with their argument expressions — named arguments matched by
/// name, otherwise positional.
fn pair_args<'a>(
    params: &'a [Parameter],
    args: &'a [Expr],
    named: &'a [NamedArgument],
) -> Vec<(&'a Parameter, &'a Expr)> {
    if named.is_empty() {
        params.iter().zip(args).collect()
    } else {
        params
            .iter()
            .enumerate()
            .filter_map(|(index, p)| {
                args.get(index)
                    .or_else(|| named.iter().find(|n| n.name == p.name).map(|n| &n.value))
                    .map(|argument| (p, argument))
            })
            .collect()
    }
}

/// When an argument is a bare name that is a callee (a function/builtin) rather
/// than a bound value or a nullary constructor, return that name so the
/// parameter can redirect calls to it.
fn alias_target(cg: &Codegen, arg: &Expr) -> Option<String> {
    match arg {
        Expr::Identifier(n)
            if cg.lookup(n).is_none()
                && !cg.is_ctor(n)
                && (cg.lambda_def(n).is_some() || !value_binding(cg, n)) =>
        {
            Some(n.clone())
        }
        _ => None,
    }
}

/// Whether `name` is a file-scope or handler-promoted binding holding a VALUE.
///
/// Absence from `lookup` does not mean "a bare callee name". Two ordinary
/// bindings are deliberately not scope-bound: a `mut` a handler arm reads,
/// which [EFFECTS-HANDLER-STATE] promotes to a heap cell, and a file-scope
/// binding read from inside a function body, which lives in its module global
/// ([`crate::globals`]). Reading either as a callee aliased it as a function,
/// and codegen then demanded a signature the author never wrote. A global that
/// really does hold a function keeps aliasing, which is what `let g = identity`
/// needs.
fn value_binding(cg: &Codegen, name: &str) -> bool {
    cg.cell_slots.contains_key(name)
        || (cg.module_globals.contains_key(name) && crate::globals::fn_type(cg, name).is_none())
}

/// If `name` is a function-typed local (a higher-order parameter or a let-bound
/// function value), lower `f(x)` to a closure call: extract the fnptr from the
/// cell `name` holds and call it with the cell as env ([`crate::closure`]).
pub(crate) fn try_indirect(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    // A function value reached through a module global has no entry in this
    // frame's tables; its ABI comes from the global's inferred type instead
    // ([`crate::globals`]).
    let Some(sig) = cg.fn_ptr_locals.get(name).cloned().or_else(|| {
        crate::globals::fn_type(cg, name)
            .as_ref()
            .and_then(|t| Codegen::fn_value_sig(&cg.prog, t))
    }) else {
        return Ok(None);
    };
    let local = cg.cell_read(name).or_else(|| cg.lookup(name));
    let Some(handle) = local.or_else(|| crate::globals::read(cg, name)) else {
        return Ok(None);
    };
    let exprs = crate::expr::arg_exprs(args, named);
    crate::closure::cell_call_exprs(cg, &handle.operand, &sig, &exprs).map(Some)
}
