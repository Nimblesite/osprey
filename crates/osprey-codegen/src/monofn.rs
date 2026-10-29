//! Shared instantiations of recursive generic functions and closed first-order
//! calls that exceed [`crate::genfn`]'s inline expansion budget.
//!
//! Each call selects concrete argument types. Recursive bodies cannot expand
//! indefinitely, and repeated generic fanout must not duplicate whole subtrees.
//!
//! Here the call site's argument types fix ONE instantiation, so that
//! instantiation is emitted as a real function `@name$N` with a concrete
//! signature and called. While its body is being emitted the instantiation is
//! already registered, so the self-call inside it resolves to the SAME symbol
//! and lowers to a direct recursive call. Two call sites with different
//! argument types get two definitions; two with the same types share one.
//!
//! Implements [GPU-KERNEL-FORM] (docs/specs/runtime/0034-GPUComputation.md): a
//! recursive helper reached from a kernel — a row walk over a flat matrix is
//! the canonical one — is a valid kernel, so it must compile without the
//! annotations Osprey's inference otherwise makes redundant. Also
//! [TYPE-FN-GENERIC]; the function-value contract is [TYPE-GENERICS-FN] in
//! `docs/specs/language/0004-TypeSystem.md` (plan 0002 retired).

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
/// cannot specialise. Mutual recursion re-enters [`crate::genfn`] mid-inline
/// and is specialised the same way there. The free-identifier collector
/// already answers "which names does this body reach", parameters subtracted
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
    specialize_values(cg, name, parameters, body, values)
}

/// Reuse an instantiation after the caller has normalized its argument values.
pub(crate) fn specialize_values(
    cg: &mut Codegen,
    name: &str,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
) -> Result<Value> {
    let key = format!(
        "{}{}",
        instantiation_key(name, &values),
        declared_spelling(cg, name)
    );
    let known = cg.monofns.get(&key).cloned();
    let target = match known {
        Some(existing) => existing,
        None => emit(cg, name, key, parameters, body, &values)?,
    };
    call(cg, &target, values)
}

/// The instantiation this call selects: the function name plus each argument's
/// lowered representation and inferred parameter types consumed by the emitted ABI.
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
    let inferred = v
        .inferred_type
        .as_ref()
        .map_or_else(String::new, crate::types::abi_spelling);
    format!("{}:{owner}:{result}:{inferred}", v.ty.as_str())
}

/// The callee's declared signature as the CURRENT call's specialization sees
/// it. A boxed argument value carries no element type (`Tree<int>` and
/// `Tree<string>` are both a bare handle), so the value spellings alone would
/// share one emitted body whose first-emission owner tags then type the
/// second call's payloads wrong. The checker resolves this per call site, so
/// its view of the signature is the distinguishing spelling (#278).
fn declared_spelling(cg: &Codegen, name: &str) -> String {
    let params = cg
        .prog
        .param_types(name)
        .unwrap_or(&[])
        .iter()
        .map(crate::types::abi_spelling)
        .collect::<Vec<_>>()
        .join(",");
    let ret = cg
        .prog
        .return_type(name)
        .map_or_else(String::new, crate::types::abi_spelling);
    format!("[{params}]->{ret}")
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
/// with its `Result` inner type and OWNER tag, and the owner tag each parameter
/// carries in. The return owner rides along so a specialisation handing back a
/// tagged collection stays element-typed at its call site
/// ([BUILTIN-LIST-GET]).
type Abi<'a> = (
    &'a [ParamSig],
    (LType, Option<LType>, Option<String>),
    &'a [Option<String>],
);

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
        sig: (params.to_vec(), ret.0, ret.1, None, ret.2.clone()),
    };
    let _ = cg.monofns.insert(key, target.clone());
    let saved = cg.enter_nested_fn();
    let position = cg.fn_defs.get(name).and_then(|(_, _, position)| *position);
    let (plist, emitted) = cg.within_source_function(name, |cg| {
        crate::closure::begin_source(cg, name, position);
        let plist = crate::cast::bind_params(cg, parameters, params, owners.iter().cloned(), 0);
        crate::closure::source_parameters(cg, parameters, 0);
        // Inside the body, so the definition line is credited when the
        // instantiation RUNS rather than when it is emitted or handed to a C
        // callback slot — the same placement `gen_function` uses for a
        // monomorphic definition [TESTING-COVERAGE-CODEGEN].
        cg.cov_hit_inline_fn(name);
        (plist, lower_body(cg, body, &target.sig))
    });
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
    let Some((parameters, body, _)) = cg.fn_defs.get(name).cloned() else {
        return Ok(None);
    };
    let (param_types, ret_type) = declared;
    if parameters.len() != param_types.len() {
        return Ok(None);
    }
    let params: Vec<ParamSig> = param_types
        .iter()
        .map(|t| ParamSig::of(&cg.prog, t))
        .collect();
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
        return_owner(cg, ret_type),
    );
    let target = emit_at(cg, name, key, &parameters, &body, (&params, ret, &owners))?;
    Ok(Some(target.symbol))
}

/// The instantiation's return slot, from the signature inference resolved for
/// the function. A hole anywhere the value representation travels has no slot to
/// emit, so the annotation diagnostic still stands for that case.
///
/// A hole in a `Result`'s ERROR channel is not one of those places: the channel
/// is phantom — the `Error` variant stores a string message and the slot is
/// `i8*` whatever `E` is ([`crate::result`]) — so inference leaves it open and
/// [`abi_shape_concrete`] skips it. Asking the whole type instead rejected
/// `-> Result<Term, _>`, the hole [TYPE-RENDER-HOLES] calls the honest spelling,
/// and told the author to annotate a signature that changes nothing emitted.
fn return_slot(cg: &Codegen, name: &str) -> Result<(LType, Option<LType>, Option<String>)> {
    let ty = cg.prog.return_type(name).ok_or_else(|| unresolved(name))?;
    if !crate::types::abi_shape_concrete(ty) {
        return Err(unresolved(name));
    }
    Ok((
        crate::types::ltype_of(ty),
        crate::types::result_inner(ty),
        return_owner(cg, ty),
    ))
}

/// Share the closure ABI's Result payload and ordinary return ownership rules.
fn return_owner(cg: &Codegen, ty: &osprey_types::Type) -> Option<String> {
    let signature = osprey_types::Type::fun(Vec::new(), ty.clone());
    Codegen::fn_value_sig(&cg.prog, &signature).and_then(|sig| sig.4)
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
        inferred_type: v.inferred_type.clone(),
    }
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
    crate::closure::lower_fn_body(cg, body, Some(sig))
}

/// Call one instantiation with the argument values the call site lowered.
fn call(cg: &mut Codegen, target: &Instantiation, args: Vec<Value>) -> Result<Value> {
    crate::closure::call_symbol(cg, &target.symbol, &target.sig, &[], args)
}

#[cfg(test)]
mod tests {
    #[test]
    fn nested_generic_fanout_has_bounded_ir_growth() {
        let helpers = crate::genfn::tests::fanout_helpers();
        let source = format!("fn leaf(x) = x\n{helpers}print(l8(\"z\"))\n");
        let parsed = osprey_syntax::parse_program_for_path("fanout.osp", &source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let ir = crate::compile_program(&parsed.program).expect("generic fanout must compile");
        assert!(
            ir.lines().count() < 1000,
            "generic fanout expanded to {} IR lines",
            ir.lines().count()
        );
    }

    #[test]
    fn expansion_budget_preserves_stateful_handle_parameter_metadata() {
        let helpers = crate::genfn::tests::fanout_helpers();
        let source = format!("fn leaf(x) = x\n{helpers}fn greet() = \"done\"\nfn waitIt(f) = await(f)\nfn takeIt(ch) = recv(ch)\nlet warm = l8(\"z\")\nlet ch = Channel(1)\nsend(ch, \"message\")\nprint(\"${{warm}}${{waitIt(spawn greet())}}${{takeIt(ch)}}\")\n");
        let parsed = osprey_syntax::parse_program_for_path("fiber-fanout.osp", &source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let ir = crate::compile_program(&parsed.program)
            .expect("fiber metadata must survive the expansion budget");
        crate::testkit::shows(&ir, &["fiber_await", "channel_recv", "inttoptr i64"]);
    }

    #[test]
    fn instantiation_keys_distinguish_inferred_parameter_abis() {
        let mut integer = crate::llty::Value::new("%argument", crate::llty::LType::Ptr);
        integer.inferred_type = Some(osprey_types::Type::con(
            "Packet",
            vec![osprey_types::Type::con(osprey_types::names::INT, vec![])],
        ));
        let mut text = integer.clone();
        text.inferred_type = Some(osprey_types::Type::con(
            "Packet",
            vec![osprey_types::Type::con(osprey_types::names::STRING, vec![])],
        ));
        assert_ne!(super::param_of(&integer), super::param_of(&text));
        assert_ne!(
            super::slot_spelling(&integer),
            super::slot_spelling(&text),
            "distinct parameter ABIs must not share an instantiation"
        );
    }

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
            module
                .matches("define internal i8* @handler$mono0(")
                .count(),
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
        crate::testkit::contents(&module, &[], &["@handler("]);

        // The ABI is the builtin's declared callback type — four strings in,
        // one response pointer out — because that is the one signature the C
        // runtime will ever call it with.
        assert!(
            module.contains(
                "define internal i8* @handler$mono0(i8* %$p0, i8* %$p1, i8* %$p2, i8* %$p3)"
            ),
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
        crate::testkit::shows(&module, &["@__osp_cov_hits.1 = internal global i64 0"]);
    }
}
