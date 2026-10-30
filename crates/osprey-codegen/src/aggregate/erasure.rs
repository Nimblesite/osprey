//! Rebuild nominal values when covariance changes their erased payload representation.
//! Implements [TYPE-ANY] [TYPE-VARIANCE-COERCION]; source aliases keep their layout.
use crate::builder::{Codegen, ParamSig};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_types::{names, Type};

struct Conversion {
    source: Type,
    target: Type,
    owner: String,
}

type Fields = Vec<(String, Value)>;

pub(crate) fn contains_any(ty: &Type) -> bool {
    match ty {
        Type::Con { name, args } if name == names::RESULT => args.first().is_some_and(contains_any),
        Type::Con { name, args } => name == names::ANY || args.iter().any(contains_any),
        Type::Record { fields, .. } => fields.values().any(contains_any),
        Type::Union { variants, .. } => variants.iter().any(contains_any),
        Type::Fun { params, ret } => params.iter().any(contains_any) || contains_any(ret),
        Type::Var(_) => false,
    }
}

pub(crate) fn adapt(cg: &mut Codegen, value: Value, want: &ParamSig) -> Result<Value> {
    let Some(conversion) = conversion(cg, &value, want) else {
        return Ok(value);
    };
    let name = converter(cg, &conversion)?;
    let source = crate::arc::as_i8ptr(cg, &value);
    let operand = cg.emit_reg(format!("call i8* @{name}(i8* {source})"));
    let mut output = Value::handle(operand, conversion.owner);
    output.inferred_type = Some(conversion.target);
    crate::arc::own(cg, &output);
    Ok(output)
}

fn conversion(cg: &Codegen, value: &Value, want: &ParamSig) -> Option<Conversion> {
    let target = want.inferred_type.as_ref()?;
    let source = value.inferred_type.as_ref()?;
    if !(contains_any(source) || contains_any(target))
        || crate::types::abi_spelling(source) == crate::types::abi_spelling(target)
    {
        return None;
    }
    let owner = crate::types::owner_name(&cg.prog, target)?;
    let nominal = cg.union_variants(&owner).is_some()
        || matches!(target, Type::Record { .. })
        || nominal_record(cg, target);
    nominal.then(|| Conversion {
        source: source.clone(),
        target: target.clone(),
        owner,
    })
}

fn nominal_record(cg: &Codegen, target: &Type) -> bool {
    let Type::Con { name, .. } = target else {
        return false;
    };
    cg.prog
        .ctors
        .get(name)
        .is_some_and(|ctor| ctor.owner_is_record)
}

fn converter(cg: &mut Codegen, conversion: &Conversion) -> Result<String> {
    let key = (
        crate::types::abi_spelling(&conversion.source),
        crate::types::abi_spelling(&conversion.target),
    );
    if let Some(name) = cg.nominal_adapters.get(&key) {
        return Ok(name.clone());
    }
    let name = format!("__nominal_adapter_{}", cg.next_lambda_id());
    let _ = cg.nominal_adapters.insert(key, name.clone());
    define(cg, &name, conversion)?;
    Ok(name)
}

fn define(cg: &mut Codegen, name: &str, conversion: &Conversion) -> Result<()> {
    let saved = cg.enter_nested_fn();
    let source_sig = ParamSig::of(&cg.prog, &conversion.source);
    let source = crate::cast::incoming_param(cg, "%__source".into(), source_sig, None);
    let emitted = body(cg, &source, conversion);
    if let Ok(value) = &emitted {
        crate::arc::epilogue(cg, Some(value));
        cg.emit(format!("ret i8* {}", value.operand));
    }
    cg.exit_nested_fn(saved, "i8*", name, &[(LType::Ptr, "__source".into())]);
    emitted.map(|_| ())
}

fn body(cg: &mut Codegen, source: &Value, conversion: &Conversion) -> Result<Value> {
    if let Some(variants) = cg.union_variants(&conversion.owner) {
        let variants = variants.to_vec();
        union(cg, source, conversion, &variants)
    } else {
        record(cg, source, conversion)
    }
}

fn record(cg: &mut Codegen, source: &Value, conversion: &Conversion) -> Result<Value> {
    let owner = source
        .osp_ty
        .as_deref()
        .ok_or_else(|| CodegenError::invalid("record erasure requires its source layout"))?;
    let values = fields(cg, source, owner, &conversion.target)?;
    let owner = cg.register_layout(conversion.owner.clone(), super::layout_of(&values));
    let mut output = super::build_tagged_object(cg, owner, &values);
    output.inferred_type = Some(conversion.target.clone());
    Ok(output)
}

fn fields(cg: &mut Codegen, source: &Value, owner: &str, target: &Type) -> Result<Fields> {
    let block = super::record_block(cg, owner, source.inferred_type.as_ref()).ok_or_else(|| {
        CodegenError::invalid(format!("missing source layout `{owner}` during erasure"))
    })?;
    let pointer = cg.emit_reg(format!(
        "bitcast i8* {} to {}*",
        source.operand, block.struct_ty
    ));
    block
        .fields
        .iter()
        .enumerate()
        .map(|(index, (name, ty))| {
            let raw = super::load_record_field(cg, owner, &block.struct_ty, &pointer, index, *ty);
            let value = field(cg, source, owner, name, Value::new(raw, *ty), target)?;
            Ok((name.clone(), value))
        })
        .collect()
}

fn field(
    cg: &mut Codegen,
    source: &Value,
    owner: &str,
    name: &str,
    raw: Value,
    target: &Type,
) -> Result<Value> {
    let inferred = super::field_type(cg, source, owner, name);
    let loaded = super::restore_field(cg, raw, owner, name, inferred.as_ref());
    let signature = target_field(cg, target, owner, name)?;
    crate::cast::coerce_semantic_param(cg, loaded, &signature)
}

fn target_field(cg: &Codegen, target: &Type, owner: &str, name: &str) -> Result<ParamSig> {
    let mut wanted = Value::handle("", owner);
    wanted.inferred_type = Some(target.clone());
    let ty = super::field_type(cg, &wanted, owner, name).ok_or_else(|| {
        CodegenError::invalid(format!(
            "missing target type for `{owner}.{name}` during erasure"
        ))
    })?;
    Ok(ParamSig::of(&cg.prog, &ty))
}

fn union(
    cg: &mut Codegen,
    source: &Value,
    conversion: &Conversion,
    variants: &[String],
) -> Result<Value> {
    let pointer = cg.emit_reg(format!("bitcast i8* {} to i64*", source.operand));
    let tag = cg.emit_reg(format!("load i64, i64* {pointer}"));
    let slot = cg.hoist_arc_slot();
    let end = cg.fresh_label();
    for (index, variant) in variants.iter().enumerate() {
        let matched = cg.emit_reg(format!("icmp eq i64 {tag}, {index}"));
        let next = cg.open_guard(&matched);
        branch(cg, source, variant, &conversion.target, &slot, &end)?;
        cg.start_block(&next);
    }
    cg.emit("unreachable");
    Ok(join_union(cg, &slot, &end, conversion))
}

fn join_union(cg: &mut Codegen, slot: &str, end: &str, conversion: &Conversion) -> Value {
    cg.start_block(end);
    let mut output = Value::handle(
        cg.emit_reg(format!("load i8*, i8** {slot}")),
        &conversion.owner,
    );
    output.inferred_type = Some(conversion.target.clone());
    crate::arc::own(cg, &output);
    output
}

fn branch(
    cg: &mut Codegen,
    source: &Value,
    variant: &str,
    target: &Type,
    slot: &str,
    end: &str,
) -> Result<()> {
    crate::arc::push_frame(cg, crate::arc::FrameKind::Scope);
    let value = variant_value(cg, source, variant, target)?;
    crate::arc::escape_retain(cg, &value);
    cg.emit(format!("store i8* {}, i8** {slot}", value.operand));
    crate::arc::pop_frame(cg);
    let _ = cg.snapshot_to(end);
    Ok(())
}

fn variant_value(cg: &mut Codegen, source: &Value, variant: &str, target: &Type) -> Result<Value> {
    let view = cg
        .ctor_layout(variant)
        .ok_or_else(|| CodegenError::unknown(variant))?;
    let fields = fields(cg, source, variant, target)?;
    let values = fields
        .into_iter()
        .zip(&view.fields)
        .map(|((_, value), (_, ty))| {
            let stored = crate::cast::erase_result(cg, value);
            super::construct::storage_value(cg, stored, *ty)
        })
        .collect::<Result<Vec<_>>>()?;
    super::construct::materialize(cg, variant, view, &values)
}
