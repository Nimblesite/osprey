//! Named aggregate construction with complete semantic field values.
use crate::arc::Held;
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::meta::MetaField;
use osprey_ast::FieldAssignment;

pub(crate) fn gen_constructor(
    cg: &mut Codegen,
    name: &str,
    fields: &[FieldAssignment],
) -> Result<Value> {
    if !cg.is_ctor(name) {
        return Err(CodegenError::unknown(name));
    }
    if (name == "Success" || name == "Error") && !fields.is_empty() {
        return super::gen_result_ctor(cg, name, fields);
    }
    if name == super::HTTP_RESPONSE {
        return super::gen_http_response(cg, fields);
    }
    named_constructor(cg, name, fields)
}

fn named_constructor(cg: &mut Codegen, name: &str, fields: &[FieldAssignment]) -> Result<Value> {
    let view = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    if view.owner_is_record && !cg.ctor_type_params(name).is_empty() {
        return super::gen_generic_record(cg, name, fields);
    }
    tagged_constructor(cg, name, fields, view)
}

fn tagged_constructor(
    cg: &mut Codegen,
    name: &str,
    fields: &[FieldAssignment],
    view: crate::builder::CtorView,
) -> Result<Value> {
    let values = view
        .fields
        .iter()
        .map(|(field, ty)| constructor_field(cg, name, field, fields, *ty))
        .collect::<Result<Vec<_>>>()?;
    let places = view
        .fields
        .iter()
        .map(|(field, _)| {
            fields
                .iter()
                .any(|f| f.name == *field && is_place(&f.value))
        })
        .collect::<Vec<_>>();
    materialize(cg, name, view, &values, &places)
}

/// Whether `value` is written as a binding or a field of one: something whose
/// owner has a name, and so can outlive a value that only holds it.
fn is_place(value: &osprey_ast::Expr) -> bool {
    match value {
        osprey_ast::Expr::Identifier(_) => true,
        osprey_ast::Expr::FieldAccess { target, .. } => is_place(target),
        _ => false,
    }
}

pub(super) fn materialize(
    cg: &mut Codegen,
    name: &str,
    view: crate::builder::CtorView,
    values: &[(Value, MetaField)],
    places: &[bool],
) -> Result<Value> {
    if !view.owner_is_record && view.fields.is_empty() {
        return Ok(Value::handle(
            cg.nullary_singleton(name, view.tag),
            view.owner,
        ));
    }
    let struct_ty = cg
        .ctor_struct_ty(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    let held = hold_fields(cg, values, places);
    let obj = cg.malloc_struct_noinit(&struct_ty, constructor_meta(cg, view.meta, values, &held));
    store_tagged_fields(cg, &struct_ty, &obj, &view, values, &held);
    let made = super::own_struct_handle(cg, &struct_ty, &obj, view.owner);
    if held.contains(&Held::Lent) {
        crate::arc::mark_lent(cg, &made);
    }
    Ok(made)
}

/// The built-in record a failed list read holds, from its two words.
/// Implements [BUILTIN-LIST-GET].
pub(crate) fn index_out_of_bounds(cg: &mut Codegen, index: &str, length: &str) -> Result<Value> {
    let name = osprey_types::names::INDEX_OUT_OF_BOUNDS;
    let view = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    let words = [index, length].map(|word| (Value::new(word, LType::I64), MetaField::Word));
    let mut record = materialize(cg, name, view, &words, &[])?;
    record.inferred_type = Some(osprey_types::Type::con(name, vec![]));
    Ok(record)
}

fn store_tagged_fields(
    cg: &mut Codegen,
    struct_ty: &str,
    obj: &str,
    view: &crate::builder::CtorView,
    values: &[(Value, MetaField)],
    held: &[Held],
) {
    super::store_tag(cg, struct_ty, obj, view.tag);
    let fields = view.fields.iter().zip(values).zip(held).enumerate();
    for (i, (((_, ty), (value, field_meta)), held)) in fields {
        if *held != Held::Stored {
            super::store_held_field(cg, struct_ty, obj, i + 1, *ty, &value.operand);
            continue;
        }
        // A later initializer can abandon construction through a control effect.
        // Retain boxed children only after every initializer has completed.
        if *field_meta == MetaField::ManagedWord {
            let pointer = crate::conv::unbox_from_i64(cg, &value.operand, LType::Ptr);
            crate::arc::dup_store(cg, pointer.ty.as_str(), &pointer.operand);
        }
        super::store_field(cg, struct_ty, obj, i + 1, *ty, &value.operand);
    }
}

/// How the new value holds each field. Under ownership a managed field written
/// as a place is moved in or lent [MEM-OWNERSHIP-STATIC-BORROWS]; every other
/// field is stored as before.
fn hold_fields(cg: &mut Codegen, values: &[(Value, MetaField)], places: &[bool]) -> Vec<Held> {
    values
        .iter()
        .zip(places.iter().chain(std::iter::repeat(&false)))
        .map(|((value, meta), place)| match meta {
            _ if !place => Held::Stored,
            MetaField::ManagedWord => {
                let pointer = crate::conv::unbox_from_i64(cg, &value.operand, LType::Ptr);
                crate::arc::hold_field(cg, &pointer)
            }
            MetaField::PtrManaged | MetaField::PtrDirect => crate::arc::hold_field(cg, value),
            _ => Held::Stored,
        })
        .collect()
}

fn constructor_field(
    cg: &mut Codegen,
    owner: &str,
    field: &str,
    fields: &[FieldAssignment],
    ty: LType,
) -> Result<(Value, MetaField)> {
    let assignment = fields
        .iter()
        .find(|f| f.name == field)
        .ok_or_else(|| CodegenError::invalid(format!("missing field `{field}` for `{owner}`")))?;
    let stored = super::settled_field_value(cg, &assignment.value, owner, field, None)?;
    storage_value(cg, stored, ty)
}

pub(super) fn storage_value(
    cg: &mut Codegen,
    stored: Value,
    ty: LType,
) -> Result<(Value, MetaField)> {
    let meta = if ty == LType::I64 && stored.ty.is_managed_ptr() {
        MetaField::ManagedWord
    } else {
        MetaField::of_lty(ty)
    };
    if ty == LType::I64 {
        Ok((crate::conv::box_to_i64(cg, stored), meta))
    } else {
        Ok((crate::cast::coerce_to(cg, stored, ty)?, meta))
    }
}

/// The layout of a new value: the declared one, unless a field holds a boxed
/// pointer or lends. A lent field is laid out as a word the drop walk skips.
fn constructor_meta(
    cg: &Codegen,
    declared: i64,
    values: &[(Value, MetaField)],
    held: &[Held],
) -> i64 {
    let boxed = values
        .iter()
        .any(|(_, meta)| *meta == MetaField::ManagedWord);
    if !boxed && !held.contains(&Held::Lent) {
        return declared;
    }
    let stored = values
        .iter()
        .zip(held)
        .map(|((_, meta), held)| match (held, meta) {
            (Held::Lent, MetaField::ManagedWord) => MetaField::Word,
            (Held::Lent, _) => MetaField::PtrOpaque,
            _ => *meta,
        });
    let fields = std::iter::once(MetaField::Word)
        .chain(stored)
        .collect::<Vec<_>>();
    cg.struct_meta(&fields)
}
