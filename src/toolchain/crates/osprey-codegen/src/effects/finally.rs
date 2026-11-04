//! `finally => cleanup`: a handler activation's cleanup runs exactly once when
//! the activation ends. It is registered on the runtime's per-thread finalizer
//! stack before the activation starts, so normal completion runs it after the
//! return clause and a dropped continuation runs it while unwinding the body
//! thread. Either way it runs outside its own activation and consumes its env.
//! Implements [EFFECTS-FINALIZATION] and [MULTI-COST-ABORT].

use super::{build_env, caps_from_names, free_idents, reload_env, ArmCap, BTreeSet};
use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Position};

/// Lower `activation` with `cleanup` as its finalizer and answer its value.
pub(super) fn guarded(
    cg: &mut Codegen,
    cleanup: &Expr,
    position: Option<Position>,
    activation: impl FnOnce(&mut Codegen) -> Result<Value>,
) -> Result<Value> {
    let mut names = BTreeSet::new();
    free_idents(cleanup, &mut names);
    let caps = caps_from_names(cg, names);
    let (env, env_ty) = build_env(cg, &caps);
    let name = format!("__handler_finally_{}", cg.next_handler_id());
    emit_cleanup_fn(cg, &name, cleanup, position, &caps, &env_ty)?;
    cg.call_void(
        "__osprey_finalizer_push",
        "void (i8*)*, i8*",
        &[&format!("@{name}"), &env],
    );
    let answer = activation(cg)?;
    cg.call_void("__osprey_finalizer_finish", "", &[]);
    Ok(answer)
}

/// `void @name(i8* %__env)`: evaluate the cleanup against its captures, then
/// release the env it was handed — the one owner on either completion path.
fn emit_cleanup_fn(
    cg: &mut Codegen,
    name: &str,
    cleanup: &Expr,
    position: Option<Position>,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    cg.begin_nested_debug(name, position);
    // Cleanup cannot resume the continuation it outlives.
    let outer_resume = cg.resume_ctx.take();
    reload_env(cg, caps, env_ty);
    let body = crate::expr::gen_body(cg, cleanup);
    cg.resume_ctx = outer_resume;
    let _ = body?;
    crate::arc::epilogue(cg, None);
    if !caps.is_empty() {
        crate::arc::release_operand(cg, "%__env");
    }
    cg.emit("ret void");
    cg.exit_nested_fn(saved, "void", name, &[(LType::Ptr, String::from("__env"))]);
    Ok(())
}
