//! The `Result<T, E>` ABI: a heap block `{ T value, i8 disc, i8* errmsg }`
//! reached by pointer, `disc == 0` ⇒ Success. `value` (slot 0) carries the
//! success payload; `errmsg` (slot 2) carries the Error-arm message as a
//! null-terminated `i8*` (`null` when there is none). The builders here
//! construct that block; the readers branch on or load out of it. Runtime
//! fallible builtins (list/map get, string ops) and user functions declared
//! `-> Result<…>` both produce this shape, so match, `toString` and value-site
//! coercion handle exactly one representation. Implements [ERR-PAYLOAD].

use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::error::Result;
use crate::llty::{result_struct_ty, LType, Value};

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
    let v = coerce_to(cg, value, inner)?;
    let payload_owner = v.osp_ty.clone();
    let struct_ty = result_struct_ty(inner);
    // Layout word: payload word 0 managed iff pointer-typed; the errmsg slot
    // is always a (possibly rodata) pointer — the registry probe sorts it out.
    let meta = crate::meta::struct_meta(&[
        crate::meta::MetaField::of_lty(inner),
        crate::meta::MetaField::Byte,
        crate::meta::MetaField::PtrManaged,
    ]);
    let obj = cg.malloc_struct(&struct_ty, meta);
    crate::aggregate::store_field(cg, &struct_ty, obj.as_str(), 0, inner, &v.operand);
    let dp = cg.fresh_reg();
    cg.emit(format!(
        "{dp} = getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 1"
    ));
    cg.emit(format!("store i8 {disc}, i8* {dp}"));
    let mp = cg.fresh_reg();
    cg.emit(format!(
        "{mp} = getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 2"
    ));
    cg.emit(format!("store i8* {errmsg}, i8** {mp}"));
    Ok(Value::result(obj, inner).with_payload_owner(payload_owner))
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
    let disc = cg.fresh_reg();
    cg.emit(format!("{disc} = select i1 {is_err}, i8 1, i8 0"));
    let errmsg = match msg {
        Some(m) => {
            let c = cg.string_constant(m);
            cg.emit_reg(format!("select i1 {is_err}, i8* {}, i8* null", c.operand))
        }
        None => NO_MSG.to_string(),
    };
    make_result(cg, value, inner, &disc, &errmsg)
}

/// `Result<i64, _>` from a runtime `i32` success flag (`0` ⇒ Error) guarding an
/// `i64` payload — the shared shape of `listGet` / `mapGet`. `msg` is the Error
/// message text.
pub(crate) fn result_from_flag(
    cg: &mut Codegen,
    flag: &str,
    value: &str,
    msg: &str,
) -> Result<Value> {
    let err = cg.emit_reg(format!("icmp eq i32 {flag}, 0"));
    make_result_if_err(
        cg,
        Value::new(value, LType::I64),
        LType::I64,
        &err,
        Some(msg),
    )
}

/// `Result<int, _>` from a C `i64` whose negative values signal failure — the
/// uniform convention of the file/process/HTTP/JSON runtime (a negative handle,
/// byte count, status or process id is Error). The success value carried is the
/// result itself; `msg` is the Error message text (`None` for a bare Error).
pub(crate) fn result_from_i64(cg: &mut Codegen, result: &str, msg: Option<&str>) -> Result<Value> {
    let err = cg.emit_reg(format!("icmp slt i64 {result}, 0"));
    make_result_if_err(cg, Value::new(result, LType::I64), LType::I64, &err, msg)
}

/// `Result<string, _>` from a possibly-NULL C `char*` (`ptr` an `i8*` operand):
/// NULL ⇒ Error, else Success. The success slot keeps the pointer itself; when
/// `err` is `Some(msg)`, the errmsg slot carries that constant on the error path
/// so the `Error { message }` arm and `toString` (`Error(msg)`, e.g.
/// `readFile`'s `Error(File read error)`) both see a real reason. With `None` a
/// failure shows the bare `Error`.
pub(crate) fn result_from_nullable(
    cg: &mut Codegen,
    ptr: &str,
    err: Option<&str>,
) -> Result<Value> {
    let is_null = cg.emit_reg(format!("icmp eq i8* {ptr}, null"));
    make_result_if_err(cg, Value::new(ptr, LType::Str), LType::Str, &is_null, err)
}

/// Branch on a Result's discriminant: load it, test `== 0` (Success), and emit
/// the conditional branch to fresh `(success, error, end)` labels — leaving the
/// builder positioned at the start of the `success` block. The shared preamble
/// of every "do one thing on Success, another on Error, `phi` the results" path.
pub(crate) fn open_result_branch(cg: &mut Codegen, v: &Value) -> (String, String, String) {
    let d = load_disc(cg, v);
    let is_succ = cg.emit_reg(format!("icmp eq i8 {d}, 0"));
    let sl = cg.fresh_label();
    let el = cg.fresh_label();
    let end = cg.fresh_label();
    cg.emit(format!("br i1 {is_succ}, label %{sl}, label %{el}"));
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
    let dp = cg.fresh_reg();
    cg.emit(format!(
        "{dp} = getelementptr {struct_ty}, {struct_ty}* {}, i32 0, i32 1",
        v.operand
    ));
    let d = cg.fresh_reg();
    cg.emit(format!("{d} = load i8, i8* {dp}"));
    d
}

/// Load a Result block's success payload as its inner [`LType`]. Invariant: `v`
/// is a Result; a non-Result yields Unit rather than panicking.
pub(crate) fn load_value(cg: &mut Codegen, v: &Value) -> Value {
    let Some(inner) = v.result_inner else {
        return Value::unit();
    };
    let struct_ty = result_struct_ty(inner);
    let loaded = crate::aggregate::load_field(cg, &struct_ty, v.operand.as_str(), 0, inner);
    Value::new(loaded, inner).with_owner(v.payload_owner.clone())
}

/// Load a Result block's raw error-message pointer (slot 2) as an `i8*` — `null`
/// when the producer stored no message. Invariant: `v` is a Result; a non-Result
/// yields `null`. `toString` distinguishes the null case to print a bare `Error`.
pub(crate) fn load_errmsg(cg: &mut Codegen, v: &Value) -> Value {
    let Some(inner) = v.result_inner else {
        return Value::new(NO_MSG, LType::Str);
    };
    let struct_ty = result_struct_ty(inner);
    let mp = cg.fresh_reg();
    cg.emit(format!(
        "{mp} = getelementptr {struct_ty}, {struct_ty}* {}, i32 0, i32 2",
        v.operand
    ));
    let raw = cg.fresh_reg();
    cg.emit(format!("{raw} = load i8*, i8** {mp}"));
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

/// Re-lay a `Result` block under `inner` as its success-slot type, preserving
/// the discriminant and error message. A no-op when `inner` already matches;
/// otherwise it rebuilds `{T, i8, i8*}` so the producer and every reader agree
/// on the layout. Load-bearing on 32-bit targets (wasm32), where `i8*` (4 bytes)
/// and `i64` (8 bytes) differ in size: an `Error { message }` constructor types
/// its success slot from the *message* (`i8*`), but a function declared
/// `-> Result<int, _>` is read back with an `i64` slot, which silently shifts
/// the disc/errmsg offsets and flips Error to Success. [WASM-TARGET-WIDTH]
pub(crate) fn repack_to_inner(cg: &mut Codegen, v: Value, inner: LType) -> Result<Value> {
    if v.result_inner == Some(inner) {
        return Ok(v);
    }
    let disc = load_disc(cg, &v);
    let errmsg = load_errmsg(cg, &v);
    let loaded = load_value(cg, &v);
    let value = coerce_to(cg, loaded, inner)?;
    make_result(cg, value, inner, &disc, &errmsg.operand)
}

/// Auto-unwrap a Result at a value site (arithmetic, `print`, an argument),
/// yielding its success payload; a non-Result value passes through.
pub(crate) fn unwrap(cg: &mut Codegen, v: Value) -> Value {
    if v.result_inner.is_some() {
        load_value(cg, &v)
    } else {
        v
    }
}
