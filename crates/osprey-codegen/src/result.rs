//! The `Result<T, E>` ABI: a heap block `{ T value, i8 disc, i8* errmsg }`
//! reached by pointer, `disc == 0` ⇒ Success. `value` (slot 0) carries the
//! success payload; `errmsg` (slot 2) carries the Error-arm message as a
//! null-terminated `i8*` (`null` when there is none). The builders here
//! construct that block; the readers branch on or load out of it. Runtime
//! fallible builtins (list/map get, string ops) and user functions declared
//! `-> Result<…>` both produce this shape, so match, `?:`, failure-preserving
//! arithmetic, and rendering handle exactly one representation. Implements
//! [ERR-PAYLOAD].

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
    // The block's drop mask releases the errmsg word too [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, "i8*", errmsg);
    cg.emit(format!("store i8* {errmsg}, i8** {mp}"));
    let out = Value::result(obj, inner).with_payload_owner(payload_owner);
    crate::arc::own(cg, &out);
    // A scalar payload plus an unmanaged errmsg means the block holds zero
    // managed references — eligible for the consume-at-unwrap fast path that
    // lets -O2 delete the whole block. A literal reason (`"integer overflow"`,
    // `"division by zero"`) reaches here as a REGISTER holding a getelementptr
    // into a private constant, so testing the spelling alone would misjudge
    // every checked-arithmetic Error arm as impure; ask the rodata ledger
    // instead. [GC-ARC-PERCEUS]
    let errmsg_unmanaged = !errmsg.starts_with('%') || cg.is_rodata(errmsg);
    if !inner.is_managed_ptr() && errmsg_unmanaged {
        crate::arc::mark_pure_scalar(cg, &out);
    }
    Ok(out)
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
    let disc = cg.fresh_reg();
    cg.emit(format!("{disc} = select i1 {is_err}, i8 1, i8 0"));
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
    let is_success = cg.emit_reg(format!("icmp eq i8 {disc}, 0"));
    let success = cg.fresh_label();
    let error = cg.fresh_label();
    let end = cg.fresh_label();
    cg.emit(format!(
        "br i1 {is_success}, label %{success}, label %{error}"
    ));

    cg.start_block(&success);
    let loaded = load_value(cg, &v);
    let owner = loaded.osp_ty.clone();
    // An `Error { message }` constructor initially carries a string-shaped
    // placeholder success slot. This branch is unreachable for that value, but
    // its LLVM still has to type-check when the contextual Result payload is a
    // float or bool. Convert pointer bits through the erased word solely to
    // produce a well-typed unreachable operand; real Success values have
    // matching source types and take the ordinary coercion.
    let converted = match (loaded.ty, inner) {
        (LType::Str | LType::Ptr, LType::Double) => {
            let bits = crate::conv::box_to_i64(cg, loaded);
            Value::new(
                cg.emit_reg(format!("bitcast i64 {} to double", bits.operand)),
                LType::Double,
            )
        }
        (LType::Str | LType::Ptr, LType::I1) => {
            let bits = crate::conv::box_to_i64(cg, loaded);
            Value::new(
                cg.emit_reg(format!("trunc i64 {} to i1", bits.operand)),
                LType::I1,
            )
        }
        _ => coerce_to(cg, loaded, inner)?,
    };
    let success_pred = cg.snapshot_to(&end);

    cg.start_block(&error);
    let zero = crate::llty::zero_literal(inner);
    let error_pred = cg.snapshot_to(&end);

    cg.start_block(&end);
    let payload = cg.emit_reg(format!(
        "phi {inner} [ {}, %{success_pred} ], [ {zero}, %{error_pred} ]",
        converted.operand
    ));
    make_result(
        cg,
        Value::new(payload, inner).with_owner(owner),
        inner,
        &disc,
        &errmsg.operand,
    )
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

/// Extract a Result's success payload after the caller has established that
/// this is an explicit handling context (such as a `?:` success branch or
/// failure-preserving arithmetic). A non-Result passes through.
pub(crate) fn unwrap(cg: &mut Codegen, v: Value) -> Value {
    if v.result_inner.is_some() {
        let out = load_value(cg, &v);
        crate::arc::consume_fresh(cg, &v);
        out
    } else {
        v
    }
}
