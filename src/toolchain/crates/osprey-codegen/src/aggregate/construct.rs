//! Named aggregate construction with complete semantic field values.
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
    materialize(cg, name, view, &values)
}

pub(super) fn materialize(
    cg: &mut Codegen,
    name: &str,
    view: crate::builder::CtorView,
    values: &[(Value, MetaField)],
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
    let obj = cg.malloc_struct_noinit(&struct_ty, constructor_meta(cg, view.meta, values));
    store_tagged_fields(cg, &struct_ty, &obj, &view, values);
    Ok(super::own_struct_handle(cg, &struct_ty, &obj, view.owner))
}

fn store_tagged_fields(
    cg: &mut Codegen,
    struct_ty: &str,
    obj: &str,
    view: &crate::builder::CtorView,
    values: &[(Value, MetaField)],
) {
    super::store_tag(cg, struct_ty, obj, view.tag);
    for (i, ((_, ty), (value, field_meta))) in view.fields.iter().zip(values).enumerate() {
        // A later initializer can abandon construction through a control effect.
        // Retain boxed children only after every initializer has completed.
        if *field_meta == MetaField::ManagedWord {
            let pointer = crate::conv::unbox_from_i64(cg, &value.operand, LType::Ptr);
            crate::arc::dup_store(cg, pointer.ty.as_str(), &pointer.operand);
        }
        super::store_field(cg, struct_ty, obj, i + 1, *ty, &value.operand);
    }
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

fn constructor_meta(cg: &Codegen, declared: i64, values: &[(Value, MetaField)]) -> i64 {
    if !values
        .iter()
        .any(|(_, meta)| *meta == MetaField::ManagedWord)
    {
        return declared;
    }
    let fields = std::iter::once(MetaField::Word)
        .chain(values.iter().map(|(_, meta)| *meta))
        .collect::<Vec<_>>();
    cg.struct_meta(&fields)
}
