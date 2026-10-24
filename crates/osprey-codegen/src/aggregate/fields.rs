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
        .filter(|ty| !osprey_types::has_type_var(ty))
        .or_else(|| {
            variant_field_type(cg, target, owner, field)
                .filter(|ty| !osprey_types::has_type_var(ty))
        })
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

/// The receiver's record: the owner codegen tracked, else the record inference
/// named (a `perform` result carries only its inferred type), else a guess by
/// field name. A row or a generic instantiation inference named registers its
/// derived layout on first sight, so a read emitted BEFORE any construction
/// runs — a helper the checker resolved to one concrete type — shares the
/// layout that construction registers instead of being quarantined by a
/// field name two records declare (#273). Implements
/// [TYPE-FIELD-ACCESS-NON-RECORD] [TYPE-GENERICS-DECL].
fn field_owner(cg: &mut Codegen, target: &Value, field: &str) -> Result<String> {
    let inferred = target.inferred_type.as_ref();
    let named = inferred.and_then(|ty| crate::types::owner_name(&cg.prog, ty));
    if let (Some(owner), Some(ty)) = (named.as_deref(), inferred) {
        let _ = super::record_block(cg, owner, Some(ty));
        if matches!(ty, Type::Record { name, .. } if name.is_empty()) {
            return Ok(owner.to_string());
        }
    }
    [target.osp_ty.clone(), named]
        .into_iter()
        .flatten()
        .find(|owner| declares_field(cg, owner, field))
        .map_or_else(|| guessed_field_owner(cg, target, field), Ok)
}

fn declares_field(cg: &Codegen, owner: &str, field: &str) -> bool {
    cg.record_layout(owner)
        .is_some_and(|(_, fields)| fields.iter().any(|(name, _)| name == field))
}

/// Broken-code quarantine: a receiver whose record neither codegen tracked nor
/// inference named falls back to choosing a record by field name. Where several
/// records declare that field the choice is arbitrary and the load would read
/// another record's slot without a diagnostic, so the access fails loudly.
fn guessed_field_owner(cg: &Codegen, target: &Value, field: &str) -> Result<String> {
    let owners = cg
        .prog
        .ctors
        .values()
        .filter(|ctor| ctor.fields.iter().any(|(name, _)| name == field))
        .count();
    if owners > 1 {
        let receiver = target
            .inferred_type
            .as_ref()
            .map_or_else(|| "an untracked receiver".to_string(), ToString::to_string);
        return Err(CodegenError::unsupported(format!(
            "quarantined field access `{field}` on {receiver}: {owners} records declare it and the receiver's record was not tracked"
        )));
    }
    cg.find_field_owner(field)
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

#[cfg(test)]
mod tests {
    use super::*;
    use osprey_types::{CtorLayout, ProgramTypes};

    fn record(owner: &str, fields: &[&str]) -> CtorLayout {
        CtorLayout {
            owner: owner.to_string(),
            owner_is_record: true,
            type_params: Vec::new(),
            fields: fields
                .iter()
                .map(|name| ((*name).to_string(), Type::con("int", Vec::new())))
                .collect(),
        }
    }

    fn generic_record(owner: &str, fields: &[(&str, Type)]) -> CtorLayout {
        CtorLayout {
            owner: owner.to_string(),
            owner_is_record: true,
            type_params: vec!["A".to_string()],
            fields: fields
                .iter()
                .map(|(n, t)| ((*n).to_string(), t.clone()))
                .collect(),
        }
    }

    fn quarantine(field: &str, receiver: &str) -> CodegenError {
        CodegenError::unsupported(format!(
            "quarantined field access `{field}` on {receiver}: 2 records declare it and the receiver's record was not tracked"
        ))
    }

    /// [TYPE-FIELD-ACCESS-NON-RECORD]: with two records declaring `x`, a guess
    /// by field name could read the other record's slot, so it must fail.
    #[test]
    fn a_shared_field_on_an_untracked_receiver_fails_loudly() {
        let ctors = [
            record("Point", &["x", "y"]),
            record("Pixel", &["x", "shade"]),
        ];
        let mut prog = ProgramTypes::default();
        prog.ctors
            .extend(ctors.into_iter().map(|ctor| (ctor.owner.clone(), ctor)));
        let cg = Codegen::with_types(prog);
        let untracked = Value::new("%receiver", LType::Ptr);
        assert_eq!(
            guessed_field_owner(&cg, &untracked, "x"),
            Err(quarantine("x", "an untracked receiver"))
        );
        let typed = Value {
            inferred_type: Some(Type::con("int", Vec::new())),
            ..untracked
        };
        assert_eq!(
            guessed_field_owner(&cg, &typed, "x"),
            Err(quarantine("x", "int"))
        );
    }

    /// [TYPE-GENERICS-DECL]: a read in a function emitted BEFORE any
    /// construction runs derives the instantiation's layout from the
    /// declaration, so two generic records sharing a field name no longer
    /// quarantine a receiver whose type inference named (#273).
    #[test]
    fn a_typed_receiver_derives_its_generic_layout_before_any_construction() {
        let string = Type::con("string", Vec::new());
        let ctors = [
            generic_record("Tab", &[("name", string.clone()), ("child", Type::Var(0))]),
            generic_record("Row", &[("key", string.clone()), ("child", Type::Var(0))]),
        ];
        let mut prog = ProgramTypes::default();
        prog.ctors
            .extend(ctors.into_iter().map(|ctor| (ctor.owner.clone(), ctor)));
        let mut cg = Codegen::with_types(prog);
        let tab_receiver = Type::con("Tab", vec![string.clone()]);
        let receiver = Value {
            inferred_type: Some(tab_receiver),
            ..Value::new("%receiver", LType::Ptr)
        };
        assert_eq!(
            field_owner(&mut cg, &receiver, "child"),
            Ok("Tab#i8*,i8*".to_string())
        );
        // The Row instantiation derives independently, at its own key.
        let row = Value {
            inferred_type: Some(Type::con("Row", vec![string])),
            ..Value::new("%row", LType::Ptr)
        };
        assert_eq!(
            field_owner(&mut cg, &row, "child"),
            Ok("Row#i8*,i8*".to_string())
        );
    }
}
