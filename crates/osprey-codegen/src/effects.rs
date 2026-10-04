//! Declared operations, scoped handler activation, and typed requests.
//! Each `handle` arm becomes a top-level handler function; entering the
//! `handle` pushes those functions onto the C runtime's handler stack
//! (`__osprey_handler_push_scoped`, keyed by an interned operation id) and leaving pops
//! them, so a `perform` in any (even forward-referenced) function resolves the
//! innermost active handler dynamically via `__osprey_handler_lookup` and an
//! indirect call. Operation declarations select value substitution or explicit
//! continuation control; arm bodies never determine an operation's mode.

use crate::builder::{CellSlot, Codegen, ParamSig, ResumeCodegenContext};
use crate::cast::coerce_to;
use crate::conv::unbox_from_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::types::{ltype_of, result_inner};
use osprey_ast::freevars::free_idents;
use osprey_ast::{AstNode, Expr, HandlerArm, Stmt};
use osprey_types::ProgramTypes;
use std::collections::{BTreeSet, HashSet};

mod captures;
mod control;
mod drive;
mod requests;
mod return_clause;
use captures::{build_env, capture_list, capture_list_resuming, reload_env, ArmCap};
pub(crate) use captures::{captured_mut_vars, captured_mut_vars_in_stmts};
pub(crate) use control::gen_resume;
use control::{gen_resuming_handler, AnswerShape, DriveArm};
use drive::emit_drive_fn;
pub(crate) use requests::{gen_arithmetic_request, gen_perform};

/// A parsed effect-operation signature: parameter types, the result LLVM type,
/// and (when the result is `Result<T, _>`) the success inner type. A generic
/// effect's type-parameter slots are ERASED — they travel as boxed `i64` and
/// the `*_erased` flags mark which slots must box/unbox at the boundaries.
/// Implements [EFFECTS-GENERIC-RUNTIME].
#[derive(Clone)]
pub(crate) struct OpSig {
    pub params: Vec<ParamSig>,
    pub ret: LType,
    pub ret_result_inner: Option<LType>,
    /// Per-parameter: whether the declared type is an effect type parameter.
    pub param_erased: Vec<bool>,
    /// Whether the declared result is an effect type parameter.
    pub ret_erased: bool,
    /// The operation's DECLARED mode. A control operation's arms are emitted
    /// through the coroutine path that can capture a continuation; a value
    /// operation's arms are plain functions returning the operation's result.
    /// Implements [EFFECTS-HANDLER-ARMS].
    pub mode: osprey_ast::OperationMode,
}

impl OpSig {
    fn word_param() -> ParamSig {
        ParamSig {
            ty: LType::I64,
            result_inner: None,
            fiber: None,
            inferred_type: None,
        }
    }

    pub(crate) fn param(&self, index: usize) -> Result<ParamSig> {
        self.params.get(index).cloned().ok_or_else(|| {
            CodegenError::invalid(format!("operation has no parameter at index {index}"))
        })
    }

    pub(crate) fn param_is_erased(&self, index: usize) -> Result<bool> {
        self.param_erased.get(index).copied().ok_or_else(|| {
            CodegenError::invalid("operation parameter is missing its erasure metadata")
        })
    }

    /// The handler function's LLVM return-type spelling (the Result block
    /// pointer for a Result result, else the plain type).
    fn ret_ty(&self) -> String {
        crate::llty::ret_spelling(self.ret, self.ret_result_inner)
    }

    /// The handler function-pointer type. Every arm takes a hidden leading
    /// `i8* env` (its captured cells + values), e.g. `i64 (i8*, i64)*`.
    fn fn_ptr_ty(&self) -> String {
        let mut parts = vec!["i8*".to_string()];
        parts.extend(self.params.iter().map(|param| param.ty.to_string()));
        format!("{} ({})*", self.ret_ty(), parts.join(", "))
    }
}

/// Emit `ret <ty> <operand>` for `ret` and close out the nested function whose
/// LLVM return type matches `sig`.
fn ret_and_exit(
    cg: &mut Codegen,
    saved: crate::builder::SavedFn,
    sig: &OpSig,
    name: &str,
    params: &[(LType, String)],
    ret: &Value,
) {
    // Nested-function epilogue: the return transfers +1, owned locals drop
    // [GC-ARC-PERCEUS].
    crate::arc::epilogue(cg, Some(ret));
    cg.emit(format!("ret {} {}", ret.llvm_ty(), ret.operand));
    cg.exit_nested_fn(saved, &sig.ret_ty(), name, params);
}

/// Bind each of an arm's operation parameters as an SSA value (`%name`, typed
/// from the checked `sig`) and append it to the emitted `params`
/// list. An erased (generic) slot arrives as a boxed `i64` and is unboxed to
/// the type inference resolved for this handle site. Implements
/// [EFFECTS-GENERIC-RUNTIME].
fn bind_arm_params(
    cg: &mut Codegen,
    arm: &HandlerArm,
    sig: &OpSig,
    resolved: &osprey_types::OpType,
    params: &mut Vec<(LType, String)>,
) -> Result<()> {
    if arm.params.len() != sig.params.len() || resolved.params.len() != sig.params.len() {
        return Err(CodegenError::invalid(
            "handler parameter arity differs from its checked operation",
        ));
    }
    for (i, pname) in arm.params.iter().enumerate() {
        let param = sig.param(i)?;
        let erased = sig.param_is_erased(i)?;
        // The REGISTER is positional and compiler-namespaced, never the
        // source binder: a binder spelled `entry` would collide with the
        // function's `entry:` block label and clang refused the module —
        // the same rule user-function parameters follow ([FLAVOR-IR-EQUIV],
        // [`crate::llty::param_register`]).
        let reg = format!("%__arm{i}");
        let bound = match resolved.params.get(i).filter(|_| erased) {
            Some(rt) => crate::effect_generics::unbox_erased(cg, &reg, rt),
            None => crate::cast::incoming_param(
                cg,
                reg.clone(),
                param.clone(),
                resolved
                    .params
                    .get(i)
                    .and_then(|t| crate::types::owner_name(&cg.prog, t)),
            ),
        };
        cg.emit_debug_param(pname, &bound, params.len());
        cg.bind(pname.clone(), bound);
        params.push((param.ty, format!("__arm{i}")));
    }
    Ok(())
}

/// Build an [`OpSig`] from inference's resolved operation signature — the one
/// source of truth for effect types (no string re-parsing in the backend).
pub(crate) fn op_sig_of(prog: &ProgramTypes, op: &osprey_types::OpType) -> OpSig {
    // A slot mentioning a type parameter ANYWHERE (a bare `T` or a nested
    // `Result<T, string>`) is erased: it travels as one boxed `i64`, boxed
    // and unboxed against each site's resolved instantiation — a nested
    // parameter changes the slot's concrete shape per instantiation just as
    // a top-level one does. Implements [EFFECTS-GENERIC-RUNTIME].
    let ret_erased = osprey_types::has_type_var(&op.ret);
    let inner = if ret_erased {
        None
    } else {
        result_inner(&op.ret)
    };
    let ret = if ret_erased {
        LType::I64
    } else if inner.is_some() {
        LType::Ptr
    } else {
        ltype_of(&op.ret)
    };
    OpSig {
        params: op
            .params
            .iter()
            .map(|t| {
                if osprey_types::has_type_var(t) {
                    OpSig::word_param()
                } else {
                    ParamSig::of(prog, t)
                }
            })
            .collect(),
        ret,
        ret_result_inner: inner,
        param_erased: op.params.iter().map(osprey_types::has_type_var).collect(),
        ret_erased,
        mode: op.mode,
    }
}

/// Operation types and modes must come from the checked declaration.
fn op_sig_for(cg: &Codegen, effect: &str, operation: &str) -> Result<OpSig> {
    let key = format!("{effect}.{operation}");
    cg.prog
        .effects
        .get(effect)
        .and_then(|operations| operations.get(operation))
        .map(|operation| op_sig_of(&cg.prog, operation))
        .ok_or_else(|| CodegenError::invalid(format!("missing checked effect operation `{key}`")))
}

fn checked_arm_op<'a>(
    site: &'a osprey_types::HandlerSite,
    arm: &HandlerArm,
) -> Result<&'a osprey_types::OpType> {
    site.ops.get(&arm.operation).ok_or_else(|| {
        CodegenError::invalid(format!(
            "handler arm `{}` has no checked operation",
            arm.operation
        ))
    })
}

fn declare_stack(cg: &mut Codegen) {
    cg.add_extern("declare i32 @__osprey_handler_depth()");
    cg.add_extern("declare i32 @__osprey_handler_push_scoped(i32, i8*, i8*, i32)");
    cg.add_extern("declare i32 @__osprey_handler_pop()");
    cg.add_extern("declare i8* @__osprey_handler_lookup(i32)");
    cg.add_extern("declare i8* @__osprey_handler_lookup_env(i32)");
    cg.add_extern("declare i8* @__osprey_handler_suspend_scope(i32)");
    cg.add_extern("declare void @__osprey_handler_restore_scope(i8*)");
}

/// Branch on a null handler pointer: print `unhandled effect: <key>.<op>` and
/// exit, so a missed lookup fails loudly instead of calling null. Implements
/// [EFFECTS-GENERIC-RUNTIME].
fn emit_unhandled_guard(cg: &mut Codegen, raw: &str, lookup_key: &str, operation: &str) {
    cg.add_extern("declare i32 @puts(i8*)");
    cg.add_extern("declare void @exit(i32)");
    let msg = cg.string_constant(&format!("unhandled effect: {lookup_key}.{operation}"));
    let is_null = cg.emit_reg(format!("icmp eq i8* {raw}, null"));
    let abort_lbl = cg.fresh_label();
    let ok_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {is_null}, label %{abort_lbl}, label %{ok_lbl}"
    ));
    cg.start_block(&abort_lbl);
    let _ = cg.emit_reg(format!("call i32 @puts(i8* {})", msg.operand));
    cg.emit("call void @exit(i32 1)");
    cg.emit("unreachable");
    cg.start_block(&ok_lbl);
}

fn declare_coro(cg: &mut Codegen) {
    cg.add_extern("declare i8* @__osprey_coro_new(i8*)");
    cg.add_extern("declare void @__osprey_coro_start(i8*, i64 (i8*)*, i8*, i8*)");
    cg.add_extern("declare i64 @__osprey_coro_suspend(i8*, i64, i64*, i8*, i64)");
    cg.add_extern("declare i64 @__osprey_coro_resume(i8*, i64)");
    cg.add_extern("declare i64 @__osprey_coro_done(i8*)");
    cg.add_extern("declare i8* @__osprey_coro_take_args(i8*)");
    cg.add_extern("declare i64 @__osprey_coro_mail_op(i8*)");
    cg.add_extern("declare i64 @__osprey_coro_mail_arg(i8*, i64)");
    cg.add_extern("declare void @__osprey_coro_mail_free(i8*)");
    cg.add_extern("declare i64 @__osprey_coro_result(i8*)");
    cg.add_extern("declare void @__osprey_coro_abort(i8*)");
    cg.add_extern("declare void @__osprey_coro_free(i8*)");
    cg.add_extern("declare i8* @__osprey_handler_snapshot()");
}

/// Install a handler activation: capture its environment (the cells
/// and values its arms reference), emit a handler function per arm bound to that
/// env, push them on the runtime stack for the duration of `body`, then pop.
pub(crate) fn gen_handler(
    cg: &mut Codegen,
    effect: &str,
    arms: &[HandlerArm],
    body: &Expr,
    return_clause: Option<&Expr>,
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    declare_stack(cg);
    let effect = osprey_ast::effect_name::base(effect);
    let site_ops = crate::effect_generics::site_handler_ops(cg, position)?;
    // The DECLARATION decides which lowering a region needs: any control
    // operation among its arms means a continuation may be captured.
    // Implements [EFFECTS-HANDLER-ARMS].
    if arms
        .iter()
        .map(|arm| op_sig_for(cg, effect, &arm.operation))
        .collect::<Result<Vec<_>>>()?
        .iter()
        .any(|sig| sig.mode.is_control())
    {
        return gen_resuming_handler(cg, effect, arms, body, return_clause, &site_ops);
    }
    // A generic effect's handler registers under its instantiation-mangled
    // key, so only same-instantiation performs resolve to it. Implements
    // [EFFECTS-GENERIC-RUNTIME].
    let key = crate::effect_generics::runtime_effect_key(effect, &site_ops.effect_args)?;
    let caps = capture_list(cg, arms);
    let (env, env_ty) = build_env(cg, &caps);
    let activation = cg.call("i32", "__osprey_handler_depth", "", &[]);
    for arm in arms {
        let sig = op_sig_for(cg, effect, &arm.operation)?;
        let resolved = checked_arm_op(&site_ops, arm)?;
        let id = cg.next_handler_id();
        let fn_name = format!("__handler_{effect}_{}_{id}", arm.operation);
        emit_handler_fn(cg, &fn_name, arm, &sig, resolved, &caps, &env_ty)?;
        let op_id = cg.operation_id(&key, &arm.operation)?;
        let fp = cg.emit_reg(format!("bitcast {} @{fn_name} to i8*", sig.fn_ptr_ty()));
        let _ = cg.emit_reg(format!(
            "call i32 @__osprey_handler_push_scoped(i32 {op_id}, i8* {fp}, i8* {env}, i32 {activation})"
        ));
    }

    let result = gen_expr(cg, body)?;

    for _ in arms {
        let _ = cg.emit_reg("call i32 @__osprey_handler_pop()");
    }
    // The popped region's env reached its structural end: drop it (its mask
    // releases the captured values) [GC-ARC-PERCEUS].
    if env != "null" {
        crate::arc::release_operand(cg, &env);
    }
    return_clause::apply(cg, return_clause, result)
}

/// Emit a top-level handler function for one arm: a hidden leading `i8* %__env`
/// it reloads its captures from, then the operation's own parameters; its body
/// is the arm body coerced to the operation's result.
fn emit_handler_fn(
    cg: &mut Codegen,
    name: &str,
    arm: &HandlerArm,
    sig: &OpSig,
    resolved: &osprey_types::OpType,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    cg.begin_nested_debug(name, arm.position);
    let mut params = vec![(LType::Ptr, String::from("__env"))];
    reload_env(cg, caps, env_ty);
    bind_arm_params(cg, arm, sig, resolved, &mut params)?;
    let body = crate::expr::gen_body(cg, &arm.body)?;
    let ret = if sig.ret_erased {
        // Adapt before retaining: a plain body can become a freshly allocated
        // Success block, and that actual return value must survive the epilogue.
        let adapted = crate::effect_generics::adapt_erased(cg, body, &resolved.ret)?;
        crate::arc::escape_retain(cg, &adapted);
        // The perform site unboxes it to its resolved type. Implements
        // [EFFECTS-GENERIC-RUNTIME] [GC-ARC-PERCEUS].
        crate::effect_generics::box_raw_value(cg, adapted)
    } else if let Some(inner) = sig.ret_result_inner {
        if body.result_inner.is_some() {
            crate::result::repack_to_inner(cg, body, inner)?
        } else {
            crate::result::make_ok(cg, body, inner)?
        }
    } else {
        coerce_to(cg, body, sig.ret)?
    };
    ret_and_exit(cg, saved, sig, name, &params, &ret);
    Ok(())
}

/// Coerce a value into an operation's RESULT slot: what `resume(v)` supplies,
/// and what a substituting arm's value becomes. Preserves a complete Result,
/// safely promotes a plain value to `Success` when that is the declared slot,
/// and rejects the forbidden `Result -> plain` direction.
fn coerce_to_op_result(
    cg: &mut Codegen,
    value: Value,
    ret_ty: LType,
    result_inner: Option<LType>,
) -> Result<Value> {
    match result_inner {
        Some(inner) => promote_to_result(cg, value, inner),
        None => coerce_to(cg, value, ret_ty),
    }
}

/// Carry `value` into a `Result<inner, _>` answer slot: repack one that is
/// already a Result, else apply the language's safe `T -> Success(T)`
/// promotion. Never erases a discriminant.
fn promote_to_result(cg: &mut Codegen, value: Value, inner: LType) -> Result<Value> {
    if value.result_inner.is_some() {
        crate::result::repack_to_inner(cg, value, inner)
    } else {
        crate::result::make_ok(cg, value, inner)
    }
}

pub(crate) fn box_codegen_value(cg: &mut Codegen, value: Value) -> Value {
    // Every effect-boundary boxing erases pointer-ness from the ARC drop
    // walk: dup so the unboxing side owns +1 [GC-ARC-PERCEUS].
    crate::arc::escape_retain(cg, &value);
    crate::effect_generics::box_raw_value(cg, value)
}

pub(crate) fn unbox_coro_value(
    cg: &mut Codegen,
    raw: &str,
    ty: LType,
    result_inner: Option<LType>,
) -> Value {
    if let Some(inner) = result_inner {
        let ptr = cg.emit_reg(format!("inttoptr i64 {raw} to i8*"));
        let struct_ty = crate::llty::result_struct_ty(inner);
        let typed = cg.emit_reg(format!("bitcast i8* {ptr} to {struct_ty}*"));
        return Value::result(typed, inner);
    }
    unbox_from_i64(cg, raw, ty)
}
