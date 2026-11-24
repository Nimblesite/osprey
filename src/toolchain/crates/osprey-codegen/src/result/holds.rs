//! How a Result block holds its payload and its message, and the layout
//! that follows from it [MEM-OWNERSHIP-STATIC-BORROWS].
use crate::arc::Held;
use crate::builder::Codegen;
use crate::llty::{LType, Value, RESULT_STRUCT};

/// How a Result holds its message or typed error payload: under ownership a
/// counted one moves in when it can and is lent otherwise
/// [MEM-OWNERSHIP-STATIC-BORROWS].
pub(super) fn message_hold(cg: &mut Codegen, errmsg: &str) -> Held {
    let counted = errmsg.starts_with('%') && !cg.is_rodata(errmsg);
    if cg.ownership.on && counted {
        crate::arc::hold_field(cg, &Value::new(errmsg, LType::Str))
    } else {
        Held::Stored
    }
}

pub(super) fn result_meta(cg: &Codegen, inner: LType, message: Held) -> i64 {
    use crate::meta::MetaField;
    // The payload is a physical `i64` on every target even when it carries a
    // managed pointer, so it is a `ManagedWord`, not a pointer-size slot.
    let payload = if inner.is_managed_ptr() {
        MetaField::ManagedWord
    } else {
        MetaField::Word
    };
    // A message the Result only holds stays out of its drop walk.
    let text = if message == Held::Lent {
        MetaField::PtrOpaque
    } else {
        MetaField::PtrManaged
    };
    cg.struct_meta(&[payload, MetaField::Byte, text])
}

pub(super) fn store_status(
    cg: &mut Codegen,
    object: &str,
    disc: &str,
    errmsg: &str,
    message: Held,
) {
    let dp = cg.emit_reg(format!(
        "getelementptr {RESULT_STRUCT}, {RESULT_STRUCT}* {object}, i32 0, i32 1"
    ));
    cg.emit(format!("store i8 {disc}, i8* {dp}"));
    if message == Held::Stored {
        crate::aggregate::store_field(cg, RESULT_STRUCT, object, 2, LType::Str, errmsg);
    } else {
        crate::aggregate::store_held_field(cg, RESULT_STRUCT, object, 2, LType::Str, errmsg);
    }
}

/// Scalar payloads with static reasons hold no managed references. [GC-ARC-PERCEUS]
pub(super) fn own_result(cg: &mut Codegen, value: &Value, inner: LType, errmsg: &str) {
    crate::arc::own(cg, value);
    let errmsg_unmanaged = !errmsg.starts_with('%') || cg.is_rodata(errmsg);
    if !inner.is_managed_ptr() && errmsg_unmanaged {
        crate::arc::mark_pure_scalar(cg, value);
    }
}

/// How a Result holds its payload [MEM-OWNERSHIP-STATIC-BORROWS]. One read
/// out of a collection holds the element lent. Under ownership any other
/// managed payload moves in when it can and is lent otherwise, like a field
/// of a record; every other backend stores it as before.
pub(super) fn payload_hold(cg: &mut Codegen, value: &Value, reading: bool) -> Held {
    if reading {
        Held::Lent
    } else if cg.ownership.on && value.ty.is_managed_ptr() {
        crate::arc::hold_field(cg, value)
    } else {
        Held::Stored
    }
}

pub(super) fn store_payload(cg: &mut Codegen, object: &str, value: &Value, hold: Held) {
    if hold == Held::Stored {
        crate::arc::store_owned(cg, value.ty.as_str(), &value.operand);
    }
    let bits = crate::conv::box_to_i64(cg, value.clone());
    crate::aggregate::store_field(cg, RESULT_STRUCT, object, 0, LType::I64, &bits.operand);
}
