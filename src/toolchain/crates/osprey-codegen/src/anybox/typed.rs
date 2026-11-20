//! Typed `Error` payloads. A payload that is not a string travels in the
//! Result's message slot as a SHALLOW box `{ desc, payload }`: the value's own
//! word, under a descriptor that renders it at its static type. The box is a
//! valid erased value, so a reader that does not know the error type renders
//! it truthfully, and one that does reads the word back. Implements
//! [ERR-PAYLOAD].

use super::{build_box, desc_operand, descriptor, open_box, DescKey};
use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};

/// Box `v` whole at its static type.
pub(crate) fn box_typed(cg: &mut Codegen, v: Value) -> Result<Value> {
    let v = crate::listlit::escaping(cg, v);
    let proto = Value {
        operand: String::new(),
        ..v.clone()
    };
    let key = format!("{proto:?}");
    let _ = cg.anys.typed.insert(key.clone(), proto);
    let desc = descriptor(cg, &DescKey::Typed(key))?;
    let managed = v.result_inner.is_some() || v.ty.is_managed_ptr();
    let word = crate::conv::box_to_i64(cg, v);
    let payload = if managed {
        crate::conv::unbox_from_i64(cg, &word.operand, LType::Ptr)
    } else {
        word
    };
    Ok(build_box(cg, &desc_operand(&desc), payload, managed))
}

/// The payload word of a typed box.
pub(crate) fn typed_word(cg: &mut Codegen, boxed: &str) -> String {
    open_box(cg, &Value::new(boxed, LType::Any)).1
}

/// The value `proto` describes, read back from its payload `word`.
pub(super) fn restore(cg: &mut Codegen, word: &str, proto: &Value) -> Value {
    let operand = if proto.result_inner.is_some() {
        cg.emit_reg(format!(
            "inttoptr i64 {word} to {}",
            crate::llty::result_pointer()
        ))
    } else {
        crate::conv::unbox_from_i64(cg, word, proto.ty).operand
    };
    Value {
        operand,
        ..proto.clone()
    }
}
