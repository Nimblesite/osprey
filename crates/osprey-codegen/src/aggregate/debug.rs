//! Debug types consume the same physical and semantic record layouts as field reads.
use super::{fields::field_type, record_block, Codegen, LType, Value};

pub(crate) struct DebugRecord {
    pub fields: Vec<(String, Value)>,
    pub tagged: bool,
}

pub(crate) fn record_fields(cg: &mut Codegen, value: &Value) -> Option<DebugRecord> {
    let owner = value.osp_ty.as_deref()?;
    if cg
        .ctor_layout(owner)
        .is_some_and(|view| !view.owner_is_record)
    {
        return None;
    }
    let block = record_block(cg, owner, value.inferred_type.as_ref())?;
    Some(DebugRecord {
        fields: block
            .fields
            .into_iter()
            .map(|(name, ty)| {
                let field = debug_field(cg, value, owner, &name, ty);
                (name, field)
            })
            .collect(),
        tagged: super::record_has_tag(owner),
    })
}

fn debug_field(cg: &Codegen, value: &Value, owner: &str, name: &str, ty: LType) -> Value {
    let inferred = field_type(cg, value, owner, name);
    let mut field = Value::new("", ty).with_owner(
        inferred
            .as_ref()
            .and_then(|ty| crate::types::owner_name(&cg.prog, ty))
            .or_else(|| cg.ctor_field_owner(owner, name)),
    );
    field.inferred_type = inferred;
    field
}
