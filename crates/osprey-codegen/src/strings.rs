//! String builtins (`length`/`contains`/`substring`/`parseInt`/`join`/…) — thin
//! wrappers over the C string runtime declared in `runtime/string_runtime.h`
//! and linked from `libfiber_runtime`, whose symbols fix each builtin's
//! signature. Total operations return their bare value; fallible ones return
//! the uniform `{ value, i8 }*` Result block. Implements [BUILTIN-STRING-*].

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::result::{make_result, make_result_if_err, result_from_nullable};
use osprey_ast::{Expr, NamedArgument};

/// Dispatch a string builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    let v = match name {
        // `osp_strlen` (not libc `strlen`) returns `i64` on every target; libc
        // `strlen` returns `size_t`, 32-bit on wasm32, breaking the IR's width.
        "length" => unary_i64(cg, "osp_strlen", args, named)?,
        "isEmpty" => bool_from_i64(cg, "osp_string_is_empty", &[(0, LType::Str)], args, named)?,
        "contains" => contains(cg, args, named)?,
        "startsWith" => bool_from_i64(
            cg,
            "osp_string_starts_with",
            &[(0, LType::Str), (1, LType::Str)],
            args,
            named,
        )?,
        "endsWith" => bool_from_i64(
            cg,
            "osp_string_ends_with",
            &[(0, LType::Str), (1, LType::Str)],
            args,
            named,
        )?,
        "toUpperCase" => unary_str(cg, "osp_string_to_upper", args, named)?,
        "toLowerCase" => unary_str(cg, "osp_string_to_lower", args, named)?,
        "trim" => unary_str(cg, "osp_string_trim", args, named)?,
        "trimStart" => unary_str(cg, "osp_string_trim_start", args, named)?,
        "trimEnd" => unary_str(cg, "osp_string_trim_end", args, named)?,
        "reverse" => unary_str(cg, "osp_string_reverse", args, named)?,
        "take" => str_int_str(cg, "osp_string_take", args, named)?,
        "drop" => str_int_str(cg, "osp_string_drop", args, named)?,
        "indexOf" => index_of(cg, args, named)?,
        "substring" => substring(cg, args, named)?,
        "replace" => nullable_str(
            cg,
            "osp_string_replace",
            &[LType::Str, LType::Str, LType::Str],
            "replace: needle must not be empty",
            args,
            named,
        )?,
        "repeat" => nullable_str(
            cg,
            "osp_string_repeat",
            &[LType::Str, LType::I64],
            "repeat: count must not be negative",
            args,
            named,
        )?,
        "padStart" => nullable_str(
            cg,
            "osp_string_pad_start",
            &[LType::Str, LType::I64, LType::Str],
            "padStart: fill must not be empty",
            args,
            named,
        )?,
        "padEnd" => nullable_str(
            cg,
            "osp_string_pad_end",
            &[LType::Str, LType::I64, LType::Str],
            "padEnd: fill must not be empty",
            args,
            named,
        )?,
        "parseInt" => parse_strict(
            cg,
            "osp_parse_int_strict",
            LType::I64,
            "parseInt: invalid integer",
            args,
            named,
        )?,
        "parseFloat" => parse_strict(
            cg,
            "osp_parse_float_strict",
            LType::Double,
            "parseFloat: invalid number",
            args,
            named,
        )?,
        "join" => join(cg, args, named)?,
        "lines" => string_list(cg, "osp_string_lines", args)?,
        "words" => string_list(cg, "osp_string_words", args)?,
        "split" => split(cg, args)?,
        // O(1) byte / codepoint cursor (BUILTIN-STRING-CURSOR).
        "byteLength" => unary_i64(cg, "osp_string_byte_length", args, named)?,
        "byteAt" => cursor_int(cg, "osp_string_byte_at", &[LType::Str, LType::I64], args)?,
        "codePointAt" => cursor_int(
            cg,
            "osp_string_codepoint_at",
            &[LType::Str, LType::I64],
            args,
        )?,
        "codePointWidth" => cursor_int(cg, "osp_string_codepoint_width", &[LType::I64], args)?,
        "fromCodePoint" => from_codepoint(cg, args)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// The `i`-th positional argument, evaluated and coerced to `want`.
fn arg(cg: &mut Codegen, args: &[Expr], i: usize, want: LType) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("string builtin: missing argument"))?;
    let v = gen_expr(cg, e)?;
    crate::cast::coerce_to(cg, v, want)
}

/// Evaluate the listed `(index, LType)` arguments, returning their operands and
/// the matching LLVM parameter-type list — the shared front half of the runtime
/// calls whose arity varies (`startsWith`, `replace`, `padStart`, …).
fn typed_args(
    cg: &mut Codegen,
    sig: &[(usize, LType)],
    args: &[Expr],
) -> Result<(Vec<String>, String)> {
    let mut ops = Vec::with_capacity(sig.len());
    let mut params = Vec::with_capacity(sig.len());
    for (i, ty) in sig {
        ops.push(arg(cg, args, *i, *ty)?.operand);
        params.push(ty.to_string());
    }
    Ok((ops, params.join(", ")))
}

/// `f(s: string) -> int`.
fn unary_i64(
    cg: &mut Codegen,
    cname: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    Ok(Value::new(
        cg.call("i64", cname, "i8*", &[&s.operand]),
        LType::I64,
    ))
}

/// `f(s: string) -> string`.
fn unary_str(
    cg: &mut Codegen,
    cname: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let v = Value::new(cg.call("i8*", cname, "i8*", &[&s.operand]), LType::Str);
    // The C string runtime mints fresh malloc'd outputs (+1) — never aliases
    // its inputs — so the caller owns every return [GC-ARC-PERCEUS].
    crate::arc::own(cg, &v);
    Ok(v)
}

/// `f(s: string, n: int) -> string`.
fn str_int_str(
    cg: &mut Codegen,
    cname: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let n = arg(cg, args, 1, LType::I64)?;
    let r = cg.call("i8*", cname, "i8*, i64", &[&s.operand, &n.operand]);
    let v = Value::new(r, LType::Str);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// A runtime predicate returning `i64` truthiness, narrowed to `i1`. `sig` lists
/// each argument index and the LLVM type it travels as.
fn bool_from_i64(
    cg: &mut Codegen,
    cname: &str,
    sig: &[(usize, LType)],
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let (ops, params) = typed_args(cg, sig, args)?;
    let op_refs: Vec<&str> = ops.iter().map(String::as_str).collect();
    let raw = cg.call("i64", cname, &params, &op_refs);
    let r = cg.fresh_reg();
    cg.emit(format!("{r} = icmp ne i64 {raw}, 0"));
    Ok(Value::new(r, LType::I1))
}

/// `contains(s, needle) -> bool` via libc `strstr` (non-NULL ⇒ found).
fn contains(cg: &mut Codegen, args: &[Expr], _named: &[NamedArgument]) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let needle = arg(cg, args, 1, LType::Str)?;
    let hit = cg.call("i8*", "strstr", "i8*, i8*", &[&s.operand, &needle.operand]);
    let r = cg.fresh_reg();
    cg.emit(format!("{r} = icmp ne i8* {hit}, null"));
    Ok(Value::new(r, LType::I1))
}

/// `indexOf(s, needle) -> Result<int, _>` (`-1` ⇒ Error).
fn index_of(cg: &mut Codegen, args: &[Expr], _named: &[NamedArgument]) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let needle = arg(cg, args, 1, LType::Str)?;
    let idx = cg.call(
        "i64",
        "osp_string_index_of",
        "i8*, i8*",
        &[&s.operand, &needle.operand],
    );
    let iserr = cg.fresh_reg();
    cg.emit(format!("{iserr} = icmp slt i64 {idx}, 0"));
    let val = cg.fresh_reg();
    cg.emit(format!("{val} = select i1 {iserr}, i64 0, i64 {idx}"));
    make_result_if_err(
        cg,
        Value::new(val, LType::I64),
        LType::I64,
        &iserr,
        Some("indexOf: substring not found"),
    )
}

/// `substring(s, start, end) -> Result<string, _>` (NULL ⇒ Error).
fn substring(cg: &mut Codegen, args: &[Expr], _named: &[NamedArgument]) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let start = arg(cg, args, 1, LType::I64)?;
    let end = arg(cg, args, 2, LType::I64)?;
    let ptr = cg.call(
        "i8*",
        "osp_string_substring",
        "i8*, i64, i64",
        &[&s.operand, &start.operand, &end.operand],
    );
    // The raw +1 return is owned here; the Result block dups its own copy.
    crate::arc::own(cg, &Value::new(&ptr, LType::Str));
    result_from_nullable(cg, &ptr, Some("substring: index out of range"))
}

/// A fallible string transform returning a runtime `char*` that is NULL on
/// failure, wrapped into `Result<string, _>` with `errmsg` as the Error reason.
/// `argtys` lists each argument's LLVM type in order.
fn nullable_str(
    cg: &mut Codegen,
    cname: &str,
    argtys: &[LType],
    errmsg: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let (ops, params) = typed_args_for_types(cg, argtys, args)?;
    let op_refs: Vec<&str> = ops.iter().map(String::as_str).collect();
    let ptr = cg.call("i8*", cname, &params, &op_refs);
    crate::arc::own(cg, &Value::new(&ptr, LType::Str));
    result_from_nullable(cg, &ptr, Some(errmsg))
}

/// `parseInt`/`parseFloat`: strict parse writing through an out-slot, returning
/// `0` on success. `inner` is the parsed value's LLVM type.
fn parse_strict(
    cg: &mut Codegen,
    cname: &str,
    inner: LType,
    errmsg: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let slot = cg.fresh_reg();
    cg.emit(format!("{slot} = alloca {inner}"));
    let zero = if inner == LType::Double { "0.0" } else { "0" };
    cg.emit(format!("store {inner} {zero}, {inner}* {slot}"));
    let rc = cg.call(
        "i64",
        cname,
        &format!("i8*, {inner}*"),
        &[&s.operand, &slot],
    );
    let parsed = cg.fresh_reg();
    cg.emit(format!("{parsed} = load {inner}, {inner}* {slot}"));
    let iserr = cg.fresh_reg();
    cg.emit(format!("{iserr} = icmp ne i64 {rc}, 0"));
    make_result_if_err(cg, Value::new(parsed, inner), inner, &iserr, Some(errmsg))
}

/// `lines`/`words`: split a string into a `List<string>`. The C `osp_string_list`
/// shares its leading `i64 length` with the runtime list, so list builtins read
/// it directly; tag the handle as a list.
fn string_list(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let r = cg.call("i8*", cname, "i8*", &[&s.operand]);
    let v = Value::handle(r, crate::collections::LIST_OWNER);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// `split(s, sep) -> Result<List<string>, _>` (NULL ⇒ Error, e.g. empty sep).
fn split(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let s = arg(cg, args, 0, LType::Str)?;
    let sep = arg(cg, args, 1, LType::Str)?;
    let ptr = cg.call(
        "i8*",
        "osp_string_split",
        "i8*, i8*",
        &[&s.operand, &sep.operand],
    );
    crate::arc::own(cg, &Value::new(&ptr, LType::Ptr));
    let iserr = cg.fresh_reg();
    cg.emit(format!("{iserr} = icmp eq i8* {ptr}, null"));
    make_result_if_err(
        cg,
        Value::handle(ptr, crate::collections::LIST_OWNER),
        LType::Ptr,
        &iserr,
        Some("split: separator must not be empty"),
    )
}

/// A fallible cursor builtin returning `Result<int, _>` (BUILTIN-STRING-CURSOR):
/// the C function writes its `i64` result through an out-slot and returns NULL on
/// success or a static `i8*` error message, which lands directly in the Result's
/// errmsg slot. `argtys` lists the leading argument types before the out-slot.
fn cursor_int(cg: &mut Codegen, cname: &str, argtys: &[LType], args: &[Expr]) -> Result<Value> {
    let (mut ops, params) = typed_args_for_types(cg, argtys, args)?;
    let slot = cg.fresh_reg();
    cg.emit(format!("{slot} = alloca i64"));
    cg.emit(format!("store i64 0, i64* {slot}"));
    ops.push(slot.clone());
    let full_params = format!("{params}, i64*");
    let op_refs: Vec<&str> = ops.iter().map(String::as_str).collect();
    let emsg = cg.call("i8*", cname, &full_params, &op_refs);
    let parsed = cg.fresh_reg();
    cg.emit(format!("{parsed} = load i64, i64* {slot}"));
    let is_err = cg.fresh_reg();
    cg.emit(format!("{is_err} = icmp ne i8* {emsg}, null"));
    let disc = cg.fresh_reg();
    cg.emit(format!("{disc} = select i1 {is_err}, i8 1, i8 0"));
    make_result(cg, Value::new(parsed, LType::I64), LType::I64, &disc, &emsg)
}

/// Lower positional arguments according to their declared LLVM types.
fn typed_args_for_types(
    cg: &mut Codegen,
    argtys: &[LType],
    args: &[Expr],
) -> Result<(Vec<String>, String)> {
    let signature: Vec<_> = argtys
        .iter()
        .enumerate()
        .map(|(index, ty)| (index, *ty))
        .collect();
    typed_args(cg, &signature, args)
}

/// `fromCodePoint(cp: int) -> Result<string, _>` — the C encoder returns NULL on
/// an invalid scalar value (surrogate / out of range), wrapped as Error.
fn from_codepoint(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let cp = arg(cg, args, 0, LType::I64)?;
    let ptr = cg.call("i8*", "osp_string_from_codepoint", "i64", &[&cp.operand]);
    crate::arc::own(cg, &Value::new(&ptr, LType::Str));
    result_from_nullable(cg, &ptr, Some("fromCodePoint: invalid code point"))
}

/// `join(list: List<string>, separator: string) -> string`.
fn join(cg: &mut Codegen, args: &[Expr], _named: &[NamedArgument]) -> Result<Value> {
    let list = arg(cg, args, 0, LType::Ptr)?;
    let sep = arg(cg, args, 1, LType::Str)?;
    let r = cg.call(
        "i8*",
        "osp_string_join",
        "i8*, i8*",
        &[&list.operand, &sep.operand],
    );
    let v = Value::new(r, LType::Str);
    crate::arc::own(cg, &v);
    Ok(v)
}
