//! Monomorphisation of RECURSIVE generic functions — the one place
//! polymorphism is resolved by emitting a definition rather than by inlining
//! ([`crate::genfn`] does the rest by inlining).
//!
//! A generic function has no `@name` symbol: it exists only as the body
//! [`crate::genfn::try_inline`] expands at each call site with that site's
//! concrete argument types. A body that calls itself cannot be expanded that
//! way — the expansion never terminates — so an unannotated recursive helper
//! was rejected outright (`annotate its parameters and return type so it is
//! emitted as a real function`).
//!
//! Here the call site's argument types fix ONE instantiation, so that
//! instantiation is emitted as a real function `@name$N` with a concrete
//! signature and called. While its body is being emitted the instantiation is
//! already registered, so the self-call inside it resolves to the SAME symbol
//! and lowers to a direct recursive call. Two call sites with different
//! argument types get two definitions; two with the same types share one.
//!
//! Implements [GPU-KERNEL-FORM] (docs/specs/0034-GPUComputation.md): a
//! recursive helper reached from a kernel — a row walk over a flat matrix is
//! the canonical one — is a valid kernel, so it must compile without the
//! annotations Osprey's inference otherwise makes redundant. Also
//! [TYPE-FN-GENERIC] via [plan 0002](docs/plans/0002-codegen-generic-function-values.md).

use crate::builder::{Codegen, FnSig, ParamSig};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter};

/// Symbol prefix of every emitted instantiation. The suffix is a
/// module-monotonic id advanced only by specialisation, so a symbol is a pure
/// function of AST walk order — identical for a Default/ML twin pair
/// [FLAVOR-IR-EQUIV].
const MONO_INFIX: &str = "$mono";

/// One emitted instantiation: the symbol to call and the signature to call it
/// with.
#[derive(Clone, Debug)]
pub(crate) struct Instantiation {
    symbol: String,
    sig: FnSig,
}

/// Whether `body` references `name` — direct self-recursion, which inlining
/// cannot specialise. Mutual recursion still trips [`crate::genfn`]'s re-entry
/// guard and its diagnostic. The free-identifier collector already answers
/// "which names does this body reach", parameters subtracted
/// ([`crate::freevars`]).
pub(crate) fn calls_itself(name: &str, body: &Expr) -> bool {
    let mut names = std::collections::BTreeSet::new();
    crate::freevars::free_idents(body, &mut names);
    names.contains(name)
}

/// Specialise `name` for this call site's argument types: emit the
/// instantiation if it is new, then call it.
pub(crate) fn specialize(
    cg: &mut Codegen,
    name: &str,
    parameters: &[Parameter],
    body: &Expr,
    args: &[&Expr],
) -> Result<Value> {
    if parameters.len() != args.len() {
        return Err(CodegenError::invalid(format!(
            "`{name}`: expected {} argument(s), got {}",
            parameters.len(),
            args.len()
        )));
    }
    let values = args
        .iter()
        .map(|a| gen_expr(cg, a))
        .collect::<Result<Vec<_>>>()?;
    let key = instantiation_key(name, &values);
    let known = cg.monofns.get(&key).cloned();
    let target = match known {
        Some(existing) => existing,
        None => emit(cg, name, key, parameters, body, &values)?,
    };
    call(cg, &target, values)
}

/// The instantiation this call selects: the function name plus each argument's
/// lowered representation. Two calls agree exactly when their arguments travel
/// identically, which is what the emitted signature is built from.
fn instantiation_key(name: &str, values: &[Value]) -> String {
    let args: Vec<String> = values.iter().map(slot_spelling).collect();
    format!("{name}({})", args.join(","))
}

/// One argument's contribution to the key: its LLVM type, its owner tag (a
/// `Gpu#double` buffer and a bare handle are different instantiations) and its
/// Result layout.
fn slot_spelling(v: &Value) -> String {
    let owner = v.osp_ty.clone().unwrap_or_default();
    let result = v.result_inner.map_or_else(String::new, |i| i.to_string());
    format!("{}:{owner}:{result}", v.ty.as_str())
}

/// Emit `@name$monoN` for one instantiation and register it BEFORE lowering the
/// body, so the self-call inside resolves to the symbol being defined.
fn emit(
    cg: &mut Codegen,
    name: &str,
    key: String,
    parameters: &[Parameter],
    body: &Expr,
    values: &[Value],
) -> Result<Instantiation> {
    let ret = return_slot(cg, name)?;
    let params: Vec<ParamSig> = values.iter().map(param_of).collect();
    let owners: Vec<Option<String>> = values.iter().map(|v| v.osp_ty.clone()).collect();
    emit_at(cg, name, key, parameters, body, (&params, ret, &owners))
}

/// One instantiation's lowered ABI: the parameter slots, the return slot paired
/// with its `Result` inner type, and the owner tag each parameter carries in.
type Abi<'a> = (&'a [ParamSig], (LType, Option<LType>), &'a [Option<String>]);

/// Emit one instantiation of `name` from its lowered ABI: register the symbol
/// BEFORE the body so a self-call resolves to it, then lower the body into a
/// nested function.
fn emit_at(
    cg: &mut Codegen,
    name: &str,
    key: String,
    parameters: &[Parameter],
    body: &Expr,
    slots: Abi<'_>,
) -> Result<Instantiation> {
    let (params, ret, owners) = slots;
    let target = Instantiation {
        symbol: format!("{name}{MONO_INFIX}{}", cg.next_monofn_id()),
        sig: (params.to_vec(), ret.0, ret.1, None),
    };
    let _ = cg.monofns.insert(key, target.clone());
    let saved = cg.enter_nested_fn();
    let plist = bind_params(cg, parameters, owners, params);
    let emitted = lower_body(cg, body, &target.sig);
    let spelling = crate::llty::ret_spelling(ret.0, ret.1);
    // Restore the enclosing function BEFORE propagating, so a failed body never
    // leaves the host's emission state clobbered ([`crate::gpu_kernel`] does
    // the same).
    cg.exit_nested_fn(saved, &spelling, &target.symbol, &plist);
    let _ = emitted?;
    Ok(target)
}

/// Emit `name` as a REAL function at the types a C callback slot dictates, and
/// return its symbol — for `httpListen`/`spawnProcess` handlers, whose only
/// caller is the C runtime.
///
/// A handler whose types are inferred rather than written is generic, and a
/// generic function has no `@name` symbol: it exists only as the body inlined
/// per Osprey call site ([`crate::genfn`]). A callback has no Osprey call site,
/// so codegen emitted the `@name` REFERENCE, never the body, and still returned
/// `Ok` — clang then rejected the module with `use of undefined value`. The
/// builtin's declared parameter type is the one instantiation the C side will
/// ever call, so it is the one to emit ([BUILTIN-HTTP], [BUILTIN-PROCESS]).
pub(crate) fn specialize_callback(
    cg: &mut Codegen,
    name: &str,
    declared: &(Vec<osprey_types::Type>, osprey_types::Type),
) -> Result<Option<String>> {
    let Some((parameters, body)) = cg.fn_defs.get(name).cloned() else {
        return Ok(None);
    };
    let (param_types, ret_type) = declared;
    if parameters.len() != param_types.len() {
        return Ok(None);
    }
    let params: Vec<ParamSig> = param_types.iter().map(ParamSig::of).collect();
    let key = format!("{name}$callback");
    if let Some(existing) = cg.monofns.get(&key) {
        return Ok(Some(existing.symbol.clone()));
    }
    let owners: Vec<Option<String>> = param_types.iter().map(crate::types::owner_name).collect();
    let ret = (
        crate::types::ltype_of(ret_type),
        crate::types::result_inner(ret_type),
    );
    // The emitted body runs whenever the C runtime dispatches to it, exactly as
    // a monomorphic definition's line is covered [TESTING-COVERAGE-CODEGEN].
    cg.cov_hit_inline_fn(name);
    let target = emit_at(cg, name, key, &parameters, &body, (&params, ret, &owners))?;
    Ok(Some(target.symbol))
}

/// The instantiation's return slot, from the signature inference resolved for
/// the function. A return type inference left open has no slot to emit, so the
/// annotation diagnostic still stands for that case.
fn return_slot(cg: &Codegen, name: &str) -> Result<(LType, Option<LType>)> {
    let ty = cg.prog.return_type(name).ok_or_else(|| unresolved(name))?;
    if osprey_types::has_type_var(ty) {
        return Err(unresolved(name));
    }
    Ok((crate::types::ltype_of(ty), crate::types::result_inner(ty)))
}

/// The diagnostic for a recursive generic whose return type inference could not
/// resolve: its instantiations have no signature to emit, so annotating is
/// still the answer.
fn unresolved(name: &str) -> CodegenError {
    CodegenError::unsupported(format!(
        "`{name}` is recursive and its return type is not inferred; \
         annotate its return type so it is emitted as a real function"
    ))
}

/// The parameter slot an argument travels in, keeping its Result layout so a
/// `Result` argument stays one block rather than an erased word.
fn param_of(v: &Value) -> ParamSig {
    ParamSig {
        ty: v.ty,
        result_inner: v.result_inner,
        fiber: None,
    }
}

/// Bind each parameter to its incoming register, carrying its owner tag in so
/// the body sees the same typed handle the caller passed (a `Gpu#double` buffer
/// stays element-typed inside the specialisation).
fn bind_params(
    cg: &mut Codegen,
    parameters: &[Parameter],
    owners: &[Option<String>],
    params: &[ParamSig],
) -> Vec<(LType, String)> {
    let mut out = Vec::with_capacity(parameters.len());
    for (i, ((p, sig), owner)) in parameters.iter().zip(params).zip(owners).enumerate() {
        let reg = crate::llty::param_register(i);
        // `incoming_param` registers no ownership: parameters are BORROWED for
        // the call's duration, exactly as a top-level function's are
        // [GC-ARC-PERCEUS].
        let value = crate::cast::incoming_param(cg, format!("%{reg}"), *sig, owner.clone());
        cg.bind(p.name.clone(), value);
        out.push((sig.ty, reg));
    }
    out
}

/// Lower the instantiation's body and emit its `ret`, fitted to the declared
/// return slot exactly as a top-level function's body is.
fn lower_body(cg: &mut Codegen, body: &Expr, sig: &FnSig) -> Result<Value> {
    let outer = std::mem::replace(&mut cg.value_discarded, false);
    let lowered = gen_expr(cg, body).and_then(|v| crate::expr::fit_lambda_return(cg, v, Some(sig)));
    cg.value_discarded = outer;
    let value = lowered?;
    // Function epilogue: the return transfers +1, owned locals drop
    // [GC-ARC-PERCEUS].
    crate::arc::epilogue(cg, Some(&value));
    cg.emit(format!("ret {} {}", value.llvm_ty(), value.operand));
    Ok(value)
}

/// Call one instantiation with the argument values the call site lowered.
fn call(cg: &mut Codegen, target: &Instantiation, args: Vec<Value>) -> Result<Value> {
    let typed = crate::closure::coerce_typed_args(cg, &target.sig, args)?;
    let ret = crate::llty::ret_spelling(target.sig.1, target.sig.2);
    let reg = cg.emit_reg(format!(
        "call {ret} @{}({})",
        target.symbol,
        typed.join(", ")
    ));
    let value = crate::closure::returned(reg, &target.sig);
    // Callee epilogues transfer +1 on every return [GC-ARC-PERCEUS].
    crate::arc::own(cg, &value);
    Ok(value)
}
