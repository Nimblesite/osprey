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
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm, Pattern};

mod list;
mod literal;
mod result;
mod structural;
mod union;
use list::gen_list_match;
use literal::gen_literal_match;
use result::{gen_result_match, is_result_arm};
use structural::gen_structural_match;
use union::{gen_union_match, union_owner};

pub(crate) fn gen_match(cg: &mut Codegen, value: &Expr, arms: &[MatchArm]) -> Result<Value> {
    let disc = gen_expr(cg, value)?;
    if arms.iter().any(|a| is_result_arm(&a.pattern))
        && (disc.result_inner.is_some()
            || union_owner(cg, arms).is_none_or(|owner| owner == "Result"))
    {
        return gen_result_match(cg, &disc, arms);
    }
    if arms
        .iter()
        .any(|a| matches!(a.pattern, Pattern::List { .. }))
    {
        return gen_list_match(cg, &disc, arms);
    }
    if arms
        .iter()
        .any(|a| matches!(a.pattern, Pattern::Structural { .. }))
    {
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
    let mark = crate::arc::frame_mark(cg);
    (
        cg.fresh_label(),
        Vec::new(),
        arms.len().saturating_sub(1),
        mark,
    )
}

/// Generate a successful match arm and branch to the common result block.
fn emit_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    bind: impl FnOnce(&mut Codegen),
) -> Result<()> {
    let position = match &arm.body {
        Expr::Block { statements, .. } => statements.first().and_then(crate::stmt::stmt_position),
        _ => None,
    }
    .or_else(|| crate::stmt::tail_position(&arm.body));
    cg.with_debug_scope(position, |cg| {
        cg.with_local_scope(|cg| {
            bind(cg);
            push_arm(cg, &arm.body, phi_in)
        })
    })
}

/// Open a guarded arm: branch on `cond` into a fresh body block, make that
/// block current, and hand back the fall-through label the next arm starts at.
fn open_guarded_arm(cg: &mut Codegen, cond: &str) -> String {
    let body_lbl = cg.fresh_label();
    let next_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {cond}, label %{body_lbl}, label %{next_lbl}"
    ));
    cg.start_block(&body_lbl);
    next_lbl
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
    emit_arm(cg, arm, phi_in, bind)?;
    cg.start_block(next);
    if is_last {
        cg.emit("unreachable");
    }
    Ok(())
}

/// Join the arm values with a `phi`. A single arm needs none. `Str`/`Ptr` count
/// as the same type (both `i8*`). A common owner / payload-owner across arms is
/// preserved so a matched handle (record, nested list) stays field-accessible /
/// indexable.
///
/// Arms that disagree on LLVM type are a hard error, not a silent Unit: a `phi`
/// over them would be ill-typed, and yielding Unit instead converted a class of
/// type-system mistakes into an expression that quietly evaluated to nothing.
fn finish_phi(
    cg: &mut Codegen,
    phi_in: &[(Value, String)],
    end: &str,
    mark: usize,
) -> Result<Value> {
    let target_result_inner = result_join_inner(phi_in)?;
    // When any arm yields an erased box, every arm must: a concrete value
    // travelling under `LType::Any` would have its bytes read as a shape
    // descriptor by rendering and narrowing — a crash, found the moment a
    // catch-all's string joined a narrowed field ([TYPE-ANY]).
    let wants_any = phi_in.iter().any(|(v, _)| v.ty == LType::Any);
    let mut incoming_values = Vec::with_capacity(phi_in.len());
    for (value, exit) in phi_in {
        cg.start_block(exit);
        let adapted = match target_result_inner {
            Some(inner) => crate::result::repack_to_inner(cg, value.clone(), inner)?,
            None => value.clone(),
        };
        let adapted = if wants_any && adapted.ty != LType::Any {
            crate::anybox::box_any(cg, adapted)?
        } else {
            adapted
        };
        let pred = cg.snapshot_to(end);
        incoming_values.push((adapted, pred));
    }
    cg.start_block(end);

    let Some((first_val, _)) = incoming_values.first() else {
        return Ok(Value::unit());
    };
    let ty = first_val.ty;
    let llvm_ty = first_val.llvm_ty();
    if let Some((odd, _)) = incoming_values.iter().find(|(v, _)| v.llvm_ty() != llvm_ty) {
        if !cg.value_discarded {
            return Err(CodegenError::invalid(format!(
                "match arms disagree on type: `{}` and `{}`",
                llvm_ty,
                odd.llvm_ty()
            )));
        }
        return Ok(Value::unit());
    }
    let incoming = incoming_values
        .iter()
        .map(|(v, blk)| format!("[ {}, %{blk} ]", v.operand))
        .collect::<Vec<_>>()
        .join(", ");
    let reg = cg.emit_reg(format!("phi {llvm_ty} {incoming}"));
    let common = |sel: fn(&Value) -> Option<String>| {
        let first = sel(first_val);
        incoming_values
            .iter()
            .all(|(v, _)| sel(v) == first)
            .then_some(first)
            .flatten()
    };
    // Preserve Result identity across the merge only when every arm has the
    // exact same block layout. The LLVM-type check above rejects mixed Result
    // payload layouts rather than emitting a broad pointer phi that discards
    // the discriminant-bearing type.
    let result_inner = first_val.result_inner.filter(|first| {
        incoming_values.iter().all(|(v, _)| {
            v.result_inner
                .is_some_and(|ri| ri.as_str() == first.as_str())
        })
    });
    let mut out = match result_inner {
        Some(inner) => Value::result(reg, inner),
        None => Value::new(reg, ty).with_owner(common(|v| v.osp_ty.clone())),
    };
    out.result_inner_is_placeholder = result_inner.is_some()
        && incoming_values
            .iter()
            .all(|(v, _)| v.result_inner_is_placeholder);
    out.payload_owner = if result_inner.is_some() {
        result_payload_property(phi_in, |value| value.payload_owner.clone())
    } else {
        common(|v| v.payload_owner.clone())
    };
    out.result_payload_type = result_payload_property(phi_in, |value| {
        value
            .element_type(osprey_types::names::RESULT)
            .or_else(|| value.result_payload_type.clone())
    });
    // Perceus join transfer: if every arm produced a fresh owner AFTER `mark`
    // (i.e. inside its own arm — never the scrutinee, which predates the mark
    // and lives on every path), the phi owns the merged value directly — the
    // arm entries move into it, no dup, no per-arm drop. Ledger bookkeeping
    // only; the repositioned dup/drop calls are no-ops off ARC.
    let incoming_ops = incoming_values
        .iter()
        .map(|(v, _)| v.operand.clone())
        .collect::<Vec<_>>();
    crate::arc::move_phi_owners(cg, &incoming_ops, &out, mark);
    Ok(out)
}

/// Resolve the concrete success-slot layout for a Result-valued match.  A bare
/// Error constructor contributes only a placeholder layout; any real producer
/// fixes the contextual `T`.  Multiple concrete layouts remain a hard backend
/// error (the type checker should already have rejected such arms).
fn result_join_inner(phi_in: &[(Value, String)]) -> Result<Option<LType>> {
    if phi_in.is_empty() || phi_in.iter().any(|(v, _)| v.result_inner.is_none()) {
        return Ok(None);
    }
    let mut concrete = phi_in
        .iter()
        .filter(|(v, _)| !v.result_inner_is_placeholder)
        .filter_map(|(v, _)| v.result_inner);
    let target = concrete
        .next()
        .or_else(|| phi_in.first().and_then(|(v, _)| v.result_inner));
    if let Some(target_inner) = target {
        if let Some(other) = concrete.find(|inner| *inner != target_inner) {
            return Err(CodegenError::invalid(format!(
                "match Result arms disagree on success type: `{target_inner}` and `{other}`"
            )));
        }
    }
    Ok(target)
}

/// [MODULES-ABI]: Error has no Success payload whose owner can disagree with
/// a record-producing arm. Use the original arms before placeholder repacking.
fn result_payload_property<T: PartialEq>(
    phi_in: &[(Value, String)],
    property: impl Fn(&Value) -> Option<T>,
) -> Option<T> {
    let mut values = phi_in
        .iter()
        .filter(|(value, _)| !value.result_inner_is_placeholder)
        .map(|(value, _)| property(value));
    let first = values.next()?;
    values
        .all(|value| value == first)
        .then_some(first)
        .flatten()
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
