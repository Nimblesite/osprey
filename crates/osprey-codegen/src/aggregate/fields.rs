//! Shared semantic field shapes for construction, selection and pattern projection.
use crate::builder::{Codegen, ParamSig};
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
        .or_else(|| variant_field_type(cg, target, owner, field))
        .or_else(|| cg.ctor_field_ty(owner, field).cloned())
        .filter(|ty| !osprey_types::has_type_var(ty))
}

/// Union variants instantiate against their containing union's arguments.
fn variant_field_type(cg: &Codegen, value: &Value, variant: &str, field: &str) -> Option<Type> {
    let Type::Con { name, args } = value.inferred_type.as_ref()? else {
        return None;
    };
    if cg.prog.ctors.get(variant)?.owner != *name {
        return None;
    }
    let instantiated = Type::Con {
        name: variant.to_string(),
        args: args.clone(),
    };
    cg.prog.field_type(&instantiated, field)
}

pub(crate) fn gen_field_access(cg: &mut Codegen, target: &Expr, field: &str) -> Result<Value> {
    let target = crate::expr::gen_expr(cg, target)?;
    let owner = field_owner(cg, &target, field)?;
    let inferred = field_type(cg, &target, &owner, field);
    load_field_value(cg, target, &owner, field, inferred.as_ref())
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
    let (index, ty) = field_slot(&fields, owner, field)?;
    let target = crate::cast::coerce_to(cg, target, LType::Ptr)?;
    let source = cg.emit_reg(format!("bitcast i8* {} to {struct_ty}*", target.operand));
    let loaded = super::load_record_field(cg, owner, &struct_ty, &source, index, ty);
    Ok(restore_field(
        cg,
        Value::new(loaded, ty),
        owner,
        field,
        inferred,
    ))
}

fn field_slot(fields: &[(String, LType)], owner: &str, field: &str) -> Result<(usize, LType)> {
    fields
        .iter()
        .enumerate()
        .find_map(|(index, (name, ty))| (name == field).then_some((index, *ty)))
        .ok_or_else(|| CodegenError::invalid(format!("`{owner}` has no field `{field}`")))
}

/// Fields retain the same semantic shape as parameters. [TYPE-RECORD-RESULT]
pub(super) fn shape_of(cg: &Codegen, ty: &Type) -> Value {
    let sig = ParamSig::of(&cg.prog, ty);
    let mut value = match sig.result_inner {
        Some(inner) => Value::result("", inner)
            .with_payload_owner(crate::types::result_payload_owner(&cg.prog, ty)),
        None => Value::new("", sig.ty).with_owner(crate::types::owner_name(&cg.prog, ty)),
    };
    value.inferred_type = sig.inferred_type;
    match sig.fiber {
        Some(handle) => handle.restore(value),
        None => value,
    }
}

fn field_shape(cg: &Codegen, owner: &str, field: &str, inferred: Option<&Type>) -> Option<Value> {
    inferred
        .filter(|ty| !osprey_types::has_type_var(ty))
        .map(|ty| shape_of(cg, ty))
        .or_else(|| {
            cg.obj_layout(owner)?
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, shape)| shape.clone())
        })
        .or_else(|| cg.ctor_field_ty(owner, field).map(|ty| shape_of(cg, ty)))
}

/// Recover Results, callable signatures and handle element types at every read.
pub(crate) fn restore_field(
    cg: &mut Codegen,
    value: Value,
    owner: &str,
    field: &str,
    inferred: Option<&Type>,
) -> Value {
    let Some(mut shape) = field_shape(cg, owner, field, inferred) else {
        return value;
    };
    shape.osp_ty = shape.osp_ty.or_else(|| cg.ctor_field_owner(owner, field));
    shape.operand = restored_operand(cg, value, &shape);
    shape
}

fn restored_operand(cg: &mut Codegen, value: Value, shape: &Value) -> String {
    let value = if value.ty == LType::I64 && shape.ty != LType::I64 {
        crate::conv::unbox_from_i64(cg, &value.operand, shape.ty)
    } else {
        value
    };
    if shape.result_inner.is_some() {
        cg.emit_reg(format!(
            "bitcast i8* {} to {}",
            value.operand,
            shape.llvm_ty()
        ))
    } else {
        value.operand
    }
}

/// A Result slot promotes a bare payload once and preserves an existing failure.
pub(super) fn coerce_field(
    cg: &mut Codegen,
    value: Value,
    owner: &str,
    field: &str,
    inferred: Option<&Type>,
) -> Result<Value> {
    let Some(shape) = field_shape(cg, owner, field, inferred) else {
        return Ok(value);
    };
    let sig = field_signature(shape, &value);
    crate::cast::coerce_semantic_param(cg, value, &sig)
}

fn field_signature(shape: Value, value: &Value) -> ParamSig {
    let generic = shape
        .inferred_type
        .as_ref()
        .is_some_and(osprey_types::has_type_var);
    let inner = match shape.result_inner {
        Some(_) if generic_payload(shape.inferred_type.as_ref()) => {
            Some(actual_payload_type(value))
        }
        None if generic => value.result_inner,
        other => other,
    };
    ParamSig {
        ty: if generic { value.ty } else { shape.ty },
        result_inner: inner,
        fiber: None,
        inferred_type: shape.inferred_type,
    }
}

fn generic_payload(ty: Option<&Type>) -> bool {
    match ty {
        Some(Type::Con { name, args }) if name == osprey_types::names::RESULT => {
            args.first().is_some_and(osprey_types::has_type_var)
        }
        _ => false,
    }
}

fn actual_payload_type(value: &Value) -> LType {
    match value.result_inner {
        Some(inner) => inner,
        None => value.ty,
    }
}
