//! Continuation handler construction and answer transport.
use super::{
    bind_arm_params, box_codegen_value, build_env, capture_list_resuming, checked_arm_op,
    coerce_to, coerce_to_op_result, declare_coro, declare_stack, emit_drive_fn, gen_expr, ltype_of,
    op_sig_for, promote_to_result, reload_env, result_inner, ret_and_exit, return_clause,
    unbox_coro_value, ArmCap, Codegen, CodegenError, Expr, HandlerArm, LType, OpSig, Result,
    ResumeCodegenContext, Value,
};

#[derive(Clone)]
pub(super) struct DriveArm {
    pub(super) op_id: usize,
    pub(super) operation: String,
    pub(super) sig: OpSig,
    pub(super) arm_fn: String,
}

/// The complete runtime shape of the handled expression's answer. Resuming
/// handlers cross an erased `i64` mailbox, so Result and aggregate metadata
/// must travel beside the LLVM scalar type instead of being discarded.
#[derive(Clone)]
pub(super) struct AnswerShape {
    pub(super) ty: LType,
    pub(super) inferred_type: Option<osprey_types::Type>,
    pub(super) owner: Option<String>,
    pub(super) result_inner: Option<LType>,
    pub(super) payload_owner: Option<String>,
}

impl AnswerShape {
    pub(super) fn of(value: &Value) -> Self {
        Self {
            ty: value.ty,
            inferred_type: value.inferred_type.clone(),
            owner: value.osp_ty.clone(),
            result_inner: value.result_inner,
            payload_owner: value.payload_owner.clone(),
        }
    }

    pub(super) fn restore(&self, cg: &mut Codegen, raw: &str) -> Value {
        let mut value =
            unbox_coro_value(cg, raw, self.ty, self.result_inner).with_owner(self.owner.clone());
        value.payload_owner.clone_from(&self.payload_owner);
        value.inferred_type.clone_from(&self.inferred_type);
        value
    }
}

/// Coerce an arm answer without ever erasing a Result. The only
/// representation-changing coercion allowed here is the language's safe
/// `T -> Success(T)` promotion when the handled expression itself is Result.
pub(super) fn coerce_to_answer(
    cg: &mut Codegen,
    value: Value,
    answer: &AnswerShape,
) -> Result<Value> {
    match answer.result_inner {
        Some(inner) => promote_to_result(cg, value, inner),
        // An arm that does not `resume` abandons the continuation, so ITS value
        // becomes the whole `handle` block's result. Whether it CAN be that
        // result is settled in inference, where the semantic types still exist
        // (`check_abandoning_arm` in `crates/osprey-types/src/expr.rs`).
        //
        // A codegen-side guard cannot decide it: whether an arm's answer may
        // BE the region's result is a question about the SOURCE types, which
        // only inference still has. When the region's answer type is `any`,
        // `coerce_to` erases the arm's value into its shape-carrying box
        // ([`crate::anybox`], [TYPE-ANY]) — that boxing is also where the
        // answer's ownership transfer happens; the plain-scalar directions
        // carry no ownership, exactly as `coerce_return` in `lower.rs`.
        None => coerce_to(cg, value, answer.ty),
    }
}

/// A region implementing declared control operations: the handled body runs
/// on a body thread and each `perform` suspends into this host-side dispatcher.
pub(super) fn gen_resuming_handler(
    cg: &mut Codegen,
    effect: &str,
    arms: &[HandlerArm],
    body: &Expr,
    return_clause: Option<&Expr>,
    site_ops: &osprey_types::HandlerSite,
) -> Result<Value> {
    declare_stack(cg);
    declare_coro(cg);
    // Same instantiation-mangled runtime key as the non-resuming path.
    // Implements [EFFECTS-GENERIC-RUNTIME].
    let key = crate::effect_generics::runtime_effect_key(effect, &site_ops.effect_args)?;

    let caps = capture_list_resuming(cg, arms, body, return_clause);
    let (env, env_ty) = build_env(cg, &caps);
    let id = cg.next_handler_id();
    let body_fn = format!("__resume_body_{effect}_{id}");
    let drive_fn = format!("__resume_drive_{effect}_{id}");

    let body_answer = emit_resuming_body_fn(cg, &body_fn, body, &caps, &env_ty)?;
    let return_fn = return_clause.map(|_| format!("__handler_return_{effect}_{id}"));
    let answer = match (return_clause, return_fn.as_deref()) {
        (Some(clause), Some(name)) => {
            return_clause::emit(cg, name, clause, &body_answer, &caps, &env_ty)?
        }
        _ => body_answer,
    };
    let mut drive_arms = Vec::new();
    for (op_id, arm) in arms.iter().enumerate() {
        let sig = op_sig_for(cg, effect, &arm.operation)?;
        let resolved = checked_arm_op(site_ops, arm)?;
        let suspend_fn = format!("__resume_suspend_{effect}_{}_{id}_{op_id}", arm.operation);
        let arm_fn = format!("__resume_arm_{effect}_{}_{id}_{op_id}", arm.operation);
        emit_suspend_fn(cg, &suspend_fn, op_id, &sig, resolved)?;
        emit_resuming_arm_fn(
            cg,
            arm,
            &ArmFnSpec {
                name: &arm_fn,
                drive_fn: &drive_fn,
                answer: &answer,
                sig: &sig,
                resolved,
                caps: &caps,
                env_ty: &env_ty,
            },
        )?;
        drive_arms.push(DriveArm {
            op_id,
            operation: arm.operation.clone(),
            sig,
            arm_fn,
        });
    }
    emit_drive_fn(cg, &drive_fn, &drive_arms, return_fn.as_deref())?;

    let coro = cg.call("i8*", "__osprey_coro_new", "i8*", &[&env]);
    let activation = cg.call("i32", "__osprey_handler_depth", "", &[]);
    for arm in &drive_arms {
        let suspend_fn = format!(
            "__resume_suspend_{effect}_{}_{id}_{}",
            arm.operation, arm.op_id
        );
        let op_id = cg.operation_id(&key, &arm.operation)?.to_string();
        let fp = cg.emit_reg(format!(
            "bitcast {} @{suspend_fn} to i8*",
            arm.sig.fn_ptr_ty()
        ));
        let _ = cg.call(
            "i32",
            "__osprey_handler_push_scoped",
            "i32, i8*, i8*, i32",
            &[&op_id, &fp, &coro, &activation],
        );
    }

    let snap = cg.call("i8*", "__osprey_handler_snapshot", "", &[]);
    // The body owns a deep snapshot. Arms execute in the enclosing scope.
    // Implements [EFFECTS-RESUME-NESTING].
    for _ in arms {
        let _ = cg.call("i32", "__osprey_handler_pop", "", &[]);
    }
    cg.call_void(
        "__osprey_coro_start",
        "i8*, i64 (i8*)*, i8*, i8*",
        &[&coro, &format!("@{body_fn}"), &env, &snap],
    );
    let boxed = cg.emit_reg(format!("call i64 @{drive_fn}(i8* {env}, i8* {coro})"));

    cg.call_void("__osprey_coro_free", "i8*", &[&coro]);
    // The coro region ended: drop its env (mask releases the captures)
    // [GC-ARC-PERCEUS].
    if env != "null" {
        crate::arc::release_operand(cg, &env);
    }
    let out = answer.restore(cg, &boxed);
    // The body fn escape-retained its answer at the boxing site: own it here.
    crate::arc::own(cg, &out);
    Ok(out)
}

pub(super) fn emit_resuming_body_fn(
    cg: &mut Codegen,
    name: &str,
    body: &Expr,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<AnswerShape> {
    let saved = cg.enter_nested_fn();
    reload_env(cg, caps, env_ty);
    let body = crate::expr::gen_body(cg, body)?;
    let answer = AnswerShape::of(&body);
    let boxed = box_codegen_value(cg, body);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", boxed.operand));
    cg.exit_nested_fn(saved, "i64", name, &[(LType::Ptr, String::from("__env"))]);
    Ok(answer)
}

pub(super) fn emit_suspend_fn(
    cg: &mut Codegen,
    name: &str,
    op_id: usize,
    sig: &OpSig,
    resolved: &osprey_types::OpType,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    let mut params = vec![(LType::Ptr, String::from("__coro"))];
    for (i, param) in sig.params.iter().cloned().enumerate() {
        params.push((param.ty, format!("__arg{i}")));
    }

    let (args_ptr, kinds_ptr) = if sig.params.is_empty() {
        (String::from("null"), String::from("null"))
    } else {
        crate::effect_mailbox::emit_mailbox_arrays(cg, sig, resolved)?
    };
    let raw = cg.call(
        "i64",
        "__osprey_coro_suspend",
        "i8*, i64, i64*, i8*, i64",
        &[
            "%__coro",
            &op_id.to_string(),
            &args_ptr,
            &kinds_ptr,
            &sig.params.len().to_string(),
        ],
    );
    let ret = unbox_coro_value(cg, &raw, sig.ret, sig.ret_result_inner);
    // `resume(v)` DUPs v into the coro's result mailbox (box_codegen_value),
    // so the value arrives here owned: register it, or the performer's side of
    // every resuming operation leaks it. [GC-ARC-PERCEUS].
    crate::arc::own(cg, &ret);
    ret_and_exit(cg, saved, sig, name, &params, &ret);
    Ok(())
}

pub(super) struct ArmFnSpec<'a> {
    name: &'a str,
    drive_fn: &'a str,
    answer: &'a AnswerShape,
    sig: &'a OpSig,
    resolved: &'a osprey_types::OpType,
    caps: &'a [ArmCap],
    env_ty: &'a str,
}

pub(super) fn emit_resuming_arm_fn(
    cg: &mut Codegen,
    arm: &HandlerArm,
    spec: &ArmFnSpec<'_>,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    cg.begin_nested_debug(spec.name, arm.position);
    reload_env(cg, spec.caps, spec.env_ty);
    let mut params = vec![
        (LType::Ptr, String::from("__env")),
        (LType::Ptr, String::from("__coro")),
    ];
    bind_arm_params(cg, arm, spec.sig, spec.resolved, &mut params)?;
    let op_ret_ty = ltype_of(&spec.resolved.ret);
    let op_ret_result_inner = result_inner(&spec.resolved.ret);
    cg.resume_ctx = spec.sig.mode.is_control().then(|| ResumeCodegenContext {
        env: String::from("%__env"),
        coro: String::from("%__coro"),
        drive_fn: spec.drive_fn.to_string(),
        answer_ty: spec.answer.ty,
        answer_result_inner: spec.answer.result_inner,
        answer_owner: spec.answer.owner.clone(),
        answer_payload_owner: spec.answer.payload_owner.clone(),
        answer_inferred_type: spec.answer.inferred_type.clone(),
        op_ret_ty,
        op_ret_result_inner,
    });
    let body_raw = crate::expr::gen_body(cg, &arm.body)?;
    // Control arms answer the region even when they never invoke resume.
    // Value arms supply the operation result. Implements [EFFECTS-HANDLER-ARMS].
    let body = if spec.sig.mode.is_control() {
        coerce_to_answer(cg, body_raw, spec.answer)?
    } else {
        coerce_to_op_result(cg, body_raw, op_ret_ty, op_ret_result_inner)?
    };
    let boxed = box_codegen_value(cg, body);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", boxed.operand));
    cg.exit_nested_fn(saved, "i64", spec.name, &params);
    Ok(())
}

pub(crate) fn gen_resume(cg: &mut Codegen, value: Option<&Expr>) -> Result<Value> {
    declare_coro(cg);
    let Some(ctx) = cg.resume_ctx.clone() else {
        return Err(CodegenError::invalid("`resume` outside a handler arm"));
    };
    let raw_value = match value {
        Some(expr) => gen_expr(cg, expr)?,
        None => Value::unit(),
    };
    let raw_value = coerce_to_op_result(cg, raw_value, ctx.op_ret_ty, ctx.op_ret_result_inner)?;
    let boxed_value = box_codegen_value(cg, raw_value);
    let _ = cg.call(
        "i64",
        "__osprey_coro_resume",
        "i8*, i64",
        &[&ctx.coro, &boxed_value.operand],
    );
    let raw = cg.emit_reg(format!(
        "call i64 @{}(i8* {}, i8* {})",
        ctx.drive_fn, ctx.env, ctx.coro
    ));
    let mut answer = unbox_coro_value(cg, &raw, ctx.answer_ty, ctx.answer_result_inner)
        .with_owner(ctx.answer_owner);
    answer.payload_owner = ctx.answer_payload_owner;
    answer.inferred_type = ctx.answer_inferred_type;
    // Dispatch returns an escape-retained transformed or control answer. Registering
    // it is what balances the retain the enclosing arm adds when it boxes this
    // value as its own return — without it every managed continuation answer
    // survived to exit. [GC-ARC-PERCEUS]
    crate::arc::own(cg, &answer);
    Ok(answer)
}
