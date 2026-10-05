//! Union pattern lowering.
use super::{bind_value, emit_arm, finish_phi, open_guarded_arm, take_catch_all};
use crate::builder::Codegen;
use crate::builder::CtorView;
use crate::error::{CodegenError, Result};
use crate::llty::Value;
use osprey_ast::{MatchArm, Pattern};

/// How a constructor arm's binders map onto the variant's payload slots. The
/// *pattern form* decides this, never the declaration: `osprey-types` binds
/// `sub_patterns` by column and `fields` by name for every variant alike, so
/// codegen must agree or a well-typed arm reads the wrong slot.
#[derive(Clone, Copy)]
enum BindMode {
    /// `Ctor { a, b }` — each binder names the slot it takes, so a reordered
    /// destructure (`PersonData { age, name }`) still binds correctly.
    ByName,
    /// `Ctor(a, b)`, and ML `Ctor a b` — column *i* takes payload slot *i*
    /// whatever the binder is spelled ([TYPE-UNION-POSITIONAL]).
    BySlot,
}

/// The constructor name a pattern selects, if any: an explicit `Ctor { … }` or a
/// bare `Ctor` (a nullary variant lowers to a `Binding` indistinguishable from a
/// capture until we know the constructor table).
fn pattern_ctor<'a>(cg: &Codegen, p: &'a Pattern) -> Option<(&'a str, Vec<String>, BindMode)> {
    match p {
        // `Ctor { a, b }` names its binders directly; `Ctor(a, b)` carries them
        // as sub-patterns, which bind by slot ([TYPE-UNION-POSITIONAL]). A
        // sub-pattern that is not a plain binder contributes no name — nested
        // destructuring is not supported and is rejected upstream.
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } if fields.is_empty() => Some((
            name,
            sub_patterns.iter().map(binder_name).collect(),
            BindMode::BySlot,
        )),
        Pattern::Constructor { name, fields, .. } => Some((name, fields.clone(), BindMode::ByName)),
        Pattern::Binding(name) if cg.is_ctor(name) => Some((name, Vec::new(), BindMode::BySlot)),
        _ => None,
    }
}

/// The name a positional sub-pattern binds, or the ignored-slot placeholder for
/// a wildcard / unsupported shape.
fn binder_name(p: &Pattern) -> String {
    match p {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => name.clone(),
        _ => String::new(),
    }
}

/// The owner type name a constructor arm destructures, if any. That owner is
/// either a multi-variant union (present in `union_variants`) or a single-variant
/// record: the record shorthand `type V = V { … }` is classified as a record, so
/// it never enters `union_variants`, yet its heap block is the same
/// `{ i64 tag, fields… }` shape carrying tag `0` — so `gen_union_match` binds its
/// fields identically. Without the record arm here such a match falls through to
/// `gen_literal_match`, which rejects the constructor pattern (#175). Result
/// (`Success`/`Error`) arms are dispatched earlier and never reach this.
pub(super) fn union_owner(cg: &Codegen, arms: &[MatchArm]) -> Option<String> {
    for a in arms {
        if let Some((name, _, _)) = pattern_ctor(cg, &a.pattern) {
            if let Some(view) = cg.ctor_layout(name) {
                if view.owner_is_record || cg.union_variants(&view.owner).is_some() {
                    return Some(view.owner);
                }
            }
        }
    }
    None
}

/// User-union match: read the leading tag of the heap block and branch per
/// variant, binding that variant's fields.
pub(super) fn gen_union_match(
    cg: &mut Codegen,
    disc: &Value,
    arms: &[MatchArm],
    owner: &str,
) -> Result<Value> {
    // Load the discriminant tag (every variant block starts with `{ i64 tag, … }`).
    let tag = discriminant(cg, disc, owner);

    let end = cg.fresh_label();
    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    let variants = cg.union_variants(owner).unwrap_or(&[]).to_vec();

    for arm in arms {
        if let Some((name, fields, mode)) = pattern_ctor(cg, &arm.pattern) {
            let name = name.to_string();
            let vpos = variants.iter().position(|v| *v == name).unwrap_or(0);
            let vtag = i64::try_from(vpos).unwrap_or(0);
            let cond = cg.emit_reg(format!("icmp eq i64 {tag}, {vtag}"));
            let next_lbl = open_guarded_arm(cg, &cond);
            emit_arm(cg, arm, &mut phi_in, |cg| {
                bind_variant_fields(cg, disc, &name, &fields, mode);
            })?;
            cg.start_block(&next_lbl);
        } else {
            match &arm.pattern {
                Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                    take_catch_all(cg, arm, disc, &mut phi_in)?;
                    break;
                }
                _ => return Err(CodegenError::unsupported("structural union arm")),
            }
        }
    }
    // A non-exhaustive fall-through is unreachable by construction.
    cg.emit("unreachable");
    finish_phi(cg, &phi_in, &end, mark)
}

/// A variant declared positionally (`Fail string`) carries only synthetic field
/// names, so no binder in a `fields` destructure can ever name one.
fn declared_positionally(view: &CtorView) -> bool {
    view.fields
        .first()
        .is_some_and(|(f, _)| osprey_ast::is_positional_field(f))
}

/// The payload slot a binder column resolves to, or `None` when a named
/// destructure mentions a field this variant does not declare.
fn slot_of(view: &CtorView, column: usize, bind_name: &str, mode: BindMode) -> Option<usize> {
    let by_slot = (column < view.fields.len()).then_some(column);
    match mode {
        BindMode::BySlot => by_slot,
        // A `fields` destructure of a POSITIONALLY declared variant is
        // positional after all — its binders name nothing, so they must take
        // their column ([TYPE-UNION-POSITIONAL]). Named payloads keep the strict
        // lookup: a binder naming no field binds nothing.
        BindMode::ByName => match view.fields.iter().position(|(f, _)| f == bind_name) {
            Some(idx) => Some(idx),
            None if declared_positionally(view) => by_slot,
            None => None,
        },
    }
}

/// The `{ i64 tag, fields… }` view and LLVM struct type of a variant that has a
/// payload for this pattern to bind, or `None` when there is nothing to bind.
fn bindable_layout(
    cg: &Codegen,
    variant: &str,
    pat_fields: &[String],
) -> Option<(CtorView, String)> {
    let view = cg.ctor_layout(variant)?;
    let struct_ty = cg.ctor_struct_ty(variant)?;
    let bindable = !view.fields.is_empty() && !pat_fields.is_empty();
    bindable.then_some((view, struct_ty))
}

/// Bind a matched variant's fields (in declared order) from the heap block. The
/// value's owner type comes from the DECLARED name at the resolved slot, not
/// from the binder's spelling, which under [`BindMode::BySlot`] names no field.
fn bind_variant_fields(
    cg: &mut Codegen,
    disc: &Value,
    variant: &str,
    pat_fields: &[String],
    mode: BindMode,
) {
    let Some((view, struct_ty)) = bindable_layout(cg, variant, pat_fields) else {
        return;
    };
    let src = cg.emit_reg(format!("bitcast i8* {} to {struct_ty}*", disc.operand));
    for (column, bind_name) in pat_fields.iter().enumerate() {
        if bind_name.is_empty() {
            continue; // an ignored slot binds nothing
        }
        let Some(idx) = slot_of(&view, column, bind_name, mode) else {
            continue;
        };
        let Some((declared, fty)) = view.fields.get(idx) else {
            continue;
        };
        let fty = *fty;
        let loaded = crate::aggregate::load_record_field(cg, variant, &struct_ty, &src, idx, fty);
        let inferred = crate::aggregate::field_type(cg, disc, variant, declared);
        let value = crate::aggregate::restore_field(
            cg,
            Value::new(loaded, fty),
            variant,
            declared,
            inferred.as_ref(),
        );
        bind_value(cg, bind_name.clone(), value);
    }
}

fn discriminant(cg: &mut Codegen, value: &Value, owner: &str) -> String {
    if !crate::aggregate::record_has_tag(owner) {
        return "0".to_string();
    }
    let pointer = cg.emit_reg(format!("bitcast i8* {} to i64*", value.operand));
    cg.emit_reg(format!("load i64, i64* {pointer}"))
}
