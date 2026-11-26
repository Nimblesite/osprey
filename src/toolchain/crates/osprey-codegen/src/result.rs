//! The `Result<T, E>` ABI: a heap block `{ i64 bits, i8 disc, i8* errmsg }`
//! reached by pointer, `disc == 0` ⇒ Success. `value` (slot 0) carries the
//! success payload bits; `errmsg` (slot 2) carries the Error payload `E`: a
//! null-terminated `i8*` string under `disc == 1` (`null` when there is
//! none), or a typed box ([`crate::anybox::box_typed`]) of any other `E`
//! under `disc == 2` ([`TYPED_ERROR`]). The builders here
//! construct that block; the readers branch on or load out of it. Runtime
//! fallible builtins (list/map get, string ops) and user functions declared
//! `-> Result<…>` both produce this shape, so match, `?:`, aggregate fields
//! and rendering handle exactly one representation. Implements
//! [ERR-PAYLOAD].

mod holds;
use crate::arc::Held;
use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::error::Result;
use crate::llty::{LType, Value, RESULT_STRUCT};
use holds::{message_hold, own_result, payload_hold, result_meta, store_payload, store_status};

/// A literal `null` `i8*` — the errmsg slot of a Success (or message-less Error).
pub(crate) const NO_MSG: &str = "null";

/// The discriminant of an Error whose slot 2 is a typed payload box, not a
/// string. Every reader of slot 2 tells the two apart by it, so a box is never
/// read as a string nor a string as a box. Implements [ERR-PAYLOAD].
pub(crate) const TYPED_ERROR: &str = "2";

/// `Error { message: payload }`: the block, its discriminant chosen by what
/// the payload is. A string is the message itself; an erased value is already
/// its own box; anything else is boxed whole at its type.
pub(crate) fn make_error(cg: &mut Codegen, payload: Value) -> Result<Value> {
    if payload.ty == LType::Str && payload.result_inner.is_none() {
        // Slot 2 holds the message and owns it; no reader takes an Error's
        // slot 0, so it stays empty and the text has one reference.
        let inner = payload.ty;
        let empty = Value::new(crate::llty::zero_literal(inner), inner);
        return make_result(cg, empty, inner, "1", &payload.operand);
    }
    let boxed = match payload.ty {
        LType::Any => payload,
        _ => crate::anybox::box_typed(cg, payload)?,
    };
    let zero = Value::new("0", LType::I64);
    make_result(cg, zero, LType::I64, TYPED_ERROR, &boxed.operand)
}

/// The error type the checker proved for a Result value, when it is closed.
pub(crate) fn error_type(v: &Value) -> Option<osprey_types::Type> {
    let osprey_types::Type::Con { name, args } = v.inferred_type.as_ref()? else {
        return None;
    };
    let error = args
        .get(1)
        .filter(|_| name == osprey_types::names::RESULT)?;
    (!osprey_types::has_type_var(error)).then(|| error.clone())
}

/// An error whose payload is its message text.
fn is_string_error(ty: &osprey_types::Type) -> bool {
    ty.is_named(osprey_types::names::STRING)
}

/// What an `Error { message }` arm binds: the payload at the Result's own
/// error type. An error type the backend cannot see binds the payload's
/// rendering, never a blind read. Implements [ERR-PAYLOAD].
pub(crate) fn load_error_payload(cg: &mut Codegen, v: &Value) -> Value {
    match error_type(v) {
        Some(ty) if is_string_error(&ty) => load_errmsg_str(cg, v),
        Some(ty) => typed_error_payload(cg, v, &ty),
        None => load_error_text(cg, v, true),
    }
}

/// The payload word of a typed Error read back as `ty`; zero on every other
/// discriminant, whose slot holds no box.
fn typed_error_payload(cg: &mut Codegen, v: &Value, ty: &osprey_types::Type) -> Value {
    let slot = load_errmsg(cg, v);
    if crate::types::ltype_of(ty) == LType::Any {
        let mut erased = Value::new(slot.operand, LType::Any);
        erased.inferred_type = Some(ty.clone());
        return erased;
    }
    let (typed, plain, end) = typed_branch(cg, v);
    cg.start_block(&typed);
    let word = crate::anybox::typed_word(cg, &slot.operand);
    let from_typed = cg.snapshot_to(&end);
    cg.start_block(&plain);
    let from_plain = cg.snapshot_to(&end);
    cg.start_block(&end);
    let word = cg.emit_reg(format!(
        "phi i64 [ {word}, %{from_typed} ], [ 0, %{from_plain} ]"
    ));
    value_of_word(cg, &word, ty)
}

/// Branch on whether `v` is a typed Error: `(typed, plain, end)` labels.
fn typed_branch(cg: &mut Codegen, v: &Value) -> (String, String, String) {
    let disc = load_disc(cg, v);
    let typed = cg.emit_reg(format!("icmp eq i8 {disc}, {TYPED_ERROR}"));
    cg.diamond(&typed)
}

/// A machine word read as a value of the checked type `ty`.
fn value_of_word(cg: &mut Codegen, word: &str, ty: &osprey_types::Type) -> Value {
    let mut value = if ty.is_named(osprey_types::names::RESULT) {
        let block = cg.emit_reg(format!(
            "inttoptr i64 {word} to {}",
            crate::llty::result_pointer()
        ));
        let mut nested = Value::result(block, crate::types::ltype_of(ty));
        nested.payload_owner = crate::types::result_payload_owner(&cg.prog, ty);
        nested
    } else {
        let owner = crate::types::owner_name(&cg.prog, ty);
        crate::conv::unbox_from_i64(cg, word, crate::types::ltype_of(ty)).with_owner(owner)
    };
    value.inferred_type = Some(ty.clone());
    value
}

/// The error payload as text: the stored string, or a typed payload rendered
/// at its own type. `named` substitutes `"Error"` for a missing message;
/// otherwise a missing message stays `null` for the caller to tell apart.
pub(crate) fn load_error_text(cg: &mut Codegen, v: &Value, named: bool) -> Value {
    let plain_text = |cg: &mut Codegen| {
        if named {
            load_errmsg_str(cg, v)
        } else {
            load_errmsg(cg, v)
        }
    };
    if error_type(v).is_some_and(|ty| is_string_error(&ty)) || v.result_inner.is_none() {
        return plain_text(cg);
    }
    let slot = load_errmsg(cg, v);
    let (typed, plain, end) = typed_branch(cg, v);
    cg.start_block(&typed);
    let rendered = crate::anybox::any_to_string(cg, &Value::new(slot.operand, LType::Any));
    let from_typed = cg.snapshot_to(&end);
    cg.start_block(&plain);
    let text = plain_text(cg);
    let from_plain = cg.snapshot_to(&end);
    cg.start_block(&end);
    let joined = cg.emit_reg(format!(
        "phi i8* [ {}, %{from_typed} ], [ {}, %{from_plain} ]",
        rendered.operand, text.operand
    ));
    Value::new(joined, LType::Str)
}

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
    // A Result that wraps an element read out of a collection holds it without
    // owning it: the collection does [MEM-OWNERSHIP-STATIC-BORROWS].
    let reading = crate::arc::take_reading(cg);
    let value = settled_payload(cg, value, disc);
    let value = coerce_to(cg, value, inner)?;
    let payload = payload_hold(cg, &value, reading);
    // A payload the Result only holds stays out of its drop walk.
    let walked = if payload == Held::Lent {
        LType::I64
    } else {
        inner
    };
    let message = message_hold(cg, errmsg);
    let meta = result_meta(cg, walked, message);
    let obj = cg.malloc_struct(RESULT_STRUCT, meta);
    store_payload(cg, &obj, &value, payload);
    store_status(cg, &obj, disc, errmsg, message);
    let mut out = Value::result(obj, inner).with_payload_owner(value.osp_ty);
    out.result_payload_type = value.inferred_type;
    own_result(cg, &out, walked, errmsg);
    if payload.lends() || message.lends() {
        crate::arc::mark_lent(cg, &out);
    }
    Ok(out)
}

/// A Result erases flat-list owner metadata, so settle its live payload before
/// boxing. An Error's unused payload may be null and must never be read.
/// Implements [BUILTIN-LIST-GET] and [ERR-PAYLOAD].
fn settled_payload(cg: &mut Codegen, value: Value, disc: &str) -> Value {
    if !crate::listlit::is_lit(&value) {
        return value;
    }
    if disc == "0" {
        return crate::listlit::escaping(cg, value);
    }
    let success = cg.emit_reg(format!("icmp eq i8 {disc}, 0"));
    let (taken, error, end) = cg.diamond(&success);
    cg.start_block(&taken);
    let settled = crate::listlit::escaping(cg, value);
    join_payload(cg, settled, &error, &end)
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

/// A list read's `Result`: `Success(value)` when `in_bounds` (an `i1`) holds,
/// else `Error(IndexOutOfBounds { index, length })`, the record built on the
/// failing path alone. Both list layouts land here. Implements
/// [BUILTIN-LIST-GET].
pub(crate) fn make_list_read(
    cg: &mut Codegen,
    in_bounds: &str,
    value: Value,
    index: &str,
    length: impl FnOnce(&mut Codegen) -> String,
) -> Result<Value> {
    let inner = value.ty;
    let mark = crate::arc::frame_mark(cg);
    let (ok, oob, end) = cg.diamond(in_bounds);
    cg.start_block(&oob);
    crate::arc::open_conditional(cg, mark);
    let length = length(cg);
    let record = crate::aggregate::index_out_of_bounds(cg, index, &length)?;
    let boxed = crate::anybox::box_typed(cg, record)?;
    // The box exists on this path alone: the join below carries it, or null,
    // into the Result, which then owns it [GC-ARC-PERCEUS].
    let carried = crate::arc::consume_into_store(cg, &boxed.operand);
    crate::arc::close_conditional(cg);
    let from_oob = cg.snapshot_to(&end);
    cg.start_block(&ok);
    let from_ok = cg.snapshot_to(&end);
    cg.start_block(&end);
    let slot = cg.emit_reg(format!(
        "phi i8* [ null, %{from_ok} ], [ {}, %{from_oob} ]",
        boxed.operand
    ));
    if carried {
        crate::arc::own(cg, &Value::new(slot.clone(), LType::Ptr));
    }
    let disc = cg.emit_reg(format!("select i1 {in_bounds}, i8 0, i8 {TYPED_ERROR}"));
    // An element type the backend cannot see stays open; the error side is
    // what every reader of the payload decides on.
    let elem = value.inferred_type.clone();
    let read = osprey_types::list_read(elem.unwrap_or(osprey_types::Type::Var(0)));
    let mut out = make_result(cg, value, inner, &disc, &slot)?;
    out.inferred_type = Some(read);
    Ok(out)
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
    let errmsg = match reason {
        // The slot holds the runtime's reason whenever it gave one, else the
        // static text or null, so the slot takes the reason's one reference
        // instead of counting a second [MEM-OWNERSHIP-STATIC].
        Some(r) => {
            let given = cg.emit_reg(format!("icmp ne i8* {r}, null"));
            let chosen = cg.emit_reg(format!("select i1 {given}, i8* {r}, i8* {fallback}"));
            let kept = cg.emit_reg(format!("or i1 {is_err}, {given}"));
            let selected = cg.emit_reg(format!("select i1 {kept}, i8* {chosen}, i8* null"));
            crate::arc::same_value(cg, &selected, r);
            selected
        }
        None if fallback == NO_MSG => NO_MSG.to_string(),
        None => {
            let selected = cg.emit_reg(format!("select i1 {is_err}, i8* {fallback}, i8* null"));
            // With no runtime reason the message is the static text or null.
            cg.mark_rodata(&selected);
            selected
        }
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
    let failed = format!("icmp slt i64 {result}, 0");
    result_unless(cg, result, LType::I64, &failed, msg, reason)
}

/// A `Result` over the C word `operand` of type `ty`: Error when the IR
/// predicate `failed` holds, carrying `msg` unless the runtime's `reason` wins.
pub(crate) fn result_unless(
    cg: &mut Codegen,
    operand: &str,
    ty: LType,
    failed: &str,
    msg: Option<&str>,
    reason: Option<&str>,
) -> Result<Value> {
    let err = cg.emit_reg(failed);
    make_result_if_err_because(cg, Value::new(operand, ty), ty, &err, msg, reason)
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
    let failed = format!("icmp eq i8* {ptr}, null");
    result_unless(cg, ptr, LType::Str, &failed, err, reason)
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
    // A handle payload (`Success { value }` of `Channel(capacity)`) carries
    // its element ABI as a `let`-bound handle does, so `recv` and `await` on
    // the binder can unbox what travels [CONCURRENCY-CHANNEL].
    match value
        .inferred_type
        .as_ref()
        .and_then(|ty| crate::builder::FiberSig::of(&cg.prog, ty))
    {
        Some(sig) if value.fiber_elem.is_none() => sig.restore(value),
        _ => value,
    }
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
    // The message is a field of the Result: a Result at its last read gives
    // it up [MEM-OWNERSHIP-STATIC-PATHS].
    crate::arc::note_field(cg, &raw, &v.operand, &mp, "i8*");
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
    // The stored message, or a literal that has no owner.
    crate::arc::same_value(cg, &msg, &raw.operand);
    Value::new(msg, LType::Str)
}

/// Adapt the success payload while retaining failure and its message.
/// Every payload uses the same physical block on native and wasm32 targets.
pub(crate) fn repack_to_inner(cg: &mut Codegen, value: Value, inner: LType) -> Result<Value> {
    if value.result_inner == Some(inner) {
        return Ok(value);
    }
    let owner = value.payload_owner.clone();
    map_payload(cg, &value, inner, |cg, loaded| {
        Ok(coerce_to(cg, loaded, inner)?.with_owner(owner))
    })
}

/// Convert only Success; preserve the discriminant and original error message.
pub(crate) fn map_payload(
    cg: &mut Codegen,
    value: &Value,
    inner: LType,
    convert: impl FnOnce(&mut Codegen, Value) -> Result<Value>,
) -> Result<Value> {
    if value.result_inner_is_placeholder {
        // Only Error constructors contribute this placeholder layout. Their
        // block holds nothing in slot 0 and every payload type shares one
        // physical block, so the same block serves `inner` as it is.
        let mut same = value.clone();
        same.result_inner = Some(inner);
        same.result_inner_is_placeholder = false;
        return Ok(same);
    }
    let errmsg = load_errmsg(cg, value);
    let disc = load_disc(cg, value);
    let (_, error, end) = open_result_branch(cg, value);
    let loaded = load_value(cg, value);
    let converted = convert(cg, loaded)?;
    let payload = join_payload(cg, converted, &error, &end);
    make_result(cg, payload, inner, &disc, &errmsg.operand)
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
/// return and binding boundary uses this physical layout adapter unless its
/// semantic payload needs element erasure through `cast::coerce_result`. The
/// reverse (Result to plain) is never a silent coercion.
pub(crate) fn fit_to_inner(cg: &mut Codegen, v: Value, inner: LType) -> Result<Value> {
    if v.result_inner.is_some() {
        repack_to_inner(cg, v, inner)
    } else {
        make_ok(cg, v, inner)
    }
}
