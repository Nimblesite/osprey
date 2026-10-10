//! Immutable updates preserve the concrete record ABI and source. [TYPE-RECORD-UPDATE]
use super::{
    gen_expr, load_record_field, own_struct_handle, record_block, store_record_field, store_tag,
};
use super::{Codegen, CodegenError, Expr, FieldAssignment, RecordBlock, Result, Value};

pub(crate) fn gen_update(
    cg: &mut Codegen,
    record: &str,
    fields: &[FieldAssignment],
) -> Result<Value> {
    // Resolve through normal lookup so module globals and promoted cells agree.
    let base = gen_expr(cg, &Expr::Identifier(record.to_owned()))?;
    let owner = base
        .osp_ty
        .clone()
        .ok_or_else(|| CodegenError::invalid(format!("`{record}` is not a record")))?;
    let block = record_block(cg, &owner, base.inferred_type.as_ref())
        .ok_or_else(|| CodegenError::unknown(&owner))?;
    copy_record(cg, &owner, &base, &block, fields)
}

fn copy_record(
    cg: &mut Codegen,
    owner: &str,
    base: &Value,
    block: &RecordBlock,
    fields: &[FieldAssignment],
) -> Result<Value> {
    let ty = &block.struct_ty;
    let source = cg.emit_reg(format!("bitcast i8* {} to {ty}*", base.operand));
    // Every tag and field is initialized before the new allocation escapes.
    let target = cg.malloc_struct_noinit(ty, block.meta);
    if let Some(tag) = block.tag {
        store_tag(cg, ty, &target, tag);
    }
    copy_fields(cg, owner, base, block, &source, &target, fields)?;
    Ok(own_struct_handle(cg, ty, &target, owner))
}

fn copy_fields(
    cg: &mut Codegen,
    owner: &str,
    base: &Value,
    block: &RecordBlock,
    source: &str,
    target: &str,
    overrides: &[FieldAssignment],
) -> Result<()> {
    for (index, (name, ty)) in block.fields.iter().enumerate() {
        let value = match overrides.iter().find(|field| &field.name == name) {
            Some(field) => {
                let value = gen_expr(cg, &field.value)?;
                let inferred = super::field_type(cg, base, owner, name);
                let value = super::fields::coerce_field(cg, value, owner, name, inferred.as_ref())?;
                crate::cast::erase_result(cg, value).operand
            }
            None => load_record_field(cg, owner, &block.struct_ty, source, index, *ty),
        };
        store_record_field(cg, owner, &block.struct_ty, target, index, *ty, &value);
    }
    Ok(())
}
