//! `match` lowering. Three shapes, dispatched on the arm patterns:
//!   * literal arms (bool/int/float/string) + catch-all — a compare/branch chain;
//!   * `Success`/`Error` arms — Result discrimination (a scrutinee that is not
//!     a `Result` takes the Success arm unconditionally, per the auto-wrap
//!     rule: any value may be matched as if wrapped in `Success`);
//!   * user-union variant arms — tag comparison against the heap block's leading
//!     discriminant, binding the variant's fields.

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::Value;
use osprey_ast::{Expr, MatchArm, Pattern};

mod list;
mod literal;
mod nested;
mod phi;
mod result;
mod structural;
#[cfg(test)]
mod tests;
mod union;
use list::gen_list_match;
use literal::gen_literal_match;
use phi::finish_phi;
use result::{gen_result_match, is_result_arm};
use structural::gen_structural_match;
use union::{gen_union_match, union_owner};

/// Whether any arm's pattern satisfies `shape`.
fn any_arm(arms: &[MatchArm], shape: impl Fn(&Pattern) -> bool) -> bool {
    arms.iter().any(|arm| shape(&arm.pattern))
}

/// Whether `pattern` catches every value the arms before it missed.
fn is_catch_all(pattern: &Pattern) -> bool {
    matches!(
        pattern,
        Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. }
    )
}

/// End a pattern chain: a catch-all binds the whole scrutinee and stops the
/// chain, and every other shape is one this chain cannot lower.
fn close_chain(
    cg: &mut Codegen,
    pattern: &Pattern,
    arm: &MatchArm,
    disc: &Value,
    phi_in: &mut Vec<(Value, String)>,
    unsupported: &str,
) -> Result<()> {
    if !is_catch_all(pattern) {
        return Err(CodegenError::unsupported(unsupported));
    }
    take_catch_all(cg, arm, disc, phi_in)
}

pub(crate) fn gen_match(cg: &mut Codegen, value: &Expr, arms: &[MatchArm]) -> Result<Value> {
    if std::env::var("OSPREY_DBG_MATCH").is_ok() {
        eprintln!(
            "DBG gen_match in {:?}: scrutinee {:?}  arms {:?}",
            cg.source_function, value, arms
        );
    }
    if let Some(fused) = result::gen_list_get_match(cg, value, arms)? {
        return Ok(fused);
    }
    let disc = gen_expr(cg, value)?;
    if arms.iter().any(|a| is_result_arm(&a.pattern))
        && (disc.result_inner.is_some()
            || union_owner(cg, arms).is_none_or(|owner| owner == "Result"))
    {
        return gen_result_match(cg, &disc, arms);
    }
    if any_arm(arms, |p| matches!(p, Pattern::List { .. })) {
        return gen_list_match(cg, &disc, arms);
    }
    if any_arm(arms, |p| matches!(p, Pattern::Structural { .. })) {
        return gen_structural_match(cg, &disc, arms);
    }
    if let Some(owner) = union_owner(cg, arms) {
        return gen_union_match(cg, &disc, arms, &owner);
    }
    gen_literal_match(cg, &disc, arms)
}

/// Evaluate a matched arm's body, then branch to a deferred exit block.  The
/// exit is emitted only after every arm has revealed its physical value shape,
/// allowing [`finish_phi`] to re-layout placeholder Error Results before they
/// meet at the closing `phi`.
fn push_arm(cg: &mut Codegen, body: &Expr, phi_in: &mut Vec<(Value, String)>) -> Result<()> {
    // A phi is an ESCAPE: the merged value leaves the block whose static
    // knowledge produced it, and a SIBLING arm may have produced the other list
    // representation. `match … { Success { value } => value  Error { … } =>
    // [777] }` merged a runtime list with a flat literal, and the joined value
    // — whose owners disagree, so it carries none — was then read as whichever
    // one the receiver's type predicted, dereferencing a `{ length, data }`
    // header as an `OspreyList` ([`crate::listlit::escaping`]).
    let raw = crate::expr::gen_body(cg, body)?;
    let v = crate::listlit::escaping(cg, raw);
    let exit = cg.fresh_label();
    cg.emit(format!("br label %{exit}"));
    phi_in.push((v, exit));
    Ok(())
}

/// Allocate the join state for a match chain: the end label, the phi inputs,
/// the last-arm index, and the arc frame mark taken BEFORE any arm runs (the
/// [`crate::arc::move_phi_owners`] scrutinee gate).
fn match_state(
    cg: &mut Codegen,
    arms: &[MatchArm],
) -> (String, Vec<(Value, String)>, usize, usize) {
    let mark = match_mark(cg);
    (
        cg.fresh_label(),
        Vec::new(),
        arms.len().saturating_sub(1),
        mark,
    )
}

/// Stores in an arm cannot consume owners live before the match: a sibling
/// may return another value without performing that ownership transfer.
fn match_mark(cg: &mut Codegen) -> usize {
    let mark = crate::arc::frame_mark(cg);
    crate::arc::open_conditional(cg, mark);
    mark
}

/// The join state of a `select`, opened before its arms run.
pub(crate) fn select_state(cg: &mut Codegen) -> (String, Vec<(Value, String)>, usize) {
    let (end, phi_in, _, mark) = match_state(cg, &[]);
    (end, phi_in, mark)
}

/// One `select` arm, joined like a match arm.
pub(crate) fn select_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    bind: impl FnOnce(&mut Codegen) -> Result<()>,
) -> Result<()> {
    try_emit_arm(cg, arm, phi_in, bind)
}

/// Close a `select`'s join.
pub(crate) fn select_finish(
    cg: &mut Codegen,
    phi_in: &[(Value, String)],
    end: &str,
    mark: usize,
) -> Result<Value> {
    finish_phi(cg, phi_in, end, mark)
}

/// Generate a successful match arm and branch to the common result block.
fn emit_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    bind: impl FnOnce(&mut Codegen),
) -> Result<()> {
    try_emit_arm(cg, arm, phi_in, |cg| {
        bind(cg);
        Ok(())
    })
}

fn try_emit_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    bind: impl FnOnce(&mut Codegen) -> Result<()>,
) -> Result<()> {
    let position = match &arm.body {
        Expr::Block { statements, .. } => statements.first().and_then(crate::stmt::stmt_position),
        _ => None,
    }
    .or_else(|| crate::stmt::tail_position(&arm.body));
    cg.with_debug_scope(position, |cg| {
        cg.with_local_scope(|cg| {
            bind(cg)?;
            push_arm(cg, &arm.body, phi_in)
        })
    })
}

/// Open a guarded arm: branch on `cond` into a fresh body block, make that
/// block current, and hand back the fall-through label the next arm starts at.
fn open_guarded_arm(cg: &mut Codegen, cond: &str) -> String {
    cg.open_guard(cond)
}

/// Complete a guarded arm after its shape-specific bindings have been emitted.
fn finish_guarded_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    next: &str,
    is_last: bool,
    bind: impl FnOnce(&mut Codegen),
) -> Result<()> {
    finish_checked_arm(cg, arm, phi_in, next, is_last, |cg| {
        bind(cg);
        Ok(())
    })
}

fn finish_checked_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    next: &str,
    is_last: bool,
    bind: impl FnOnce(&mut Codegen) -> Result<()>,
) -> Result<()> {
    try_emit_arm(cg, arm, phi_in, bind)?;
    cg.start_block(next);
    if is_last {
        cg.emit("unreachable");
    }
    Ok(())
}

/// Take a catch-all arm: bind the scrutinee under the arm's name and evaluate
/// its body — the shared tail of every match shape's fall-through.
fn take_catch_all(
    cg: &mut Codegen,
    arm: &MatchArm,
    disc: &Value,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    emit_arm(cg, arm, phi_in, |cg| bind_catch_all(cg, &arm.pattern, disc))
}

fn bind_catch_all(cg: &mut Codegen, pattern: &Pattern, disc: &Value) {
    match pattern {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => {
            bind_value(cg, name.clone(), disc.clone());
        }
        _ => {}
    }
}

/// Pattern names shadow every representation of an enclosing binding.
/// Implements [PATTERN-BINDING-SCOPE].
fn bind_value(cg: &mut Codegen, name: String, value: Value) {
    cg.forget_binding(&name);
    if let Some(ty @ osprey_types::Type::Fun { .. }) = &value.inferred_type {
        cg.bind_fn_local(&name, ty.clone());
    }
    cg.emit_debug_local(&name, &value);
    cg.bind(name, value);
}
