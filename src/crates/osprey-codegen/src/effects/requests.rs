//! Typed operation requests and arithmetic dispatch.
use super::scope::lookup;
use super::{
    declare_stack, emit_unhandled_guard, gen_expr, op_sig_for, Codegen, CodegenError, Expr, OpSig,
    Result, Value,
};

/// `perform Effect.op(args)` — look up the active handler and call it. An
/// erased (generic) slot boxes its argument and unboxes its result against
/// the signature inference resolved for this site. Implements
/// [EFFECTS-GENERIC-RUNTIME].
pub(crate) fn gen_perform(
    cg: &mut Codegen,
    effect: &str,
    operation: &str,
    args: &[Expr],
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    declare_stack(cg);
    let target = super::scope::target(cg, effect, position)?;
    let effect = target.effect.as_str();
    let sig = op_sig_for(cg, effect, operation)?;
    let site = crate::effect_generics::site_perform_op(cg, position)?;
    // Look up the handler under the instantiation-mangled key, so a
    // mismatched instantiation misses (a loud unhandled-effect abort) rather
    // than reaching a handler of the wrong type. Implements
    // [EFFECTS-GENERIC-RUNTIME].
    let lookup_key = crate::effect_generics::runtime_effect_key(effect, &site.effect_args)?;
    if args.len() != sig.params.len() || site.op.params.len() != sig.params.len() {
        return Err(CodegenError::invalid(
            "perform argument arity differs from its checked operation",
        ));
    }

    // Evaluate + coerce arguments to the operation's parameter types.
    let mut typed = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let v = gen_expr(cg, a)?;
        let v = if sig.param_is_erased(i)? {
            crate::effect_generics::box_erased(
                cg,
                v,
                site.op
                    .params
                    .get(i)
                    .ok_or_else(|| CodegenError::invalid("missing checked operation parameter"))?,
            )?
        } else {
            crate::cast::coerce_param(cg, v, &sig.param(i)?)?
        };
        typed.push(v.typed());
    }

    let request = Request {
        lookup_key: &lookup_key,
        operation,
        instance: &target.instance,
    };
    dispatch_request(cg, &request, &sig, &site.op.ret, typed)
}

/// What one request asks for: the operation of an effect instantiation,
/// answered at a capability identity ([EFFECTS-INSTANCE]).
pub(super) struct Request<'a> {
    lookup_key: &'a str,
    operation: &'a str,
    instance: &'a str,
}

/// Implicit arithmetic requests use the same value-operation ABI as `perform`.
/// Implements [ARITH-EFFECT-OPS] on native, wasm and inherited fiber handlers.
pub(crate) fn gen_arithmetic_request(
    cg: &mut Codegen,
    operation: &str,
    args: &[Value],
) -> Result<Value> {
    let name = ensure_arithmetic_fault(cg, operation, args)?;
    let sig = op_sig_for(cg, osprey_ast::ARITH_EFFECT, operation)?;
    let typed = args.iter().map(Value::typed).collect::<Vec<_>>();
    let r = cg.emit_reg(format!(
        "call {} @{name}({})",
        sig.ret_ty(),
        typed.join(", ")
    ));
    Ok(Value::new(r, sig.ret))
}

/// A checked operation's slow path lives in one `cold` function per operation and
/// operand shape, so the hot function stays small enough to inline and unroll.
fn ensure_arithmetic_fault(cg: &mut Codegen, operation: &str, args: &[Value]) -> Result<String> {
    let shape = args
        .iter()
        .map(|arg| arg.ty.as_str().replace('*', "p"))
        .collect::<Vec<_>>()
        .join(".");
    let name = format!("osp.arith.fault.{operation}.{shape}");
    if !cg.arithmetic_faults.insert(name.clone()) {
        return Ok(name);
    }
    declare_stack(cg);
    let sig = op_sig_for(cg, osprey_ast::ARITH_EFFECT, operation)?;
    let ret = cg
        .prog
        .effects
        .get(osprey_ast::ARITH_EFFECT)
        .and_then(|ops| ops.get(operation))
        .map(|op| op.ret.clone())
        .ok_or_else(|| CodegenError::invalid("missing Arith signature"))?;
    let request = Request {
        lookup_key: osprey_ast::ARITH_EFFECT,
        operation,
        instance: super::scope::DEFAULT_INSTANCE,
    };
    let params = args
        .iter()
        .enumerate()
        .map(|(index, arg)| (arg.ty, format!("a{index}")))
        .collect::<Vec<_>>();
    let saved = cg.enter_nested_fn();
    let typed = params
        .iter()
        .map(|(ty, param)| format!("{ty} %{param}"))
        .collect();
    let out = dispatch_request(cg, &request, &sig, &ret, typed)?;
    cg.emit(format!("ret {} {}", sig.ret_ty(), out.operand));
    cg.exit_nested_fn(saved, &sig.ret_ty(), &name, &params);
    cg.mark_last_function_cold();
    Ok(name)
}

pub(super) fn dispatch_request(
    cg: &mut Codegen,
    request: &Request<'_>,
    sig: &OpSig,
    ret: &osprey_types::Type,
    typed: Vec<String>,
) -> Result<Value> {
    let instance = request.instance;
    let op_id = cg
        .operation_id(request.lookup_key, request.operation)?
        .to_string();
    let raw = lookup(cg, "lookup", "i8*", &op_id, instance);
    // A missed lookup returns null — abort with a message instead of calling
    // a null pointer (an instantiation mismatch on a generic effect misses by
    // design, [EFFECTS-GENERIC-RUNTIME]).
    emit_unhandled_guard(cg, &raw, request.lookup_key, request.operation);
    let env = lookup(cg, "lookup_env", "i8*", &op_id, instance);
    let fp = cg.emit_reg(format!("bitcast i8* {raw} to {}", sig.fn_ptr_ty()));
    let ret_ty = sig.ret_ty();
    let r = cg.fresh_reg();
    let mut call_args = vec![format!("i8* {env}")];
    call_args.extend(typed);
    let scope =
        (!sig.mode.is_control()).then(|| lookup(cg, "suspend_scope", "i8*", &op_id, instance));
    cg.emit(format!(
        "{r} = call {ret_ty} {fp}({})",
        call_args.join(", ")
    ));
    if let Some(scope) = scope {
        cg.call_void("__osprey_handler_restore_scope", "i8*", &[&scope]);
    }
    if sig.ret_erased {
        let v = crate::effect_generics::unbox_erased(cg, &r, ret);
        crate::arc::own(cg, &v);
        return Ok(v);
    }
    let out = match sig.ret_result_inner {
        Some(inner) => Value::result(r, inner),
        None => Value::new(r, sig.ret),
    };
    // The handler fn's epilogue transferred +1 [GC-ARC-PERCEUS].
    crate::arc::own(cg, &out);
    Ok(out)
}
