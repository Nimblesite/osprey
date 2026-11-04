//! Result pattern lowering.
use super::{bind_catch_all, bind_value, emit_arm, finish_phi, match_mark};
use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm, Pattern};

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

    let mark = match_mark(cg);
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

/// `match listGet(xs, i) { … }` without the Result: branch on the bounds check
/// itself, read the element only on the Success path, and bind the constant
/// reason on the Error one. The Result block the call would allocate exists
/// only to be taken apart by these arms, so a lookup costs no allocation --
/// and a scan over a list is a loop of lookups [BUILTIN-LIST-GET].
///
/// `None` when the scrutinee is not a builtin `listGet` call, or when an arm
/// binds the whole Result and so needs it to exist.
pub(super) fn gen_list_get_match(
    cg: &mut Codegen,
    value: &Expr,
    arms: &[MatchArm],
) -> Result<Option<Value>> {
    let Expr::Call {
        function,
        arguments,
        named_arguments,
    } = value
    else {
        return Ok(None);
    };
    let mut callee = function.as_ref();
    let mut applications = Vec::new();
    while let Expr::TypeApply {
        function, position, ..
    } = callee
    {
        applications.push(*position);
        callee = function;
    }
    let builtin = matches!(callee, Expr::Identifier(name) if name == "listGet")
        && named_arguments.is_empty()
        && arguments.len() == 2
        && cg.lookup("listGet").is_none()
        && cg.lambda_def("listGet").is_none();
    let fusible = arms
        .iter()
        .all(|arm| is_result_arm(&arm.pattern) || matches!(arm.pattern, Pattern::Wildcard));
    if !builtin || !fusible {
        return Ok(None);
    }
    within(cg, callee, &applications, &mut |cg| {
        fused_list_get(cg, arguments, arms)
    })
    .map(Some)
}

/// Run `generate` inside each generic application the callee carried,
/// outermost first, exactly as an ordinary call would be generated.
fn within(
    cg: &mut Codegen,
    callee: &Expr,
    applications: &[Option<osprey_ast::Position>],
    generate: &mut dyn FnMut(&mut Codegen) -> Result<Value>,
) -> Result<Value> {
    match applications.split_first() {
        Some((position, rest)) => crate::expr::with_application(cg, callee, *position, |cg| {
            within(cg, callee, rest, generate)
        }),
        None => generate(cg),
    }
}

fn fused_list_get(cg: &mut Codegen, arguments: &[Expr], arms: &[MatchArm]) -> Result<Value> {
    let (list, index) = crate::collections::list_get_operands(cg, arguments)?;
    let operands = [list.operand.as_str(), index.operand.as_str()];
    let in_bounds = cg.call("i32", "osprey_list_in_bounds", "i8*, i64", &operands);
    let cond = cg.emit_reg(format!("icmp ne i32 {in_bounds}, 0"));
    let (sl, el, end) = cg.diamond(&cond);
    let mark = match_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    cg.start_block(&sl);
    let raw = cg.call("i64", "osprey_list_get", "i8*, i64", &operands);
    let element = crate::collections::elem_value(cg, &list, &raw);
    emit_result_body(cg, result_arm(arms, "Success"), element, None, &mut phi_in)?;
    cg.start_block(&el);
    let reason = cg.string_constant(crate::collections::INDEX_OOB).operand;
    let reason = Value::new(reason, LType::Str);
    emit_result_body(cg, result_arm(arms, "Error"), reason, None, &mut phi_in)?;
    finish_phi(cg, &phi_in, &end, mark)
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
    emit_result_body(cg, arm, bound, Some(disc), phi_in)
}

/// The arm's bindings and body in the current block. A catch-all binds the
/// whole discriminant, which a fused match does not have (`disc` is `None`)
/// and so never offers to one.
fn emit_result_body(
    cg: &mut Codegen,
    arm: Option<&MatchArm>,
    bound: Value,
    disc: Option<&Value>,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    let Some(arm) = arm else {
        cg.emit("unreachable");
        return Ok(());
    };
    emit_arm(cg, arm, phi_in, |cg| {
        if let Pattern::Constructor { fields, .. } = &arm.pattern {
            if let Some(f) = fields.first() {
                bind_value(cg, f.clone(), bound);
            }
        } else if let (false, Some(disc)) = (is_result_arm(&arm.pattern), disc) {
            bind_catch_all(cg, &arm.pattern, disc);
        }
    })
}
