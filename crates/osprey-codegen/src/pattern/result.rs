//! Result pattern lowering.
use super::{bind_catch_all, bind_value, emit_arm, finish_phi};
use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};
use osprey_ast::{MatchArm, Pattern};

pub(super) fn is_result_arm(p: &Pattern) -> bool {
    result_variant(p).is_some()
}

fn result_variant(pattern: &Pattern) -> Option<&str> {
    match pattern {
        Pattern::Constructor { name, .. } | Pattern::Binding(name)
            if name == "Success" || name == "Error" =>
        {
            Some(name)
        }
        _ => None,
    }
}

/// Result match. A struct-pointer Result (the uniform runtime ABI) branches on
/// its `i8` discriminant (`== 0` ⇒ Success) and binds the success arm's field to
/// the loaded payload; a bare scalar discriminant falls back to `disc >= 0`
/// (always Success), preserving the scalar's own type for the binding.
pub(super) fn gen_result_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let success = result_arm(arms, "Success");
    let error = result_arm(arms, "Error");

    // (cond, success-binding, error-binding) by Result shape.
    let (cond, succ_val, err_val) = if disc.result_inner.is_some() {
        let d = crate::result::load_disc(cg, disc);
        let c = cg.emit_reg(format!("icmp eq i8 {d}, 0"));
        // Success binds the value slot; Error binds the errmsg slot (the real
        // reason), so `Error { message }` sees the message regardless of the
        // success payload type. Implements [ERR-PAYLOAD].
        let bound = (
            c,
            crate::result::load_value(cg, disc),
            crate::result::load_errmsg_str(cg, disc),
        );
        // Both slots are now in registers, so a freshly produced block is dead
        // here. Retiring it at the match — rather than letting the region-end
        // drop do it — keeps the release off the path after the arms, which is
        // what allows a self-call in an arm to stay in tail position.
        // `consume_fresh` only fires for a pure-scalar block, whose errmsg is
        // rodata and so outlives the release. [GC-ARC-PERCEUS]
        if arms.iter().all(|arm| is_result_arm(&arm.pattern)) {
            crate::arc::consume_fresh(cg, disc);
        }
        bound
    } else if matches!(disc.ty, LType::Str | LType::Ptr) {
        // A handle discriminant (e.g. a WHERE-constrained constructor that
        // currently always succeeds) has no numeric tag — take the Success arm
        // and bind the handle itself.
        let empty = Value::new(cg.string_constant("").operand, LType::Str);
        ("true".to_string(), disc.clone(), empty)
    } else {
        // A scalar that is NOT a `Result` is matched under the auto-wrap rule:
        // "any value may be matched as if wrapped in `Success`"
        // ([`crate::pattern`] mirror of the checker's rule in
        // osprey-types/src/pattern.rs). So the Success arm is taken
        // UNCONDITIONALLY, exactly as the handle branch above does.
        //
        // This used to branch on `icmp sge i64 value, 0`, treating a negative
        // scalar as an Error — a negative-sentinel heuristic no builtin relies
        // on. It made `-1 ?: 99` evaluate to `99` and `abs(-1)` see `0`: a
        // SILENT wrong answer on every negative value reaching `?:`.
        // Implements [PATTERN-RESULT-AUTOWRAP].
        let empty = Value::new(cg.string_constant("").operand, LType::Str);
        ("true".to_string(), disc.clone(), empty)
    };

    let (sl, el, end) = cg.diamond(&cond);

    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    emit_result_arm(cg, &sl, success, succ_val, disc, &mut phi_in)?;
    emit_result_arm(cg, &el, error, err_val, disc, &mut phi_in)?;

    finish_phi(cg, &phi_in, &end, mark)
}

/// Select the first matching variant or catch-all, preserving source order.
fn result_arm<'a>(arms: &'a [MatchArm], variant: &str) -> Option<&'a MatchArm> {
    arms.iter().find(|arm| match result_variant(&arm.pattern) {
        Some(name) => name == variant,
        None => matches!(arm.pattern, Pattern::Wildcard | Pattern::Binding(_)),
    })
}

/// Bind a variant's payload or a catch-all's complete discriminant and emit
/// its value. A missing arm is unreachable for a checked exhaustive Result;
/// it cannot contribute an empty predecessor to the result phi.
fn emit_result_arm(
    cg: &mut Codegen,
    label: &str,
    arm: Option<&MatchArm>,
    bound: Value,
    disc: &Value,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    cg.start_block(label);
    if let Some(arm) = arm {
        emit_arm(cg, arm, phi_in, |cg| {
            if let Pattern::Constructor { fields, .. } = &arm.pattern {
                if let Some(f) = fields.first() {
                    bind_value(cg, f.clone(), bound);
                }
            } else if !is_result_arm(&arm.pattern) {
                bind_catch_all(cg, &arm.pattern, disc);
            }
        })?;
    } else {
        cg.emit("unreachable");
    }
    Ok(())
}
