//! Named aggregate construction with complete semantic field values.
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
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
    if !view.owner_is_record && view.fields.is_empty() {
        return Ok(Value::handle(
            cg.nullary_singleton(name, view.tag),
            view.owner,
        ));
    }
    let struct_ty = cg
        .ctor_struct_ty(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    let obj = cg.malloc_struct_noinit(&struct_ty, view.meta);
    super::store_tag(cg, &struct_ty, &obj, view.tag);
    for (i, (field, ty)) in view.fields.iter().enumerate() {
        let value = constructor_field(cg, name, field, fields, *ty)?;
        super::store_field(cg, &struct_ty, &obj, i + 1, *ty, &value.operand);
    }
    Ok(super::own_struct_handle(cg, &struct_ty, &obj, view.owner))
}

fn constructor_field(
    cg: &mut Codegen,
    owner: &str,
    field: &str,
    fields: &[FieldAssignment],
    ty: LType,
) -> Result<Value> {
    let assignment = fields
        .iter()
        .find(|f| f.name == field)
        .ok_or_else(|| CodegenError::invalid(format!("missing field `{field}` for `{owner}`")))?;
    let value = crate::expr::gen_expr(cg, &assignment.value)?;
    let value = crate::listlit::escaping(cg, value);
    let value = super::fields::coerce_field(cg, value, owner, field, None)?;
    let stored = crate::cast::erase_result(cg, value);
    if ty == LType::I64 {
        Ok(crate::conv::box_to_i64(cg, stored))
    } else {
        crate::cast::coerce_to(cg, stored, ty)
    }
}
