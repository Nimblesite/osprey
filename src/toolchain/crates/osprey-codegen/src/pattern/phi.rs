//! Joining match arms: the result `phi`, literal arm fitting and the
//! inference that reconciles arm layouts.

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};

/// Join the arm values with a `phi`. A single arm needs none. `Str`/`Ptr` count
/// as the same type (both `i8*`). A common owner / payload-owner across arms is
/// preserved so a matched handle (record, nested list) stays field-accessible /
/// indexable.
///
/// Arms that disagree on LLVM type are a hard error, not a silent Unit: a `phi`
/// over them would be ill-typed, and yielding Unit instead converted a class of
/// type-system mistakes into an expression that quietly evaluated to nothing.
pub(super) fn finish_phi(
    cg: &mut Codegen,
    phi_in: &[(Value, String)],
    end: &str,
    mark: usize,
) -> Result<Value> {
    let target_result_inner = result_join_inner(phi_in).map_err(|error| match error {
        CodegenError::Invalid(why) => CodegenError::Invalid(format!(
            "{why}{}",
            cg.source_function
                .as_deref()
                .map_or_default(|name| format!(" in `{name}`"))
        )),
        other => other,
    })?;
    // When any arm yields an erased box, every arm must: a concrete value
    // travelling under `LType::Any` would have its bytes read as a shape
    // descriptor by rendering and narrowing — a crash, found the moment a
    // catch-all's string joined a narrowed field ([TYPE-ANY]).
    let wants_any = phi_in.iter().any(|(v, _)| v.ty == LType::Any);
    let adapted_arms = adapt_phi_arms(cg, phi_in, target_result_inner, wants_any)?;
    let adapted_arms = fit_literal_arms(cg, adapted_arms)?;
    let arm_values = adapted_arms
        .iter()
        .map(|(value, _)| value.clone())
        .collect::<Vec<_>>();
    // Snapshot every edge before promoting one: two borrowed exits may name
    // the same SSA operand, but only one of their retains runs on any path.
    let borrowed_arms = crate::arc::borrowed_phi_arms(cg, &arm_values, mark);
    let mut incoming_values = Vec::with_capacity(phi_in.len());
    for ((adapted, exit), borrowed) in adapted_arms.into_iter().zip(borrowed_arms) {
        cg.start_block(&exit);
        crate::arc::promote_phi_arm(cg, &adapted, borrowed);
        let pred = cg.snapshot_to(end);
        incoming_values.push((adapted, pred));
    }
    cg.start_block(end);
    crate::arc::close_conditional(cg);

    let Some((first_val, _)) = incoming_values.first() else {
        return Ok(Value::unit());
    };
    let ty = first_val.ty;
    let llvm_ty = first_val.llvm_ty();
    if let Some((odd, _)) = incoming_values.iter().find(|(v, _)| v.llvm_ty() != llvm_ty) {
        if !cg.value_discarded {
            return Err(CodegenError::invalid(format!(
                "match arms disagree on type in `{}`: `{}` and `{}`",
                cg.source_function.as_deref().unwrap_or("<entry>"),
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
        None => Value::new(reg, ty).with_owner(merged_owner(cg, &incoming_values)),
    };
    out.inferred_type = merged_inference(&incoming_values);
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

/// An arm that is a literal joins its sized siblings at their type: the checker
/// proved one type, and the literal still spells its default — `int` as `i64`,
/// `float` as `double` ([TYPE-NUMERIC-LITERAL]).
pub(super) fn fit_literal_arms(
    cg: &mut Codegen,
    arms: Vec<(Value, String)>,
) -> Result<Vec<(Value, String)>> {
    let sized = arms.iter().find_map(|(value, _)| match value.ty {
        LType::Num(n) if n.is_sized() => Some(n),
        _ => None,
    });
    let Some(sized) = sized else {
        return Ok(arms);
    };
    arms.into_iter()
        .map(|(value, exit)| {
            if !matches!(value.ty, LType::I64 | LType::F64) {
                return Ok((value, exit));
            }
            cg.start_block(&exit);
            let fitted = crate::conv::fit_literal(cg, value, Some(sized))?;
            let next = cg.fresh_label();
            cg.emit(format!("br label %{next}"));
            Ok((fitted, next))
        })
        .collect()
}

pub(super) fn adapt_phi_arms(
    cg: &mut Codegen,
    arms: &[(Value, String)],
    inner: Option<LType>,
    wants_any: bool,
) -> Result<Vec<(Value, String)>> {
    arms.iter()
        .map(|(value, exit)| {
            cg.start_block(exit);
            let adapted = adapt_phi_value(cg, value.clone(), inner, wants_any)?;
            let next = cg.fresh_label();
            cg.emit(format!("br label %{next}"));
            Ok((adapted, next))
        })
        .collect()
}

pub(super) fn adapt_phi_value(
    cg: &mut Codegen,
    value: Value,
    inner: Option<LType>,
    wants_any: bool,
) -> Result<Value> {
    let adapted = match inner {
        Some(inner) => crate::result::repack_to_inner(cg, value, inner)?,
        None => value,
    };
    if wants_any && adapted.ty != LType::Any {
        crate::anybox::box_any(cg, adapted)
    } else {
        Ok(adapted)
    }
}

/// A checked join retains the concrete type even when an empty literal has no
/// inference of its own. Losing it erases nested list element layouts after a
/// `?:` default. Conflicting known types never supply a guessed layout.
pub(super) fn merged_inference(incoming: &[(Value, String)]) -> Option<osprey_types::Type> {
    let mut known = incoming
        .iter()
        .filter_map(|(value, _)| value.inferred_type.as_ref())
        .filter(|ty| !osprey_types::has_type_var(ty));
    let first = known.next()?;
    known.all(|ty| ty == first).then(|| first.clone())
}

/// The owner of the merged handle. Arms naming one owner keep it; arms naming
/// different owners for one physical block — an object literal's `__obj_N`
/// beside a row-typed binding's `__row#…` — share the first. Dropping it made
/// field access after the join guess a record by field name: a hard error, or
/// another record's slots read silently (#272). Implements [TYPE-RECORD],
/// [STAGE-SIGNALS-REBUILD].
pub(super) fn merged_owner(cg: &mut Codegen, incoming: &[(Value, String)]) -> Option<String> {
    let (first, _) = incoming.first()?;
    let owner = first.osp_ty.clone()?;
    let shape = (owner.as_str(), first.inferred_type.as_ref());
    incoming
        .iter()
        .all(|(value, _)| match value.osp_ty.as_deref() {
            Some(other) if other == owner => true,
            Some(other) => {
                crate::aggregate::same_block(cg, shape, (other, value.inferred_type.as_ref()))
            }
            None => false,
        })
        .then_some(owner.clone())
        .or_else(|| typed_list_owner(incoming))
}

/// The owner of a join of lists whose element tags differ: the tag of an arm
/// inference typed. An empty literal inference could not name elements for is
/// tagged by default, and it has no elements for that tag to describe. Dropping
/// the owner sent `length` to the string runtime, which read the list handle as
/// text ([BUILTIN-COLLECTION-LENGTH]).
pub(super) fn typed_list_owner(incoming: &[(Value, String)]) -> Option<String> {
    let all_lists = incoming.iter().all(|(value, _)| {
        value
            .osp_ty
            .as_deref()
            .is_some_and(crate::collections::is_list_owner)
    });
    all_lists.then_some(())?;
    let typed = merged_inference(incoming)?;
    incoming
        .iter()
        .find(|(value, _)| value.inferred_type.as_ref() == Some(&typed))
        .and_then(|(value, _)| value.osp_ty.clone())
}

/// Resolve the concrete success-slot layout for a Result-valued match.  A bare
/// Error constructor contributes only a placeholder layout; any real producer
/// fixes the contextual `T`.  Multiple concrete layouts remain a hard backend
/// error (the type checker should already have rejected such arms).
pub(super) fn result_join_inner(phi_in: &[(Value, String)]) -> Result<Option<LType>> {
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
pub(super) fn result_payload_property<T: PartialEq>(
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
