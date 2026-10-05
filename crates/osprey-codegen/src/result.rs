//! The `Result<T, E>` ABI: a heap block `{ i64 bits, i8 disc, i8* errmsg }`
//! reached by pointer, `disc == 0` ⇒ Success. `value` (slot 0) carries the
//! success payload bits; `errmsg` (slot 2) carries the Error-arm message as a
//! null-terminated `i8*` (`null` when there is none). The builders here
//! construct that block; the readers branch on or load out of it. Runtime
//! fallible builtins (list/map get, string ops) and user functions declared
//! `-> Result<…>` both produce this shape, so match, `?:`, aggregate fields
//! and rendering handle exactly one representation. Implements
//! [ERR-PAYLOAD].

use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::error::Result;
use crate::llty::{LType, Value, RESULT_STRUCT};

/// A literal `null` `i8*` — the errmsg slot of a Success (or message-less Error).
pub(crate) const NO_MSG: &str = "null";

/// Build a `Result` block with the given success `value`, an explicit `i8`
/// discriminant operand (`"0"` Success, `"1"` Error, or an `i8` register from a
/// `select`), and an `i8*` `errmsg` operand (`NO_MSG` for none). The value is
/// coerced to `inner` before storing.
pub(crate) fn make_result(
    cg: &mut Codegen,
    value: Value,
    inner: LType,
    disc: &str,
    errmsg: &str,
) -> Result<Value> {
    let value = coerce_to(cg, value, inner)?;
    let obj = cg.malloc_struct(RESULT_STRUCT, result_meta(inner));
    store_payload(cg, &obj, &value);
    store_status(cg, &obj, disc, errmsg);
    let mut out = Value::result(obj, inner).with_payload_owner(value.osp_ty);
    out.result_payload_type = value.inferred_type;
    own_result(cg, &out, inner, errmsg);
    Ok(out)
}

fn result_meta(inner: LType) -> i64 {
    use crate::meta::MetaField;
    let payload = if inner.is_managed_ptr() {
        MetaField::PtrManaged
    } else {
        MetaField::Word
    };
    crate::meta::struct_meta(&[payload, MetaField::Byte, MetaField::PtrManaged])
}

fn store_status(cg: &mut Codegen, object: &str, disc: &str, errmsg: &str) {
    let dp = cg.emit_reg(format!(
        "getelementptr {RESULT_STRUCT}, {RESULT_STRUCT}* {object}, i32 0, i32 1"
    ));
    cg.emit(format!("store i8 {disc}, i8* {dp}"));
    crate::aggregate::store_field(cg, RESULT_STRUCT, object, 2, LType::Str, errmsg);
}

/// Scalar payloads with static reasons hold no managed references. [GC-ARC-PERCEUS]
fn own_result(cg: &mut Codegen, value: &Value, inner: LType, errmsg: &str) {
    crate::arc::own(cg, value);
    let errmsg_unmanaged = !errmsg.starts_with('%') || cg.is_rodata(errmsg);
    if !inner.is_managed_ptr() && errmsg_unmanaged {
        crate::arc::mark_pure_scalar(cg, value);
    }
}

fn store_payload(cg: &mut Codegen, object: &str, value: &Value) {
    crate::arc::dup_store(cg, value.ty.as_str(), &value.operand);
    let bits = crate::conv::box_to_i64(cg, value.clone());
    crate::aggregate::store_field(cg, RESULT_STRUCT, object, 0, LType::I64, &bits.operand);
}

/// A Success result wrapping `value` (disc 0, no message).
pub(crate) fn make_ok(cg: &mut Codegen, value: Value, inner: LType) -> Result<Value> {
    make_result(cg, value, inner, "0", NO_MSG)
}

/// Build a `Result` whose discriminant is Error when `is_err` (an `i1` operand)
/// holds — folding the ubiquitous `select i1 …, i8 1, i8 0` then [`make_result`]
/// that every fallible runtime builtin ends with. `msg` is a static message
/// stored on the error path only (selected to `null` on success); pass `None`
/// to leave the errmsg slot empty.
pub(crate) fn make_result_if_err(
    cg: &mut Codegen,
    value: Value,
    inner: LType,
    is_err: &str,
    msg: Option<&str>,
) -> Result<Value> {
    make_result_if_err_because(cg, value, inner, is_err, msg, None)
}

/// [`make_result_if_err`] with a RUNTIME reason: `reason` is an `i8*` operand
/// (null when the producer recorded none) that outranks the static `msg`. This
/// is how a failed builtin's real cause — "writeFile: out/x.db: No such file or
/// directory" — reaches `Error { message }` instead of the placeholder the
/// static fallback carries. Implements [BUILTIN-FILE-ERRMSG].
pub(crate) fn make_result_if_err_because(
    cg: &mut Codegen,
    value: Value,
    inner: LType,
    is_err: &str,
    msg: Option<&str>,
    reason: Option<&str>,
) -> Result<Value> {
    let disc = cg.emit_reg(format!("select i1 {is_err}, i8 1, i8 0"));
    let fallback = match msg {
        Some(m) => cg.string_constant(m).operand,
        None => NO_MSG.to_string(),
    };
    let chosen = match reason {
        Some(r) => {
            let given = cg.emit_reg(format!("icmp ne i8* {r}, null"));
            cg.emit_reg(format!("select i1 {given}, i8* {r}, i8* {fallback}"))
        }
        None => fallback,
    };
    let errmsg = if chosen == NO_MSG {
        NO_MSG.to_string()
    } else {
        cg.emit_reg(format!("select i1 {is_err}, i8* {chosen}, i8* null"))
    };
    make_result(cg, value, inner, &disc, &errmsg)
}

/// `Result<int, _>` from a C `i64` whose negative values signal failure — the
/// uniform convention of the file/process/HTTP/JSON runtime (a negative handle,
/// byte count, status or process id is Error). The success value carried is the
/// result itself; `msg` is a static fallback message and `reason` the runtime
/// one the call recorded, which wins when present.
pub(crate) fn result_from_i64(
    cg: &mut Codegen,
    result: &str,
    msg: Option<&str>,
    reason: Option<&str>,
) -> Result<Value> {
    let err = cg.emit_reg(format!("icmp slt i64 {result}, 0"));
    make_result_if_err_because(
        cg,
        Value::new(result, LType::I64),
        LType::I64,
        &err,
        msg,
        reason,
    )
}

/// `Result<string, _>` from a possibly-NULL C `char*` (`ptr` an `i8*` operand):
/// NULL ⇒ Error, else Success. The success slot keeps the pointer itself. The
/// errmsg slot takes `reason` — what the runtime recorded about THIS call —
/// falling back to the static `err` text only when the producer recorded
/// nothing, so `Error { message }` and `toString` never show a placeholder for
/// a failure whose real cause is known.
pub(crate) fn result_from_nullable(
    cg: &mut Codegen,
    ptr: &str,
    err: Option<&str>,
    reason: Option<&str>,
) -> Result<Value> {
    let is_null = cg.emit_reg(format!("icmp eq i8* {ptr}, null"));
    make_result_if_err_because(
        cg,
        Value::new(ptr, LType::Str),
        LType::Str,
        &is_null,
        err,
        reason,
    )
}

/// Branch on a Result's discriminant: load it, test `== 0` (Success), and emit
/// the conditional branch to fresh `(success, error, end)` labels — leaving the
/// builder positioned at the start of the `success` block. The shared preamble
/// of every "do one thing on Success, another on Error, `phi` the results" path.
pub(crate) fn open_result_branch(cg: &mut Codegen, v: &Value) -> (String, String, String) {
    let d = load_disc(cg, v);
    let is_succ = cg.emit_reg(format!("icmp eq i8 {d}, 0"));
    let (sl, el, end) = cg.diamond(&is_succ);
    cg.start_block(&sl);
    (sl, el, end)
}

/// Load a Result block's `i8` discriminant operand. Invariant: `v` is a Result
/// (callers gate on `result_inner.is_some()`); a non-Result yields the Error
/// discriminant `1` rather than panicking.
pub(crate) fn load_disc(cg: &mut Codegen, v: &Value) -> String {
    let Some(struct_ty) = v.result_struct_ty() else {
        return "1".to_string();
    };
    let dp = cg.emit_reg(format!(
        "getelementptr {struct_ty}, {struct_ty}* {}, i32 0, i32 1",
        v.operand
    ));
    let d = cg.emit_reg(format!("load i8, i8* {dp}"));
    d
}

/// Load a Result block's success payload as its inner [`LType`]. Invariant: `v`
/// is a Result; a non-Result yields Unit rather than panicking.
pub(crate) fn load_value(cg: &mut Codegen, v: &Value) -> Value {
    let Some(inner) = v.result_inner else {
        return Value::unit();
    };
    let struct_ty = RESULT_STRUCT;
    let bits = crate::aggregate::load_field(cg, struct_ty, &v.operand, 0, LType::I64);
    let mut value =
        crate::conv::unbox_from_i64(cg, &bits, inner).with_owner(v.payload_owner.clone());
    value.inferred_type = v
        .element_type(osprey_types::names::RESULT)
        .or_else(|| v.result_payload_type.clone());
    value
}

/// Load a Result block's raw error-message pointer (slot 2) as an `i8*` — `null`
/// when the producer stored no message. Invariant: `v` is a Result; a non-Result
/// yields `null`. `toString` distinguishes the null case to print a bare `Error`.
pub(crate) fn load_errmsg(cg: &mut Codegen, v: &Value) -> Value {
    let Some(_) = v.result_inner else {
        return Value::new(NO_MSG, LType::Str);
    };
    let struct_ty = RESULT_STRUCT;
    let mp = cg.emit_reg(format!(
        "getelementptr {struct_ty}, {struct_ty}* {}, i32 0, i32 2",
        v.operand
    ));
    let raw = cg.emit_reg(format!("load i8*, i8** {mp}"));
    Value::new(raw, LType::Str)
}

/// The error message as a non-null string for `${message}` interpolation in an
/// `Error { message }` arm — substituting the bare `"Error"` constant when the
/// producer stored no message, so interpolation never reads a null pointer.
pub(crate) fn load_errmsg_str(cg: &mut Codegen, v: &Value) -> Value {
    let raw = load_errmsg(cg, v);
    let isnull = cg.emit_reg(format!("icmp eq i8* {}, null", raw.operand));
    let fallback = cg.string_constant("Error");
    let msg = cg.emit_reg(format!(
        "select i1 {isnull}, i8* {}, i8* {}",
        fallback.operand, raw.operand
    ));
    Value::new(msg, LType::Str)
}

/// Adapt the success payload while retaining failure and its message.
/// Every payload uses the same physical block on native and wasm32 targets.
pub(crate) fn repack_to_inner(cg: &mut Codegen, value: Value, inner: LType) -> Result<Value> {
    if value.result_inner == Some(inner) {
        return Ok(value);
    }
    let disc = load_disc(cg, &value);
    let errmsg = load_errmsg(cg, &value);
    let (_, error, end) = open_result_branch(cg, &value);
    let loaded = load_value(cg, &value);
    let owner = loaded.osp_ty.clone();
    let converted = convert_payload(cg, loaded, inner)?;
    let payload = join_payload(cg, converted, &error, &end).with_owner(owner);
    make_result(cg, payload, inner, &disc, &errmsg.operand)
}

/// Error constructors have an unreachable string-shaped success placeholder.
/// Bit conversion keeps that unused branch well typed for float/bool contexts.
fn convert_payload(cg: &mut Codegen, value: Value, inner: LType) -> Result<Value> {
    match (value.ty, inner) {
        (LType::Str | LType::Ptr, LType::Double | LType::I1) => {
            let bits = crate::conv::box_to_i64(cg, value);
            Ok(crate::conv::unbox_from_i64(cg, &bits.operand, inner))
        }
        _ => coerce_to(cg, value, inner),
    }
}

fn join_payload(cg: &mut Codegen, value: Value, error: &str, end: &str) -> Value {
    let success_pred = cg.snapshot_to(end);
    cg.start_block(error);
    let error_pred = cg.snapshot_to(end);
    cg.start_block(end);
    let zero = crate::llty::zero_literal(value.ty);
    let payload = cg.emit_reg(format!(
        "phi {} [ {}, %{success_pred} ], [ {zero}, %{error_pred} ]",
        value.ty, value.operand
    ));
    Value {
        operand: payload,
        ..value
    }
}

/// Fit a value into a declared `Result<inner, _>` slot: an existing Result is
/// re-laid under `inner` by [`repack_to_inner`], a plain value takes the
/// language's safe `T -> Success(T)` promotion. Every Result-typed parameter,
/// return and binding boundary routes through here so the promotion direction
/// is decided in exactly one place — the reverse (Result to plain) is never a
/// silent coercion.
pub(crate) fn fit_to_inner(cg: &mut Codegen, v: Value, inner: LType) -> Result<Value> {
    if v.result_inner.is_some() {
        repack_to_inner(cg, v, inner)
    } else {
        make_ok(cg, v, inner)
    }
}
