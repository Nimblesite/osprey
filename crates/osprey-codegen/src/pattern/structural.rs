//! Structural pattern lowering.
use super::{bind_value, emit_arm, finish_phi, match_state, open_guarded_arm, take_catch_all};
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::{MatchArm, Pattern};

/// Structural-pattern match ([PATTERN-STRUCTURAL], [PATTERN-TUPLE]). Three
/// scrutinee shapes:
///   * an erased box ([`LType::Any`]) — arms select by descriptor identity
///     against the compile-time candidate set;
///   * a concrete record handle — the first well-typed structural arm always
///     selects, statically;
///   * any other representation (an `any`-typed binding still carrying its
///     concrete scalar form) — every field-naming arm declines and the
///     catch-all takes it.
pub(super) fn gen_structural_match(
    cg: &mut Codegen,
    disc: &Value,
    arms: &[MatchArm],
) -> Result<Value> {
    if disc.ty == LType::Any {
        return gen_any_match(cg, disc, arms);
    }
    let record = disc
        .osp_ty
        .as_deref()
        .filter(|_| disc.ty == LType::Ptr)
        .and_then(|o| cg.record_layout(o).map(|(ty, fs)| (o.to_string(), ty, fs)));
    let (end, mut phi_in, _, mark) = match_state(cg, arms);
    for arm in arms {
        match (&arm.pattern, &record) {
            (Pattern::Structural { fields, open }, Some((owner, struct_ty, layout)))
                if row_selects(fields, *open, layout) =>
            {
                emit_arm(cg, arm, &mut phi_in, |cg| {
                    bind_row_statically(cg, disc, owner, struct_ty, layout, fields);
                })?;
                break;
            }
            (Pattern::Structural { .. }, _) => {} // can never select — skip
            _ => {
                take_catch_all(cg, arm, disc, &mut phi_in)?;
                break;
            }
        }
    }
    finish_phi(cg, &phi_in, &end, mark)
}

/// Whether a structural arm selects this concrete layout: an open pattern
/// needs its named fields present, a closed one the exact row ([TYPE-ROW]).
fn row_selects(fields: &[(String, String)], open: bool, layout: &[(String, LType)]) -> bool {
    let present = |f: &str| layout.iter().any(|(n, _)| n == f);
    fields.iter().all(|(f, _)| present(f)) && (open || fields.len() == layout.len())
}

/// Bind a structural arm's fields straight off a concrete record block — the
/// binder takes the DECLARED slot type, exactly as the checker typed it.
fn bind_row_statically(
    cg: &mut Codegen,
    disc: &Value,
    owner: &str,
    struct_ty: &str,
    layout: &[(String, LType)],
    fields: &[(String, String)],
) {
    let src = cg.emit_reg(format!("bitcast i8* {} to {struct_ty}*", disc.operand));
    for (fname, binder) in fields {
        if binder.is_empty() {
            continue;
        }
        let Some((idx, fty)) = layout
            .iter()
            .enumerate()
            .find_map(|(i, (n, t))| (n == fname).then_some((i, *t)))
        else {
            continue;
        };
        let loaded = crate::aggregate::load_field(cg, struct_ty, &src, idx + 1, fty);
        let field_owner = cg.ctor_field_owner(owner, fname);
        bind_value(
            cg,
            binder.clone(),
            Value::new(loaded, fty).with_owner(field_owner),
        );
    }
}

/// Narrow an erased box: load its descriptor once, then test each structural
/// arm's candidate descriptors by pointer identity. Every candidate row is a
/// DECLARED record shape ([`crate::anybox::candidate_rows`]), so the set is
/// complete at compile time and no name is compared at run time. Bound fields
/// are the row's child boxes — each binder stays `any`, exactly as inference
/// typed it.
fn gen_any_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let (desc_ptr, payload) = crate::anybox::open_box(cg, disc);
    let (end, mut phi_in, _, mark) = match_state(cg, arms);
    for arm in arms {
        match &arm.pattern {
            Pattern::Structural { fields, open } => {
                let names: Vec<String> = fields.iter().map(|(f, _)| f.clone()).collect();
                for row in crate::anybox::candidate_rows(cg, &names, *open) {
                    let global =
                        crate::anybox::descriptor(cg, &crate::anybox::DescKey::Row(row.clone()))?;
                    let cond = cg.emit_reg(format!(
                        "icmp eq i8* {desc_ptr}, {}",
                        crate::anybox::desc_operand(&global)
                    ));
                    let next_lbl = open_guarded_arm(cg, &cond);
                    emit_arm(cg, arm, &mut phi_in, |cg| {
                        bind_row_children(cg, &payload, &row, fields);
                    })?;
                    cg.start_block(&next_lbl);
                }
            }
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                take_catch_all(cg, arm, disc, &mut phi_in)?;
                break;
            }
            _ => {
                return Err(CodegenError::unsupported(
                    "non-structural arm over an erased `any`",
                ))
            }
        }
    }
    // The checker requires a catch-all over `any`, so control never falls out.
    cg.emit("unreachable");
    finish_phi(cg, &phi_in, &end, mark)
}

/// Bind each named field of a matched candidate row: slot index comes from
/// THAT row's layout order, and the loaded child box is bound as `any`,
/// borrowing the scrutinee's storage exactly as a variant bind does.
fn bind_row_children(cg: &mut Codegen, payload: &str, row: &[String], fields: &[(String, String)]) {
    let row_ty = crate::anybox::row_block_ty(row.len());
    let block = cg.emit_reg(format!("inttoptr i64 {payload} to {row_ty}*"));
    for (fname, binder) in fields {
        if binder.is_empty() {
            continue;
        }
        let Some(idx) = row.iter().position(|n| n == fname) else {
            continue;
        };
        let child = crate::aggregate::load_field(cg, &row_ty, &block, idx, LType::Any);
        bind_value(cg, binder.clone(), Value::new(child, LType::Any));
    }
}
