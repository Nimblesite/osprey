//! Shared semantic field types for selection and pattern projection.
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::Expr;
use osprey_types::Type;

/// Retain every function arrow and concrete generic field instantiation.
/// Implements [TYPE-FN-HIGHER-ORDER] [PATTERN-BINDING-SCOPE].
pub(crate) fn field_type(cg: &Codegen, target: &Value, owner: &str, field: &str) -> Option<Type> {
    target
        .inferred_type
        .as_ref()
        .and_then(|ty| cg.prog.field_type(ty, field))
        .or_else(|| {
            cg.prog
                .field_type(&Type::con(owner, Vec::new()), field)
                .filter(|ty| !osprey_types::has_type_var(ty))
        })
}

pub(crate) fn gen_field_access(cg: &mut Codegen, target: &Expr, field: &str) -> Result<Value> {
    let target = crate::expr::gen_expr(cg, target)?;
    let owner = field_owner(cg, &target, field)?;
    let inferred = field_type(cg, &target, &owner, field);
    if cg.ctor_field_result_inner(&owner, field).is_some() {
        return Err(super::result_field_unsupported());
    }
    let mut value = load_field_value(cg, target, &owner, field, inferred.as_ref())?;
    value.inferred_type = inferred;
    Ok(value)
}

fn field_owner(cg: &Codegen, target: &Value, field: &str) -> Result<String> {
    target
        .osp_ty
        .clone()
        .filter(|owner| {
            cg.record_layout(owner)
                .is_some_and(|(_, fields)| fields.iter().any(|(name, _)| name == field))
        })
        .or_else(|| cg.find_field_owner(field))
        .ok_or_else(|| CodegenError::invalid(format!("field `{field}` on a non-record")))
}

fn load_field_value(
    cg: &mut Codegen,
    target: Value,
    owner: &str,
    field: &str,
    inferred: Option<&Type>,
) -> Result<Value> {
    let (struct_ty, fields) = cg
        .record_layout(owner)
        .ok_or_else(|| CodegenError::unknown(owner))?;
    let (index, ty) = fields
        .iter()
        .enumerate()
        .find_map(|(index, (name, ty))| (name == field).then_some((index, *ty)))
        .ok_or_else(|| CodegenError::invalid(format!("`{owner}` has no field `{field}`")))?;
    let ty = inferred.map_or(ty, crate::types::ltype_of);
    let target = crate::cast::coerce_to(cg, target, LType::Ptr)?;
    let source = cg.emit_reg(format!("bitcast i8* {} to {struct_ty}*", target.operand));
    let loaded = super::load_field(cg, &struct_ty, &source, index + 1, ty);
    Ok(restore_field(
        cg,
        Value::new(loaded, ty),
        owner,
        field,
        inferred,
    ))
}

/// Stateful handles retain their element ABI; other fields retain their owner.
fn restore_field(
    cg: &Codegen,
    value: Value,
    owner: &str,
    field: &str,
    inferred: Option<&Type>,
) -> Value {
    if let Some(handle) = inferred
        .and_then(|ty| crate::builder::FiberSig::of(&cg.prog, ty))
        .or_else(|| cg.ctor_field_handle(owner, field))
    {
        return handle.restore(value);
    }
    value.with_owner(inferred.map_or_else(
        || cg.ctor_field_owner(owner, field),
        |ty| crate::types::owner_name(&cg.prog, ty),
    ))
}
