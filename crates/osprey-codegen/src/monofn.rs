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
/// ([`osprey_ast::freevars`]).
pub(crate) fn calls_itself(name: &str, body: &Expr) -> bool {
    let mut names = std::collections::BTreeSet::new();
    osprey_ast::freevars::free_idents(body, &mut names);
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
    // Inside the body, so the definition line is credited when the
    // instantiation RUNS rather than when it is emitted or handed to a C
    // callback slot — the same placement `gen_function` uses for a monomorphic
    // definition [TESTING-COVERAGE-CODEGEN].
    cg.cov_hit_inline_fn(name);
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
    let owners: Vec<Option<String>> = param_types
        .iter()
        .map(|t| crate::types::owner_name(&cg.prog, t))
        .collect();
    let ret = (
        crate::types::ltype_of(ret_type),
        crate::types::result_inner(ret_type),
    );
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
    // Cell promotion is per lowered body, and `enter_nested_fn` cleared the
    // host's set: without repopulating it here a `mut` this body declares and a
    // handler arm captures is never promoted, so the arm writes a private copy
    // and the read after the `handle` sees the initial value
    // ([EFFECTS-HANDLER-STATE]).
    cg.cell_vars = crate::effects::captured_mut_vars(body);
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

#[cfg(test)]
mod tests {
    /// An HTTP handler whose types are inferred — the shape that reaches
    /// [`specialize_callback`]. Its definition is line 1, so line 1's counter
    /// is the one that must move only when the handler runs.
    const INFERRED_HANDLER: &str = r#"fn handler(method, path, headers, body) = HttpResponse {
    status: 200, headers: "", contentType: "text/plain",
    streamFd: 0, isComplete: true, partialBody: "ok"
}
let server = httpCreateServer(18201, "127.0.0.1")
let listening = httpListen(server, handler)
let second = httpCreateServer(18202, "127.0.0.1")
let alsoListening = httpListen(second, handler)
print("${listening}${alsoListening}")
"#;

    /// The instructions of `define`d function `symbol`, up to its closing brace.
    fn body_of<'a>(module: &'a str, symbol: &str) -> &'a str {
        let opener = format!("@{symbol}(");
        let Some(start) = module.find(&opener) else {
            panic!("`@{symbol}` is not defined by the module:\n{module}")
        };
        let rest = &module[start..];
        match rest.find("\n}") {
            Some(end) => &rest[..end],
            None => rest,
        }
    }

    /// Whether `body` bumps the counter for `line` — matched on the whole
    /// suffix so `.1` never answers for `.10`.
    fn bumps(body: &str, line: usize) -> bool {
        body.lines()
            .any(|l| l.trim_end().ends_with(&format!("@__osp_cov_hits.{line}")))
    }

    /// How many times `symbol` appears in `@symbol` position.
    fn mentions(module: &str, symbol: &str) -> usize {
        module.matches(&format!("@{symbol}")).count()
    }

    #[test]
    fn registering_an_inferred_callback_emits_one_complete_instantiation() {
        let parsed = osprey_syntax::parse_program_for_path("handler.osp", INFERRED_HANDLER);
        assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
        let module = crate::compile_program_coverage(&parsed.program).expect("coverage lowering");

        // DEFINED, once. A generic function has no `@name` symbol of its own —
        // it exists only as a body inlined per Osprey call site — and a callback
        // has no Osprey call site, so codegen emitted the reference and never
        // the body while still returning `Ok`.
        assert_eq!(
            module.matches("define i8* @handler$mono0(").count(),
            1,
            "exactly one definition of the instantiation:\n{module}"
        );

        // REFERENCED once per registration, and only through the
        // instantiation. A surviving bare `@handler(` would be the dangling
        // generic stem clang rejects. The program registers the SAME inferred
        // handler twice, so the cache must serve the second site rather than
        // emit a rival `$mono1` — one definition, two addresses taken.
        assert_eq!(
            mentions(&module, "handler$mono0"),
            3,
            "defined once and taken by address once per registration:\n{module}"
        );
        assert!(
            !module.contains("handler$mono1"),
            "a second registration must reuse the instantiation, not emit \
             another:\n{module}"
        );
        assert!(
            !module.contains("@handler("),
            "the generic stem must never be referenced:\n{module}"
        );

        // The ABI is the builtin's declared callback type — four strings in,
        // one response pointer out — because that is the one signature the C
        // runtime will ever call it with.
        assert!(
            module.contains("define i8* @handler$mono0(i8* %$p0, i8* %$p1, i8* %$p2, i8* %$p3)"),
            "the instantiation is emitted at the declared callback ABI:\n{module}"
        );
        // Once per registration, at that same type. `contains` would pass with
        // ONE correctly typed cast and a second at some other signature, which
        // is exactly the mismatch a C callback slot cannot survive.
        assert_eq!(
            module
                .matches("bitcast i8* (i8*, i8*, i8*, i8*)* @handler$mono0 to i8*")
                .count(),
            2,
            "every registration hands the runtime a pointer cast at the \
             declared ABI:\n{module}"
        );

        // [TESTING-COVERAGE-CODEGEN]: "the definition line counts as covered
        // when the body executes" — `gen_function` emits that bump INSIDE the
        // body it is lowering, and a specialised callback is a real function
        // too. It was emitted before `emit_at`, so it landed in whatever
        // function was open — `main`, at the `httpListen` registration call.
        // The handler then read as covered on a server that never served a
        // single request, and its emitted body carried no counter at all. A
        // coverage report that credits code nobody ran is the silent-wrong
        // class: it looks like evidence and is not.
        assert!(
            bumps(body_of(&module, "handler$mono0"), 1),
            "the emitted callback body must bump its own definition line:\n{module}"
        );
        assert!(
            !bumps(body_of(&module, "main"), 1),
            "registering a callback must not credit its body with having run:\n{module}"
        );

        // And the counter still EXISTS — a bump that moved nowhere would pass
        // both assertions above.
        assert!(
            module.contains("@__osp_cov_hits.1 = internal global i64 0"),
            "line 1 must still be a registered coverable line:\n{module}"
        );
    }
}
