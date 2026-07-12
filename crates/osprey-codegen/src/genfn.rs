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
/// concrete argument types here) and return the result. A re-entry guard makes a
/// recursive generic call fall back to a direct call rather than inline forever.
pub(crate) fn try_inline(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    if cg.inlining.contains(name) {
        return Ok(None);
    }
    let Some((params, body)) = cg.fn_defs.get(name).cloned() else {
        return Ok(None);
    };
    // Pair each parameter with its argument expression (named by name, else
    // positional), then bind it as a value — or, when the argument is a bare
    // callee name, as a call alias so the parameter stays callable.
    let saved_aliases = cg.call_aliases.clone();
    let saved_ptr_locals = cg.fn_ptr_locals.clone();
    let saved_value_types = cg.fn_value_types.clone();
    cg.push_scope();
    let _ = cg.inlining.insert(name.to_string());
    let result = (|| {
        for (p, a) in pair_args(&params, args, named) {
            bind_inline_arg(cg, p, a)?;
        }
        gen_expr(cg, &body)
    })();
    let _ = cg.inlining.remove(name);
    cg.pop_scope();
    cg.call_aliases = saved_aliases;
    cg.fn_ptr_locals = saved_ptr_locals;
    cg.fn_value_types = saved_value_types;
    result.map(Some)
}

/// Bind one inlined-call argument to its parameter: a bare callee name becomes
/// a call alias; anything else binds as a value. A function-valued argument (a
/// lambda, a function-typed local, a call returning a function) also registers
/// the parameter's signature, so the inlined body's `f(x)` dispatches through
/// the closure cell instead of emitting a call to a symbol that does not exist.
fn bind_inline_arg(cg: &mut Codegen, p: &Parameter, a: &Expr) -> Result<()> {
    if let Some(callee) = alias_target(cg, a) {
        let _ = cg.call_aliases.insert(p.name.clone(), callee);
        return Ok(());
    }
    let v = gen_expr(cg, a)?;
    if let Some(ty) = crate::lower::fn_result_type(cg, a) {
        cg.bind_fn_local(&p.name, ty);
    }
    cg.bind(p.name.clone(), v);
    Ok(())
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
    let Some(sig) = cg.fn_ptr_locals.get(name).cloned() else {
        return Ok(None);
    };
    let Some(handle) = cg.lookup(name) else {
        return Ok(None);
    };
    let exprs = crate::expr::arg_exprs(args, named);
    crate::closure::cell_call_exprs(cg, &handle.operand, &sig, &exprs).map(Some)
}
