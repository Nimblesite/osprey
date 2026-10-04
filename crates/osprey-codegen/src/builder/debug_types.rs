//! Native record metadata follows the compiler's actual layout. [DEBUGGER-RECORD-VALUES]
use super::{metadata_escape, Codegen, DebugState, LType, Value};
use crate::aggregate::DebugRecord;
use crate::meta::{field_offsets, MetaField};

#[derive(Clone, Copy)]
struct RecordIds {
    composite: usize,
    pointer: usize,
    members: usize,
}

impl Codegen {
    pub(super) fn debug_value_type(&mut self, value: &Value) -> Option<usize> {
        let primitive = self.debug.as_ref()?.debug_type_id(value.ty);
        if value.ty != LType::Ptr || value.result_inner.is_some() {
            return Some(primitive);
        }
        let key = format!("{:?}:{:?}", value.osp_ty, value.inferred_type);
        if let Some(id) = self.debug.as_ref()?.record_types.get(&key) {
            return Some(*id);
        }
        let Some(fields) = crate::aggregate::debug_record_fields(self, value) else {
            return Some(primitive);
        };
        self.debug_record_type(value, key, &fields)
    }

    fn debug_record_type(
        &mut self,
        value: &Value,
        key: String,
        record: &DebugRecord,
    ) -> Option<usize> {
        let layout: Vec<_> = record
            .tagged
            .then_some(MetaField::Word)
            .into_iter()
            .chain(
                record
                    .fields
                    .iter()
                    .map(|(_, field)| MetaField::of_lty(field.ty)),
            )
            .collect();
        let size = field_offsets(&layout)
            .last()
            .map_or(8, |(offset, size)| (offset + size).div_ceil(8) * 8);
        let ids = self
            .debug
            .as_mut()?
            .begin_record(key, &record_name(value), size);
        let offsets = field_offsets(&layout).skip(usize::from(record.tagged));
        let members = self.debug_record_members(ids.composite, &record.fields, offsets)?;
        self.debug.as_mut()?.finish_record(ids.members, &members);
        Some(ids.pointer)
    }

    fn debug_record_members(
        &mut self,
        scope: usize,
        fields: &[(String, Value)],
        offsets: impl Iterator<Item = (u64, u64)>,
    ) -> Option<Vec<usize>> {
        fields
            .iter()
            .zip(offsets)
            .map(|((name, field), (offset, size))| {
                let ty = self.debug_value_type(field)?;
                Some(
                    self.debug
                        .as_mut()?
                        .record_member(scope, name, ty, offset, size),
                )
            })
            .collect()
    }
}

fn record_name(value: &Value) -> String {
    let owner = value.osp_ty.as_deref().map_or("record", |name| name);
    let base = owner.split('#').next().map_or(owner, |name| name);
    let base = if base.starts_with("__obj_") {
        "record"
    } else {
        base
    };
    let name = match &value.inferred_type {
        Some(ty @ osprey_types::Type::Con { args, .. }) if !args.is_empty() => ty.to_string(),
        Some(ty) if owner.contains('#') || owner.starts_with("__obj_") => format!("{base} {ty}"),
        None if owner.contains('#') || owner.starts_with("__obj_") => owner.to_string(),
        _ => base.to_string(),
    };
    osprey_ast::symbol::demangle_message(&name).into_owned()
}

impl DebugState {
    fn begin_record(&mut self, key: String, name: &str, size: u64) -> RecordIds {
        let ids = RecordIds {
            composite: self.alloc_id(),
            pointer: self.alloc_id(),
            members: self.alloc_id(),
        };
        let _ = self.record_types.insert(key, ids.pointer);
        self.record_definition(ids, name, size);
        ids
    }

    fn record_definition(&mut self, ids: RecordIds, name: &str, size: u64) {
        let name = metadata_escape(name);
        let RecordIds {
            composite,
            pointer,
            members,
        } = ids;
        self.dynamic.push((composite, format!(
            "!{composite} = distinct !DICompositeType(tag: DW_TAG_structure_type, name: \"{name}\", file: !{}, size: {}, align: 64, elements: !{members})",
            self.file_id, size * 8)));
        self.dynamic.push((pointer, format!(
            "!{pointer} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{composite}, size: 64)")));
    }

    fn record_member(
        &mut self,
        scope: usize,
        name: &str,
        ty: usize,
        offset: u64,
        size: u64,
    ) -> usize {
        let id = self.alloc_id();
        let name = metadata_escape(name);
        self.dynamic.push((id, format!(
            "!{id} = !DIDerivedType(tag: DW_TAG_member, name: \"{name}\", scope: !{scope}, file: !{}, baseType: !{ty}, size: {}, offset: {})",
            self.file_id, size * 8, offset * 8)));
        id
    }

    fn finish_record(&mut self, id: usize, members: &[usize]) {
        let members = members
            .iter()
            .map(|member| format!("!{member}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.dynamic.push((id, format!("!{id} = !{{{members}}}")));
    }
}
