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

/// If `name` is a generic user function, inline its body with the call's
/// arguments bound to its parameters (so its type variables monomorphise to the
/// concrete argument types here) and return the result. A re-entry guard stops
/// a recursive generic call from inlining forever — and fails closed, because
/// the fallback (a direct call) would name a definition that is never emitted:
/// generic functions exist only as inlined specialisations.
pub(crate) fn try_inline(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
    rest: &[crate::curry::ArgGroup<'_>],
) -> Result<Option<Value>> {
    if cg.inlining.contains(name) {
        return Err(crate::error::CodegenError::unsupported(format!(
            "`{name}` is recursive but its signature is not fully inferred; \
             annotate its parameters and return type so it is emitted as a real function"
        )));
    }
    let Some((params, body)) = cg.fn_defs.get(name).cloned() else {
        return Ok(None);
    };
    // A body that calls itself cannot be specialised by expanding it here, so
    // this instantiation is emitted as a real function instead
    // ([`crate::monofn`]).
    if crate::monofn::calls_itself(name, &body) {
        // A recursive definition is emitted as a real function, so its result
        // is a concrete value the ordinary call paths apply the rest of the
        // spine to; there is no body here to beta-reduce them into.
        if !rest.is_empty() {
            return Ok(None);
        }
        let exprs: Vec<&Expr> = pair_args(&params, args, named)
            .into_iter()
            .map(|(_, a)| a)
            .collect();
        return crate::monofn::specialize(cg, name, &params, &body, &exprs).map(Some);
    }
    let returns_result = cg
        .prog
        .return_type(name)
        .is_some_and(|ty| {
            matches!(ty, osprey_types::Type::Con { name, .. } if name == osprey_types::names::RESULT)
        });
    // Pair each parameter with its argument expression (named by name, else
    // positional), then bind it as a value — or, when the argument is a bare
    // callee name, as a call alias so the parameter stays callable.
    let saved_aliases = cg.call_aliases.clone();
    let saved_ptr_locals = cg.fn_ptr_locals.clone();
    let saved_value_types = cg.fn_value_types.clone();
    // The inlined body executing covers the definition line, exactly as a
    // monomorphic call covers it via gen_function [TESTING-COVERAGE-CODEGEN].
    cg.cov_hit_inline_fn(name);
    // The arguments are the CALLER's expressions, so they are lowered in the
    // CALLER's scope and BEFORE the re-entry guard is armed. Lowering them
    // after `inlining.insert` made `identity(identity(x))` — one generic call
    // nested in another's argument — report `identity` as recursive and demand
    // annotations for a body that never calls itself. Lowering them after
    // `push_scope` additionally let a parameter bound from an earlier argument
    // shadow a caller binding a later argument reads.
    let lowered = lower_inline_args(cg, name, &params, args, named);
    cg.push_scope();
    // The inlined body is a DIFFERENT AST from the caller's, so the caller's
    // cell-promotion pass never saw its `mut` bindings. Without this union a
    // `mut` the body declares and a handler arm captures stays an unpromoted
    // local: the arm writes a private copy and the read after the `handle`
    // returns the initial value — a silently wrong answer, not a diagnostic
    // ([EFFECTS-HANDLER-STATE], [`crate::effects::captured_mut_vars`]).
    let saved_cells = cg.cell_vars.clone();
    cg.cell_vars
        .extend(crate::effects::captured_mut_vars(&body));
    let _ = cg.inlining.insert(name.to_string());
    let result = lowered.and_then(|lowered| {
        for (p, arg) in pair_args(&params, args, named)
            .into_iter()
            .map(|(p, _)| p)
            .zip(lowered)
        {
            bind_inline_arg(cg, p, arg);
        }
        crate::curry::apply_groups(cg, &body, rest)
    });
    let _ = cg.inlining.remove(name);
    cg.cell_vars = saved_cells;
    cg.pop_scope();
    cg.call_aliases = saved_aliases;
    cg.fn_ptr_locals = saved_ptr_locals;
    cg.fn_value_types = saved_value_types;
    result
        .and_then(|value| {
            if returns_result && rest.is_empty() && value.result_inner.is_none() {
                let inner = value.ty;
                crate::result::make_ok(cg, value, inner)
            } else {
                Ok(value)
            }
        })
        .map(Some)
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
    pair_args(params, args, named)
        .into_iter()
        .enumerate()
        .map(|(i, (p, a))| lower_inline_arg(cg, p, a, declared.as_ref().and_then(|d| d.get(i))))
        .collect()
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
    Ok(InlineArg::Value(
        Box::new(v),
        crate::stmt::fn_result_type(cg, a),
    ))
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
            .filter_map(|p| {
                named
                    .iter()
                    .find(|n| n.name == p.name)
                    .map(|n| (p, &n.value))
            })
            .collect()
    }
}

/// When an argument is a bare name that is a callee (a function/builtin) rather
/// than a bound value or a nullary constructor, return that name so the
/// parameter can redirect calls to it.
fn alias_target(cg: &Codegen, arg: &Expr) -> Option<String> {
    match arg {
        Expr::Identifier(n) if cg.lookup(n).is_none() && !cg.is_ctor(n) => Some(n.clone()),
        _ => None,
    }
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
