//! The one function-value representation: a closure cell. Implements
//! [TYPE-FN-CLOSURE].
//!
//! Every Osprey function VALUE — a lambda used as a value, a top-level function
//! passed to a function-typed parameter, a returned closure — is an `i8*`
//! pointing at a cell `{ i8* fnptr, capture0, capture1, … }`. The pointed-to
//! function takes a hidden leading `i8* %__env` (the cell itself) and reloads
//! its captures from it, so a closure returned from its maker keeps working
//! after the maker's frame is gone. A capture-free closure costs nothing at
//! runtime: its cell is one private constant global. A capturing closure
//! `malloc`s one cell per evaluation (captures are immutable, by value — the
//! cell is never written after construction, so instances never alias state).
//!
//! Callers extract the fnptr from the cell and pass the cell back as the env
//! ([`cell_call`]). Top-level functions used as values get a once-per-module
//! forwarder that drops the env ([`named_fn_cell`]). The effect-handler ABI in
//! `effects.rs` keeps its own bare-pointer convention (handlers are not
//! first-class values) and is intentionally untouched.

use crate::builder::{Codegen, FnSig};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::freevars::free_idents;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter, Position};
use std::collections::BTreeSet;

/// One captured binding: its source name and the parent-scope [`Value`] whose
/// metadata (Result shape, owner tag) the reload inside the closure must keep.
pub(crate) struct Capture {
    name: String,
    val: Value,
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
    let ty = cg
        .prog
        .lambda_type(position)
        .ok_or_else(|| CodegenError::invalid("lambda has no inferred function type"))?;
    if !crate::types::fn_value_concrete(ty) {
        return Err(CodegenError::unsupported(
            "a closure value with a still-generic type (wrap it in a function with concrete parameter/return types)",
        ));
    }
    let sig = Codegen::fn_value_sig(ty)
        .ok_or_else(|| CodegenError::invalid("lambda has no inferred function type"))?;
    emit_closure(cg, parameters, body, &sig)
}

/// Emit a lambda as a closure value with the given signature (the consuming
/// slot's ABI when known, else the lambda's own inferred type).
pub(crate) fn emit_closure(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
) -> Result<Value> {
    emit_closure_keyed(cg, parameters, body, sig, None)
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
) -> Result<Value> {
    let caps = capture_list(cg, parameters, body);
    let key = key.filter(|_| caps.is_empty());
    let v = match key.as_deref().and_then(|k| cg.fnval_cells.get(k).cloned()) {
        Some(cell) => Value::new(
            cg.emit_reg(format!("bitcast {{ i8* }}* {cell} to i8*")),
            LType::Ptr,
        ),
        None => emit_fresh_closure(cg, &caps, parameters, body, sig, key)?,
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
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
    key: Option<String>,
) -> Result<Value> {
    let id = cg.next_lambda_id();
    let fn_name = format!("__closure_fn_{id}");
    let cell_ty = cell_struct_ty(caps);
    emit_closure_fn(cg, &fn_name, &cell_ty, caps, parameters, body, sig)?;
    if let Some(k) = key {
        let _ = cg.fnval_cells.insert(k, format!("@__closure_cell_{id}"));
    }
    Ok(cell_value(cg, id, &fn_name, &cell_ty, caps, sig))
}

/// The emit-once key of `target` specialised at `sig`: two uses share a body
/// only when both the function AND the whole ABI spelling match. The `|`
/// separator cannot occur in a function name, so a specialisation key can never
/// collide with the bare name [`emit_forwarder`] registers for a monomorphic
/// forwarder in the same map.
pub(crate) fn specialisation_key(target: &str, sig: &FnSig) -> String {
    let (ret, plist) = spelling_with_env(sig);
    format!("{target}|{ret}({plist})")
}

/// The free identifiers of `body` (minus the lambda's own parameters) that are
/// bound to a value in the enclosing scope — the closure's captures, in stable
/// (sorted) order. Also used by `fiber::gen_spawn` (a spawn body is a
/// zero-parameter closure).
pub(crate) fn capture_list(cg: &Codegen, parameters: &[Parameter], body: &Expr) -> Vec<Capture> {
    let mut names = BTreeSet::new();
    free_idents(body, &mut names);
    names
        .into_iter()
        .filter(|n| !parameters.iter().any(|p| &p.name == n))
        .filter_map(|name| cg.lookup(&name).map(|val| Capture { name, val }))
        .collect()
}

/// The LLVM struct spelling of a closure cell: the fnptr slot then one slot per
/// capture, each in the capture's own travelling type.
pub(crate) fn cell_struct_ty(caps: &[Capture]) -> String {
    let mut parts = vec!["i8*".to_string()];
    parts.extend(caps.iter().map(|c| c.val.llvm_ty()));
    format!("{{ {} }}", parts.join(", "))
}

/// Emit the lifted closure function: `define {ret} @{fn_name}(i8* %__env, …)`,
/// reloading each capture from the cell before lowering the body.
fn emit_closure_fn(
    cg: &mut Codegen,
    fn_name: &str,
    cell_ty: &str,
    caps: &[Capture],
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
) -> Result<()> {
    let (param_tys, ret_ty, ret_inner) = sig;
    let (ret_spelling, _) = spelling(sig);
    let saved = cg.enter_nested_fn();
    reload_captures(cg, cell_ty, caps);
    let mut params = vec![(LType::Ptr, String::from("__env"))];
    params.extend(bind_params(cg, parameters, param_tys));
    let emitted = closure_return(cg, body, *ret_ty, *ret_inner);
    cg.exit_nested_fn(saved, &ret_spelling, fn_name, &params);
    emitted
}

/// Bind each lambda parameter as its incoming LLVM register and collect the
/// `define`'s parameter list.
fn bind_params(
    cg: &mut Codegen,
    parameters: &[Parameter],
    param_tys: &[LType],
) -> Vec<(LType, String)> {
    let mut out = Vec::with_capacity(parameters.len());
    for (i, (p, pty)) in parameters.iter().zip(param_tys).enumerate() {
        let reg = crate::llty::param_register(i);
        cg.bind(p.name.clone(), Value::new(format!("%{reg}"), *pty));
        out.push((*pty, reg));
    }
    out
}

/// Inside the closure function: cast `%__env` back to the cell type and load
/// each capture into scope (parameters bind after, so they shadow correctly).
pub(crate) fn reload_captures(cg: &mut Codegen, cell_ty: &str, caps: &[Capture]) {
    if caps.is_empty() {
        return;
    }
    let cell = cg.emit_reg(format!("bitcast i8* %__env to {cell_ty}*"));
    for (i, c) in caps.iter().enumerate() {
        let slot = i + 1;
        let p = cg.emit_reg(format!(
            "getelementptr {cell_ty}, {cell_ty}* {cell}, i32 0, i32 {slot}"
        ));
        let lty = c.val.llvm_ty();
        let r = cg.emit_reg(format!("load {lty}, {lty}* {p}"));
        let mut v = c.val.clone();
        v.operand = r;
        cg.bind(c.name.clone(), v);
    }
}

/// Build the cell and hand back its `i8*` handle. Capture-free closures share
/// one private constant global; capturing ones `malloc` a cell and store the
/// fnptr plus each captured value.
pub(crate) fn cell_value(
    cg: &mut Codegen,
    id: usize,
    fn_name: &str,
    cell_ty: &str,
    caps: &[Capture],
    sig: &FnSig,
) -> Value {
    let fnptr = fnptr_const(fn_name, sig);
    if caps.is_empty() {
        let g = format!("@__closure_cell_{id}");
        cg.add_global(format!(
            "{g} = private unnamed_addr constant {{ i8* }} {{ i8* {fnptr} }}"
        ));
        let reg = cg.emit_reg(format!("bitcast {{ i8* }}* {g} to i8*"));
        return Value::new(reg, LType::Ptr);
    }
    // Layout word: the leading fnptr is a CODE pointer (never released); each
    // capture marks itself by its slot type ([`crate::meta`]).
    let mut mf = vec![crate::meta::MetaField::PtrOpaque];
    mf.extend(
        caps.iter()
            .map(|c| crate::meta::MetaField::of_slot_ty(&c.val.llvm_ty())),
    );
    let cell = cg.malloc_struct(cell_ty, crate::meta::struct_meta(&mf));
    let fpp = cg.emit_reg(format!(
        "getelementptr {cell_ty}, {cell_ty}* {cell}, i32 0, i32 0"
    ));
    cg.emit(format!("store i8* {fnptr}, i8** {fpp}"));
    for (i, c) in caps.iter().enumerate() {
        let slot = i + 1;
        let lty = c.val.llvm_ty();
        // Each captured pointer is a new reference the cell's drop releases —
        // including the same variable captured by two closures [GC-ARC-PERCEUS].
        crate::arc::dup_store(cg, &lty, &c.val.operand);
        let p = cg.emit_reg(format!(
            "getelementptr {cell_ty}, {cell_ty}* {cell}, i32 0, i32 {slot}"
        ));
        cg.emit(format!("store {lty} {}, {lty}* {p}", c.val.operand));
    }
    let reg = cg.emit_reg(format!("bitcast {cell_ty}* {cell} to i8*"));
    Value::new(reg, LType::Ptr)
}

/// Lift a lambda crossing the C boundary (an FFI callback slot) to an
/// env-free top-level function and return its RAW `i8*` code pointer — C
/// calls it through a plain function-pointer cast, so a closure cell would be
/// jumped into as code. Captures cannot cross that boundary: a capturing
/// lambda is rejected loudly.
pub(crate) fn raw_callback_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    sig: &FnSig,
) -> Result<Value> {
    if !capture_list(cg, parameters, body).is_empty() {
        return Err(CodegenError::unsupported(
            "a capturing lambda as an FFI callback (captures cannot cross the C boundary; use a named function)",
        ));
    }
    let (param_tys, ret_ty, ret_inner) = sig;
    let (ret_spelling, plist) = spelling(sig);
    let name = format!("__callback_{}", cg.next_lambda_id());
    let saved = cg.enter_nested_fn();
    let params = bind_params(cg, parameters, param_tys);
    let emitted = closure_return(cg, body, *ret_ty, *ret_inner);
    cg.exit_nested_fn(saved, &ret_spelling, &name, &params);
    emitted?;
    let reg = cg.emit_reg(format!("bitcast {ret_spelling} ({plist})* @{name} to i8*"));
    Ok(Value::new(reg, LType::Ptr))
}

/// The constant-expression spelling of a closure function's address as `i8*`.
fn fnptr_const(fn_name: &str, sig: &FnSig) -> String {
    let (ret, plist) = spelling_with_env(sig);
    format!("bitcast ({ret} ({plist})* @{fn_name} to i8*)")
}

/// Coerce already-evaluated arguments to the signature's parameter types and
/// render each as a typed operand — the shared front half of every closure
/// call site.
pub(crate) fn coerce_typed_args(
    cg: &mut Codegen,
    sig: &FnSig,
    args: Vec<Value>,
) -> Result<Vec<String>> {
    let mut typed = Vec::with_capacity(args.len());
    for (want, a) in sig.0.iter().zip(args) {
        typed.push(crate::cast::coerce_to(cg, a, *want)?.typed());
    }
    Ok(typed)
}

/// Evaluate argument expressions, coerce them to the signature, and call
/// through the closure handle.
pub(crate) fn cell_call_exprs(
    cg: &mut Codegen,
    handle: &str,
    sig: &FnSig,
    exprs: &[&Expr],
) -> Result<Value> {
    let mut vals = Vec::with_capacity(exprs.len());
    for e in exprs {
        vals.push(gen_expr(cg, e)?);
    }
    let typed = coerce_typed_args(cg, sig, vals)?;
    Ok(cell_call(cg, handle, sig, &typed))
}

/// Call through a closure handle: load the fnptr from the cell and call it with
/// the cell as the leading env argument. `typed_args` are already-coerced
/// `"{ty} {operand}"` renderings in call order.
pub(crate) fn cell_call(
    cg: &mut Codegen,
    handle: &str,
    sig: &FnSig,
    typed_args: &[String],
) -> Value {
    let (ret_spelling, plist) = spelling_with_env(sig);
    let cellp = cg.emit_reg(format!("bitcast i8* {handle} to {{ i8* }}*"));
    let fpp = cg.emit_reg(format!(
        "getelementptr {{ i8* }}, {{ i8* }}* {cellp}, i32 0, i32 0"
    ));
    let fp8 = cg.emit_reg(format!("load i8*, i8** {fpp}"));
    let fp = cg.emit_reg(format!("bitcast i8* {fp8} to {ret_spelling} ({plist})*"));
    let mut args = vec![format!("i8* {handle}")];
    args.extend_from_slice(typed_args);
    let r = cg.emit_reg(format!("call {ret_spelling} {fp}({})", args.join(", ")));
    let v = match sig.2 {
        Some(inner) => Value::result(r, inner),
        None => Value::new(r, sig.1),
    };
    // Closure functions transfer +1 on return (their `ret_as_sig` epilogue).
    crate::arc::own(cg, &v);
    v
}

/// A top-level function used as an Osprey function value: emit (once per
/// module) a forwarder that drops the env argument and tail-calls the real
/// function, plus a constant cell pointing at it.
pub(crate) fn named_fn_cell(cg: &mut Codegen, name: &str) -> Result<Value> {
    if cg.fn_defs.contains_key(name) {
        return Err(CodegenError::unsupported(
            "a generic function as a function value",
        ));
    }
    let cell = match cg.fnval_cells.get(name) {
        Some(g) => g.clone(),
        None => emit_forwarder(cg, name)?,
    };
    let reg = cg.emit_reg(format!("bitcast {{ i8* }}* {cell} to i8*"));
    Ok(Value::new(reg, LType::Ptr))
}

/// Emit `@__fnval_{name}` (env-dropping forwarder) and its constant cell;
/// register and return the cell's global name. The cell exposes the canonical
/// value ABI ([`Codegen::fn_value_sig`]); inside, the real function is called
/// with its own emitted ABI and the result adapted (a math-Result return is
/// unwrapped to its payload).
fn emit_forwarder(cg: &mut Codegen, name: &str) -> Result<String> {
    let sig = named_fn_sig(cg, name)
        .ok_or_else(|| CodegenError::invalid(format!("`{name}` has no resolved signature")))?;
    let real_inner = cg.fn_ret_result_inner(name);
    let real_ret = cg.fn_ret_spelling(name);
    let (ret_spelling, plist) = spelling(&sig);
    if !cg.fn_params.contains_key(name) {
        cg.add_extern(format!("declare {real_ret} @{name}({plist})"));
    }
    let fwd = format!("__fnval_{name}");
    let saved = cg.enter_nested_fn();
    let mut params = vec![(LType::Ptr, String::from("__env"))];
    let mut typed = Vec::new();
    for (i, t) in sig.0.iter().enumerate() {
        let reg = crate::llty::param_register(i);
        typed.push(format!("{t} %{reg}"));
        params.push((*t, reg));
    }
    let r = cg.emit_reg(format!("call {real_ret} @{name}({})", typed.join(", ")));
    let rv = match real_inner {
        Some(inner) => Value::result(r, inner),
        None => Value::new(r, cg.fn_ret_ltype(name).unwrap_or(LType::I64)),
    };
    // The forwarded call's +1 must be owned so `ret_as_sig`'s epilogue
    // transfers exactly one reference out [GC-ARC-PERCEUS].
    crate::arc::own(cg, &rv);
    let emitted = ret_as_sig(cg, rv, sig.1, sig.2);
    cg.exit_nested_fn(saved, &ret_spelling, &fwd, &params);
    emitted?;
    let g = format!("@__fnval_cell_{name}");
    let fnptr = fnptr_const(&fwd, &sig);
    cg.add_global(format!(
        "{g} = private unnamed_addr constant {{ i8* }} {{ i8* {fnptr} }}"
    ));
    let _ = cg.fnval_cells.insert(name.to_string(), g.clone());
    Ok(g)
}

/// The canonical value-ABI [`FnSig`] of a named top-level function.
pub(crate) fn named_fn_sig(cg: &Codegen, name: &str) -> Option<FnSig> {
    let (params, ret) = cg.prog.functions.get(name)?;
    Codegen::fn_value_sig(&osprey_types::Type::fun(params.clone(), ret.clone()))
}

/// The LLVM return-type and parameter-list spellings of a function value's
/// signature (no env): the return is a Result block `{ T, i8 }*` when it
/// returns `Result<T, _>`, else the plain scalar.
pub(crate) fn spelling(sig: &FnSig) -> (String, String) {
    let (param_tys, ret_ty, ret_inner) = sig;
    let ret = crate::llty::ret_spelling(*ret_ty, *ret_inner);
    (ret, crate::llty::comma_join(param_tys, LType::to_string))
}

/// [`spelling`] with the hidden leading `i8*` env parameter — the ABI every
/// closure function and every [`cell_call`] share.
fn spelling_with_env(sig: &FnSig) -> (String, String) {
    let (ret, params) = spelling(sig);
    if params.is_empty() {
        (ret, String::from("i8*"))
    } else {
        (ret, format!("i8*, {params}"))
    }
}

/// Lower a closure body and emit its `ret` matching the signature's return
/// discipline.
fn closure_return(
    cg: &mut Codegen,
    body: &Expr,
    ret_ty: LType,
    ret_inner: Option<LType>,
) -> Result<()> {
    let bv = gen_expr(cg, body)?;
    ret_as_sig(cg, bv, ret_ty, ret_inner)
}

/// Adapt a value to a signature's return discipline and emit the `ret`: a
/// `Result<T, _>` slot wraps a bare value into a Success block (or passes an
/// existing Result through); a scalar slot unwraps and coerces — mirroring
/// `lower::coerce_return` for a named function.
fn ret_as_sig(cg: &mut Codegen, v: Value, ret_ty: LType, ret_inner: Option<LType>) -> Result<()> {
    let rv = match ret_inner {
        Some(_) if v.result_inner.is_some() => v,
        Some(inner) => crate::result::make_ok(cg, v, inner)?,
        None => {
            let u = crate::result::unwrap(cg, v);
            crate::cast::coerce_to(cg, u, ret_ty)?
        }
    };
    // Nested-function epilogue: the return transfers +1, owned locals drop
    // [GC-ARC-PERCEUS].
    crate::arc::epilogue(cg, Some(&rv));
    cg.emit(format!("ret {} {}", rv.llvm_ty(), rv.operand));
    Ok(())
}
