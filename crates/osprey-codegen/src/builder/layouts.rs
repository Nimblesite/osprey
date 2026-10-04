//! Emitter layouts.
use super::{Codegen, LType, ObjField};

impl Codegen {
    /// Register an anonymous object literal's ordered field layout and return the
    /// synthetic owner name to tag its handle with.
    pub(crate) fn register_obj_layout(&mut self, fields: Vec<ObjField>) -> String {
        let name = format!("__obj_{}", self.obj_count);
        self.obj_count += 1;
        self.register_layout(name, fields)
    }

    /// Register an ordered field layout under a caller-chosen owner name and
    /// return it. A generic record instantiation names its layout after the
    /// concrete field types it was built with, so every construction of the
    /// same instantiation shares one owner ([`crate::aggregate`]).
    pub(crate) fn register_layout(&mut self, name: String, fields: Vec<ObjField>) -> String {
        let _ = self.obj_layouts.insert(name.clone(), fields);
        name
    }

    /// The registered slots of a synthetic owner — an object literal or a
    /// generic-record instantiation — `None` for a declared constructor.
    pub(crate) fn obj_layout(&self, owner: &str) -> Option<&[ObjField]> {
        self.obj_layouts.get(owner).map(Vec::as_slice)
    }

    /// The struct spelling and ordered fields of an owner — a real constructor or
    /// a synthetic object literal — for unified field access.
    pub(crate) fn record_layout(&self, owner: &str) -> Option<(String, Vec<(String, LType)>)> {
        if let Some(fields) = self.obj_layouts.get(owner) {
            let mut parts = vec!["i64".to_string()];
            parts.extend(fields.iter().map(|(_, lt, _)| lt.as_str().to_string()));
            let slots = fields.iter().map(|(f, lt, _)| (f.clone(), *lt)).collect();
            return Some((format!("{{ {} }}", parts.join(", ")), slots));
        }
        let view = self.ctor_layout(owner)?;
        Some((self.ctor_struct_ty(owner)?, view.fields))
    }
}
