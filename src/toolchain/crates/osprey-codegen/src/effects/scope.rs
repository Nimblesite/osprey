//! Which activation answers: `instance E` capabilities and `mask`. A
//! capability is a runtime identity drawn from `__osprey_instance_new`; an
//! effect form that names one resolves its effect from the checker and passes
//! the identity to the runtime, which answers it only from that identity's
//! handlers. The default identity is `0`. Implements [EFFECTS-INSTANCE] and
//! [EFFECTS-MASK].

use super::{declare_stack, gen_expr, Codegen, CodegenError, Expr, LType, Result, Value};
use osprey_ast::Position;

/// The runtime identity of the default (unnamed) capability.
pub(super) const DEFAULT_INSTANCE: &str = "0";

/// The effect an effect form performs and the identity it answers: the
/// written effect at the default identity, or a capability's effect at the
/// value bound to its name.
pub(super) struct Target {
    pub(super) effect: String,
    pub(super) instance: String,
}

pub(super) fn target(
    cg: &mut Codegen,
    written: &str,
    position: Option<Position>,
) -> Result<Target> {
    let capability = position.and_then(|at| cg.prog.capabilities.get(&(at.line, at.column)));
    let Some(effect) = capability.cloned() else {
        return Ok(Target {
            effect: osprey_ast::effect_name::base(written).to_owned(),
            instance: DEFAULT_INSTANCE.to_owned(),
        });
    };
    let name = osprey_ast::effect_name::base(written).to_owned();
    let value = gen_expr(cg, &Expr::Identifier(name))?;
    if value.ty != LType::I64 {
        return Err(CodegenError::invalid(format!(
            "capability `{written}` is not a capability identity"
        )));
    }
    Ok(Target {
        effect,
        instance: value.operand,
    })
}

/// `instance E`: a fresh identity.
pub(crate) fn gen_instance(cg: &mut Codegen) -> Value {
    let id = cg.call("i64", "__osprey_instance_new", "", &[]);
    Value::new(id, LType::I64)
}

/// `mask E { body }`: hide the activation answering each of E's operations
/// at the target identity while `body` runs, then restore it.
pub(crate) fn gen_mask(
    cg: &mut Codegen,
    effect: &str,
    body: &Expr,
    position: Option<Position>,
) -> Result<Value> {
    declare_stack(cg);
    let target = target(cg, effect, position)?;
    let site = crate::effect_generics::site_handler_ops(cg, position)?;
    let key = crate::effect_generics::runtime_effect_key(&target.effect, &site.effect_args)?;
    let mut operations: Vec<String> = site.ops.keys().cloned().collect();
    operations.sort();
    for operation in &operations {
        let op_id = cg.operation_id(&key, operation)?.to_string();
        let _ = cg.call(
            "i32",
            "__osprey_handler_mask",
            "i32, i64",
            &[&op_id, &target.instance],
        );
    }
    let value = gen_expr(cg, body)?;
    for _ in &operations {
        let _ = cg.emit_reg("call i32 @__osprey_handler_pop()");
    }
    Ok(value)
}

/// Push one handler entry at `instance`. The default identity keeps the
/// identity-free runtime entry points.
pub(super) fn push_handler(cg: &mut Codegen, instance: &str, entry: [&str; 4]) {
    let [op_id, fp, env, activation] = entry;
    if instance == DEFAULT_INSTANCE {
        let _ = cg.emit_reg(format!(
            "call i32 @__osprey_handler_push_scoped(i32 {op_id}, i8* {fp}, i8* {env}, i32 {activation})"
        ));
        return;
    }
    let _ = cg.call(
        "i32",
        "__osprey_handler_push_in",
        "i32, i64, i8*, i8*, i32",
        &[op_id, instance, fp, env, activation],
    );
}

/// `__osprey_handler_<base>(op[, instance])` returning `ret`: the identity-free
/// entry point for the default identity, else its `_in` form.
pub(super) fn lookup(
    cg: &mut Codegen,
    base: &str,
    ret: &str,
    op_id: &str,
    instance: &str,
) -> String {
    if instance == DEFAULT_INSTANCE {
        return cg.emit_reg(format!("call {ret} @__osprey_handler_{base}(i32 {op_id})"));
    }
    cg.call(
        ret,
        &format!("__osprey_handler_{base}_in"),
        "i32, i64",
        &[op_id, instance],
    )
}
