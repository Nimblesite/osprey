//! Expression arguments lowering.
use super::{
    as_double, as_i64, checked_intrinsic, fn_pointer, gen_checked_arith, gen_expr, gen_print,
    to_string_value, with_application, with_lambda_captures, Codegen, CodegenError, Expr, FnSig,
    LType, NamedArgument, Result, Value,
};

/// A call to a user-defined or runtime function. Parameter types come from
/// inference (so a string/float/bool parameter is passed in its real LLVM
/// type), as does the return type.
pub(super) fn gen_user_call(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    let args = ordered_args(cg, name, arguments, named)?;
    call_with_values(cg, name, args)
}

pub(super) fn is_binary_integer_builtin(name: &str) -> bool {
    matches!(
        name,
        "intDiv"
            | "checkedAdd"
            | "checkedSub"
            | "checkedMul"
            | "wrapAdd"
            | "wrapSub"
            | "wrapMul"
            | "satAdd"
            | "satSub"
            | "satMul"
    )
}

pub(super) fn binary_integer_builtin(
    cg: &mut Codegen,
    name: &str,
    left: Value,
    right: Value,
) -> Result<Value> {
    match name {
        "intDiv" => crate::arithmetic::int_division(cg, &left, &right),
        "checkedAdd" | "checkedSub" | "checkedMul" => {
            gen_checked_arith(cg, checked_intrinsic(name), left, right)
        }
        _ => crate::arithmetic::total(cg, name, left, right),
    }
}

/// Lower a builtin that was passed as a first-class callback — `forEach(xs,
/// print)`, `gpuMap(toFloat)`. Each arm has a value form needing no argument
/// expressions, so it lowers once per element. `None` means `name` has no such
/// form. Implements [BUILTIN-ITER-CALLBACK].
fn call_builtin_with_values(cg: &mut Codegen, name: &str, args: &[Value]) -> Option<Result<Value>> {
    let arg = || args.first().cloned().unwrap_or_else(Value::unit);
    Some(match name {
        "print" => gen_print(cg, arg()),
        "toString" => to_string_value(cg, arg()),
        // [BUILTIN-TOFLOAT] [GPU-CONVERT] the canonical float-pipeline seed
        // `gpuIota(n) |> gpuMap(toFloat)` lowers through this arm.
        "toFloat" => as_i64(cg, arg()).and_then(|n| as_double(cg, n)),
        "abs" => crate::arithmetic::absolute(cg, arg()),
        name if is_binary_integer_builtin(name) => match args {
            [left, right] => binary_integer_builtin(cg, name, left.clone(), right.clone()),
            _ => Err(CodegenError::invalid(format!("{name} needs two arguments"))),
        },
        _ => return None,
    })
}

/// Call `name` with already-evaluated argument values — the shared tail of
/// `gen_user_call` and the iterator callbacks. Coerces each argument to the
/// inferred parameter type, declares unknown (runtime) callees, and tags a
/// `Result`-returning callee's value.
pub(crate) fn call_with_values(cg: &mut Codegen, name: &str, args: Vec<Value>) -> Result<Value> {
    // An intrinsic builtin has no emitted `@name` symbol, so one reaching this
    // path as a first-class callback lowers to its value form here.
    if let Some(v) = call_builtin_with_values(cg, name, &args) {
        return v;
    }
    // Coerce each argument to the declared parameter type where known.
    // A parameter slot is typed, not tagged: a list literal handed to a real
    // (non-inlined) callee arrives as `List<T>` [`crate::listlit::escaping`].
    let args: Vec<Value> = args
        .into_iter()
        .map(|a| crate::listlit::escaping(cg, a))
        .collect();
    let coerced = match cg.fn_param_abis(name) {
        Some(ptys) if ptys.len() == args.len() => args
            .into_iter()
            .zip(ptys)
            .map(|(a, want)| crate::cast::coerce_param(cg, a, &want))
            .collect::<Result<Vec<_>>>()?,
        _ => args,
    };
    let typed = crate::llty::comma_join(&coerced, Value::typed);
    // A function declared `-> Result<T, E>` hands back a Result block pointer.
    if let Some(inner) = cg.fn_ret_result_inner(name) {
        let rty = format!("{}*", crate::llty::RESULT_STRUCT);
        let reg = emit_user_call(cg, name, &rty, &coerced, &typed);
        // [MODULES-ABI]: unwrapping a returned record must retain its field layout.
        let owner = cg
            .prog
            .return_type(name)
            .and_then(|ty| crate::types::result_payload_owner(&cg.prog, ty));
        let v = Value::result(reg, inner).with_payload_owner(owner);
        // Callee epilogues transfer +1 on every return [GC-ARC-PERCEUS].
        crate::arc::own(cg, &v);
        return Ok(v);
    }
    let ret = cg.fn_ret_ltype(name).unwrap_or(LType::I64);
    let reg = emit_user_call(cg, name, ret.as_str(), &coerced, &typed);
    let v = Value::new(reg, ret).with_owner(cg.fn_ret_owner(name));
    let v = match cg.fn_ret_fiber_sig(name) {
        Some(fiber) => fiber.restore(v),
        None => v,
    };
    crate::arc::own(cg, &v);
    Ok(v)
}

/// Emit a call to `name` returning LLVM type `rty`. A name with no user
/// definition is a runtime builtin, so synthesize its `declare` (param types
/// from `coerced`) — the IR stays valid and links only if the symbol exists.
fn emit_user_call(
    cg: &mut Codegen,
    name: &str,
    rty: &str,
    coerced: &[Value],
    typed: &str,
) -> String {
    if !cg.fn_params.contains_key(name) {
        let sig = crate::llty::comma_join(coerced, Value::llvm_ty);
        cg.add_extern(format!("declare {rty} @{name}({sig})"));
    }
    cg.emit_reg(format!("call {rty} @{name}({typed})"))
}

fn ordered_args(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Vec<Value>> {
    // The function-value signature of each declared parameter (if it is
    // function-typed), so an inline-lambda argument is lowered to that slot's
    // ABI rather than evaluated as a value. An EXTERN callee crosses the C
    // boundary: its function-typed slots take raw code pointers, not cells.
    let sigs: Vec<Option<FnSig>> = cg
        .prog
        .param_types(name)
        .map(|ts| {
            ts.iter()
                .map(|t| Codegen::fn_value_sig(&cg.prog, t))
                .collect()
        })
        .unwrap_or_default();
    let ffi = !cg.fn_params.contains_key(name) && cg.prog.functions.contains_key(name);
    if !named.is_empty() {
        if let Some(pnames) = cg
            .fn_params
            .get(name)
            .or_else(|| cg.extern_params.get(name))
            .cloned()
        {
            let mut out = Vec::new();
            for (i, pn) in pnames.iter().enumerate() {
                if let Some(argument) = arguments
                    .get(i)
                    .or_else(|| named.iter().find(|a| &a.name == pn).map(|a| &a.value))
                {
                    out.push(eval_arg(
                        cg,
                        argument,
                        sigs.get(i).and_then(Option::as_ref),
                        ffi,
                    )?);
                }
            }
            if out.len() == arguments.len() + named.len() {
                return Ok(out);
            }
        }
        return arguments
            .iter()
            .chain(named.iter().map(|na| &na.value))
            .map(|argument| gen_expr(cg, argument))
            .collect();
    }
    arguments
        .iter()
        .enumerate()
        .map(|(i, a)| eval_arg(cg, a, sigs.get(i).and_then(Option::as_ref), ffi))
        .collect()
}

/// Lower one call argument. A lambda flowing into a function-typed parameter
/// becomes a closure cell with the slot's ABI — except across the C boundary
/// (`ffi`), where the slot needs a raw code pointer: there a non-capturing
/// lambda lifts env-free and a named function takes its raw address.
/// Everything else goes through `gen_expr` (where a user function name becomes
/// its forwarder cell).
pub(super) fn eval_arg(
    cg: &mut Codegen,
    expr: &Expr,
    sig: Option<&FnSig>,
    ffi: bool,
) -> Result<Value> {
    if let Expr::TypeApply {
        function, position, ..
    } = expr
    {
        return with_application(cg, *position, |cg| eval_arg(cg, function, sig, ffi));
    }
    match (expr, sig) {
        (
            Expr::Lambda {
                parameters,
                body,
                position,
                ..
            },
            Some(sig),
        ) => {
            if ffi {
                crate::closure::raw_callback_lambda(cg, parameters, body, sig, *position, None)
            } else {
                crate::closure::emit_closure(cg, parameters, body, sig, *position)
            }
        }
        (Expr::Identifier(n), Some(sig)) if cg.lookup(n).is_none() => {
            // Resolve a call alias (`let g = identity`) to its real target.
            let target = cg.call_aliases.get(n).cloned().unwrap_or_else(|| n.clone());
            if let Some((params, body, position)) = cg.lambda_def(&target).cloned() {
                return with_lambda_captures(cg, &target, |cg| {
                    if ffi {
                        crate::closure::raw_callback_lambda(cg, &params, &body, sig, position, None)
                    } else {
                        crate::closure::emit_closure(cg, &params, &body, sig, position)
                    }
                });
            }
            // A GENERIC named function flowing into a concrete function-typed
            // slot: specialise it to the slot's ABI — its (params, body) emit
            // exactly like a capture-free lambda. A monomorphic name keeps its
            // once-per-module forwarder cell via `gen_expr`/`named_fn_cell`.
            // Implements [TYPE-GENERICS-FN].
            if let Some((params, body, position)) = cg.fn_defs.get(&target).cloned() {
                return cg.with_file_scope(|cg| {
                    if ffi {
                        crate::closure::raw_callback_lambda(
                            cg,
                            &params,
                            &body,
                            sig,
                            position,
                            Some(&target),
                        )
                    } else {
                        // Keyed by (function, slot ABI): every use at the same ABI
                        // lowers to a byte-identical body, so emit it once and
                        // share the cell. Distinct ABIs still get distinct bodies —
                        // that is what specialising means.
                        let key = crate::closure::specialisation_key(&target, sig);
                        crate::closure::emit_closure_keyed(
                            cg,
                            &params,
                            &body,
                            sig,
                            Some(key),
                            position,
                            Some(&target),
                        )
                    }
                });
            }
            if ffi && cg.fn_params.contains_key(&target) {
                return Ok(fn_pointer(cg, &target));
            }
            let v = gen_expr(cg, expr)?;
            Ok(list_arg_as_handle(cg, v, ffi))
        }
        _ => {
            let v = gen_expr(cg, expr)?;
            Ok(list_arg_as_handle(cg, v, ffi))
        }
    }
}

/// Normalise a flat list-literal argument into an `OspreyList` handle before it
/// crosses into a callee (a no-op for every other value).
///
/// A callee cannot tell which list layout it was handed: its parameter is one
/// `i8*` either way, and inside the body the value carries the `List` owner tag
/// regardless of how the caller spelled it. `osprey_list_length` happens to read
/// both layouts — they share a leading `i64` — but `osprey_list_get` and
/// `osprey_list_drop` need the real trie, so a list pattern over a literal
/// argument **segfaulted**:
///
/// ```text
/// fn headOf(xs: List<int>) -> int = match xs { [] => -1  [h, ...t] => h }
/// print("${headOf([7, 8])}")            // SIGSEGV
/// print("${headOf(listAppend(List(), 7))}")   // fine
/// ```
///
/// Rebuilding at the boundary makes a parameter's representation independent of
/// the caller's spelling. Nothing regresses: a `List<T>` parameter cannot be
/// indexed in the first place (`xs[0]` on a parameter is
/// `index of a non-list/map value` for both spellings), and every `list*`
/// builtin, the receiver-directed `length`/`isEmpty`, and `+` all accept a
/// handle. An `extern` callee is skipped — a C signature taking a list is
/// outside this contract, so its argument is left exactly as written.
fn list_arg_as_handle(cg: &mut Codegen, v: Value, ffi: bool) -> Value {
    if ffi {
        return v;
    }
    crate::listlit::to_runtime_list(cg, v)
}
