//! Emission of the C-runtime / libc calls that back Osprey's built-ins:
//! `toString` per type, `print`, and the numeric→string conversions. Float
//! formatting is delegated to `osp_float_to_string` (linked from
//! `libfiber_runtime`) so whole-valued floats keep their visible `.0`, exactly
//! as the golden outputs in `tests/regressions` expect.

use crate::builder::Codegen;
use crate::conv::as_i64;
use crate::error::Result;
use crate::llty::{LType, Value};

/// Convert a supported value to its `i8*` string form (`toString` /
/// interpolation / `print`). Strings pass through; the rest go through libc
/// `sprintf` or the float runtime. A `Result` formats as `Success(value)` /
/// `Error(message)`. Implements [BUILTIN-TOSTRING].
pub(crate) fn to_string_value(cg: &mut Codegen, v: Value) -> Result<Value> {
    if v.result_inner.is_some() {
        return result_to_string(cg, &v);
    }
    match v.ty {
        LType::Str | LType::Ptr => Ok(Value::new(v.operand, LType::Str)),
        LType::I1 => Ok(bool_to_string(cg, &v)),
        LType::Double => Ok(float_to_string(cg, &v)),
        LType::I64 | LType::I32 => int_to_string(cg, v),
        // An erased value renders through its shape descriptor — never the
        // raw word, which printed heap addresses as integers (finding D,
        // [TYPE-ANY]).
        LType::Any => Ok(crate::anybox::any_to_string(cg, &v)),
    }
}

/// Format a `Result` block as `Success(<value>)` or `Error(<message>)`, branching
/// on its discriminant — the spelling the golden outputs expect. The error
/// payload comes from the errmsg slot (slot 2), which `load_errmsg` already
/// falls back to the bare `"Error"` constant when unset. Implements [ERR-PAYLOAD].
fn result_to_string(cg: &mut Codegen, v: &Value) -> Result<Value> {
    result_string(cg, v, true)
}

/// [`result_to_string`] minus the `Success(…)` wrapping: a Success renders as
/// its bare payload, an Error as `Error(<message>)`. The assertion operands'
/// rendering — a Success compares as its payload, an Error is a visible
/// mismatch, never a blind payload load. [TESTING-EQUALITY]
pub(crate) fn result_payload_or_error_string(cg: &mut Codegen, v: &Value) -> Result<Value> {
    result_string(cg, v, false)
}

fn result_string(cg: &mut Codegen, v: &Value, wrap_success: bool) -> Result<Value> {
    let (_sl, el, end) = crate::result::open_result_branch(cg, v);
    let val = crate::result::load_value(cg, v);
    let vs = to_string_value(cg, val)?;
    let succ = if wrap_success {
        sprintf_wrap(cg, "Success(%s)", &vs.operand)
    } else {
        vs.operand
    };
    let sb = cg.snapshot_to(&end);

    cg.start_block(&el);
    // A message-less Error (null errmsg slot) prints just `Error`; any real
    // reason prints `Error(<reason>)`.
    let msg = crate::result::load_errmsg(cg, v);
    let isnull = cg.emit_reg(format!("icmp eq i8* {}, null", msg.operand));
    let fl = cg.fresh_label();
    let nl = cg.fresh_label();
    let jl = cg.fresh_label();
    cg.emit(format!("br i1 {isnull}, label %{nl}, label %{fl}"));
    cg.start_block(&fl);
    let with = sprintf_wrap(cg, "Error(%s)", &msg.operand);
    let fb = cg.cur_block().to_string();
    cg.emit(format!("br label %{jl}"));
    cg.start_block(&nl);
    let bare = cg.string_constant("Error");
    cg.emit(format!("br label %{jl}"));
    cg.start_block(&jl);
    let err = cg.fresh_reg();
    cg.emit(format!(
        "{err} = phi i8* [ {with}, %{fb} ], [ {}, %{nl} ]",
        bare.operand
    ));
    let eb = cg.snapshot_to(&end);

    cg.start_block(&end);
    let phi = cg.fresh_reg();
    cg.emit(format!(
        "{phi} = phi i8* [ {succ}, %{sb} ], [ {err}, %{eb} ]"
    ));
    Ok(Value::new(phi, LType::Str))
}

/// Wrap one string in a single-`%s` template — `Success(%s)`, `Error(%s)` —
/// into a buffer sized for the string that is actually there.
///
/// This used to `sprintf` into a fixed 64-byte block, which is a heap buffer
/// overflow, not a size heuristic: the substituted string is a runtime value of
/// any length. `toString` of a `readFile` success wrote the whole file past the
/// end of a 64-byte allocation, and an error message longer than 56 characters
/// did the same — silent heap corruption on the printing path of the built-in
/// whose entire job is to render a value truthfully. [BUILTIN-TOSTRING]
fn sprintf_wrap(cg: &mut Codegen, fmt: &str, arg: &str) -> String {
    format_sized(cg, fmt, &[format!("i8* {arg}")]).operand
}

/// Build `fmt` — a codegen-built template whose holes are all `%s` — into an
/// exactly-sized heap buffer, in two passes: measure with `osp_format_size`,
/// then fill with `osp_format_into`. `args` are complete LLVM operands
/// (`i8* %r7`). The returned buffer is owned by the current region.
///
/// Measuring goes through `osp_format_size` rather than `snprintf` directly
/// because this IR is target-neutral and `size_t` is not: it is 32-bit on
/// wasm32 and 64-bit natively, so a literal size type here mismatches
/// wasi-libc at wasm-ld time. See `string_runtime.h`. [STRING-INTERPOLATION]
pub(crate) fn format_sized(cg: &mut Codegen, fmt: &str, args: &[String]) -> Value {
    cg.add_extern("declare i64 @osp_format_size(i8*, ...)");
    cg.add_extern("declare void @osp_format_into(i8*, i64, i8*, ...)");
    let fmtv = cg.string_constant(fmt);
    let extra = args.iter().fold(String::new(), |mut acc, a| {
        acc.push_str(", ");
        acc.push_str(a);
        acc
    });
    let len = cg.emit_reg(format!(
        "call i64 (i8*, ...) @osp_format_size(i8* {}{extra})",
        fmtv.operand
    ));
    let size = cg.emit_reg(format!("add i64 {len}, 1"));
    let buf = cg.heap_alloc(&size);
    cg.emit(format!(
        "call void (i8*, i64, i8*, ...) @osp_format_into(i8* {buf}, i64 {size}, i8* {}{extra})",
        fmtv.operand
    ));
    let v = Value::new(buf, LType::Str);
    crate::arc::own(cg, &v);
    v
}

/// `print(x)` → `puts(toString(x))`; yields Unit. [BUILTIN-PRINT]
pub(crate) fn gen_print(cg: &mut Codegen, v: Value) -> Result<Value> {
    let s = to_string_value(cg, v)?;
    cg.add_extern("declare i32 @puts(i8*)");
    let reg = cg.fresh_reg();
    cg.emit(format!("{reg} = call i32 @puts(i8* {})", s.operand));
    Ok(Value::unit())
}

/// The widest `%lld` an i64 can produce: `-9223372036854775808` is 20
/// characters, so 21 bytes hold every value with its terminator. Unlike the
/// `%s` templates above, this bound is a property of the type, not of a runtime
/// value, so a fixed block is sound here and saves the measuring pass.
const INT_STRING_BYTES: &str = "21";

fn int_to_string(cg: &mut Codegen, v: Value) -> Result<Value> {
    cg.add_extern("declare i32 @sprintf(i8*, i8*, ...)");
    let i = as_i64(cg, v)?;
    // `%lld` (not `%ld`): Osprey `int` is i64, and on ILP32 targets like wasm32
    // `long` is 32-bit while `long long` is 64-bit everywhere. `%lld` reads the
    // full i64 on every target; on LP64 (native) it is identical to `%ld`.
    let fmt = cg.string_constant("%lld");
    let buf = cg.heap_alloc(INT_STRING_BYTES);
    let tmp = cg.fresh_reg();
    cg.emit(format!(
        "{tmp} = call i32 (i8*, i8*, ...) @sprintf(i8* {buf}, i8* {}, i64 {})",
        fmt.operand, i.operand
    ));
    let v = Value::new(buf, LType::Str);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// Whole-valued floats must print with a trailing `.0`; the runtime handles
/// that (and NaN/inf) — see `runtime/string_runtime.c`.
fn float_to_string(cg: &mut Codegen, v: &Value) -> Value {
    cg.add_extern("declare i8* @osp_float_to_string(double)");
    let reg = cg.fresh_reg();
    cg.emit(format!(
        "{reg} = call i8* @osp_float_to_string(double {})",
        v.operand
    ));
    let out = Value::new(reg, LType::Str);
    crate::arc::own(cg, &out);
    out
}

fn bool_to_string(cg: &mut Codegen, v: &Value) -> Value {
    let t = cg.string_constant("true");
    let f = cg.string_constant("false");
    let reg = cg.fresh_reg();
    cg.emit(format!(
        "{reg} = select i1 {}, i8* {}, i8* {}",
        v.operand, t.operand, f.operand
    ));
    Value::new(reg, LType::Str)
}
