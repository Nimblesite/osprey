//! List<T> and Map<K,V> builtins backed by the C runtime (`osprey_list_*` /
//! `osprey_map_*` in `libfiber_runtime`, whose signatures are the contract).
//! Element values cross the boundary as a uniform `i64`; pointers are
//! `ptrtoint`-boxed. List/Map handles are `i8*` tagged with their owner so the
//! `+` operator and `toString` can tell them from records. Implements
//! [TYPE-LIST-OPS], [TYPE-MAP-OPS].

use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::conv::box_to_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::loops::{close_list_loop, open_list_loop};
use crate::result::result_from_flag;
use osprey_ast::{Expr, NamedArgument};

/// The owner tag carried by runtime list / map handles.
pub(crate) const LIST_OWNER: &str = "List";
pub(crate) const MAP_OWNER: &str = "Map";

/// Dispatch a collection builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    let v = match name {
        "List" => list_empty(cg),
        "listLength" => one_list_i64(cg, "osprey_list_length", args)?,
        "listAppend" => list_box2(cg, "osprey_list_append", args)?,
        "listPrepend" => list_box2(cg, "osprey_list_prepend", args)?,
        "listDrop" => list_box2(cg, "osprey_list_drop", args)?,
        "listConcat" => binary_handle_op(cg, args, "osprey_list_concat", LIST_OWNER)?,
        "listReverse" => one_list_handle(cg, "osprey_list_reverse", args)?,
        "listGet" => list_get(cg, args)?,
        "listContains" => list_contains(cg, args)?,
        "Map" => map_empty(cg),
        "mapLength" => one_list_i64(cg, "osprey_map_length", args)?,
        "mapSet" => map_set(cg, args)?,
        "mapGet" => map_get(cg, args)?,
        "mapContains" => map_contains(cg, args)?,
        "mapRemove" => map_remove(cg, args)?,
        "mapMerge" => binary_handle_op(cg, args, "osprey_map_merge", MAP_OWNER)?,
        "mapKeys" => map_to_list(cg, args, true)?,
        "mapValues" => map_to_list(cg, args, false)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// The `i`-th positional argument as an opaque `i8*` collection handle.
fn handle_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("collection builtin: missing argument"))?;
    let v = gen_expr(cg, e)?;
    coerce_to(cg, v, LType::Ptr)
}

/// The `i`-th positional argument, evaluated and unwrapped (pre-boxing).
fn unboxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("collection builtin: missing argument"))?;
    let v = gen_expr(cg, e)?;
    Ok(crate::result::unwrap(cg, v))
}

/// The `i`-th positional argument, boxed to the uniform `i64` element ABI.
fn boxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let v = unboxed_arg(cg, args, i)?;
    Ok(box_to_i64(cg, v))
}

/// [`boxed_arg`] for an element/key the container will STORE: a persistent
/// container holds a new reference, so dup managed values before the pointer
/// is erased into the `i64` ABI (containers never release their elements —
/// the leak-safe M4 posture) [GC-ARC-PERCEUS].
fn stored_boxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let v = unboxed_arg(cg, args, i)?;
    crate::arc::escape_retain(cg, &v);
    Ok(box_to_i64(cg, v))
}

/// Own a fresh runtime container handle: every `osprey_list_*`/`osprey_map_*`
/// producer returns +1 (fresh allocations; alias returns retain-on-return and
/// the empty singletons are immortal — `memory_arc.c`, plan 0011 M4).
fn own_handle(cg: &mut Codegen, v: Value) -> Value {
    crate::arc::own(cg, &v);
    v
}

fn list_empty(cg: &mut Codegen) -> Value {
    let v = Value::handle(cg.call("i8*", "osprey_list_empty", "", &[]), LIST_OWNER);
    own_handle(cg, v)
}

fn map_empty(cg: &mut Codegen) -> Value {
    // OSPREY_KEY_STRING = 1 (Map() defaults to string keys).
    let v = Value::handle(cg.call("i8*", "osprey_map_empty", "i32", &["1"]), MAP_OWNER);
    own_handle(cg, v)
}

/// `f(handle) -> int`.
fn one_list_i64(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let r = cg.call("i64", cname, "i8*", &[&h.operand]);
    Ok(Value::new(r, LType::I64))
}

/// `f(handle) -> handle`.
fn one_list_handle(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let r = cg.call("i8*", cname, "i8*", &[&h.operand]);
    Ok(own_handle(cg, Value::handle(r, LIST_OWNER)))
}

/// `f(handle, boxed) -> handle` (append / prepend / drop). The second
/// argument is stored by append/prepend (drop's count is an int — the
/// managed-gated dup skips it).
fn list_box2(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let x = stored_boxed_arg(cg, args, 1)?;
    let r = cg.call("i8*", cname, "i8*, i64", &[&h.operand, &x.operand]);
    Ok(own_handle(cg, Value::handle(r, LIST_OWNER)))
}

/// A binary runtime op on two collection-handle arguments → a new handle
/// (`listConcat`, `mapMerge`): evaluate both, then [`combine_handles`].
fn binary_handle_op(cg: &mut Codegen, args: &[Expr], cname: &str, owner: &str) -> Result<Value> {
    let a = handle_arg(cg, args, 0)?;
    let b = handle_arg(cg, args, 1)?;
    Ok(combine_handles(cg, &a, &b, cname, owner))
}

/// A runtime op combining two collection handles into a new one — the body
/// behind both list concat and map merge.
fn combine_handles(cg: &mut Codegen, a: &Value, b: &Value, cname: &str, owner: &str) -> Value {
    let r = cg.call("i8*", cname, "i8*, i8*", &[&a.operand, &b.operand]);
    let v = Value::handle(r, owner);
    own_handle(cg, v)
}

/// Emit `osprey_list_concat` on two already-evaluated list handles.
pub(crate) fn concat_handles(cg: &mut Codegen, a: &Value, b: &Value) -> Value {
    combine_handles(cg, a, b, "osprey_list_concat", LIST_OWNER)
}

/// `listGet(l, i) -> Result<T, _>` gated on `osprey_list_in_bounds`.
fn list_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = handle_arg(cg, args, 0)?;
    let i = boxed_arg(cg, args, 1)?;
    let inb = cg.call(
        "i32",
        "osprey_list_in_bounds",
        "i8*, i64",
        &[&l.operand, &i.operand],
    );
    let val = cg.call(
        "i64",
        "osprey_list_get",
        "i8*, i64",
        &[&l.operand, &i.operand],
    );
    result_from_flag(cg, &inb, &val, "listGet: index out of bounds")
}

/// `listContains(l, x) -> bool`: linear scan, content-equality for strings.
fn list_contains(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = handle_arg(cg, args, 0)?;
    let needle_e = args
        .get(1)
        .ok_or_else(|| CodegenError::invalid("listContains: missing argument"))?;
    let needle = gen_expr(cg, needle_e)?;
    let needle = crate::result::unwrap(cg, needle);
    let is_str = needle.ty == LType::Str;
    let boxed = box_to_i64(cg, needle.clone());

    let res = cg.fresh_reg();
    cg.emit(format!("{res} = alloca i1"));
    cg.emit(format!("store i1 0, i1* {res}"));

    let lp = open_list_loop(cg, &l.operand);
    let eq = cg.fresh_reg();
    if is_str {
        let ep = cg.fresh_reg();
        cg.emit(format!("{ep} = inttoptr i64 {} to i8*", lp.elem));
        let c = cg.call("i32", "strcmp", "i8*, i8*", &[&ep, &needle.operand]);
        cg.emit(format!("{eq} = icmp eq i32 {c}, 0"));
    } else {
        cg.emit(format!("{eq} = icmp eq i64 {}, {}", lp.elem, boxed.operand));
    }
    let found = cg.fresh_label();
    let cont = cg.fresh_label();
    cg.emit(format!("br i1 {eq}, label %{found}, label %{cont}"));
    cg.start_block(&found);
    cg.emit(format!("store i1 1, i1* {res}"));
    cg.emit(format!("br label %{cont}"));
    cg.start_block(&cont);
    close_list_loop(cg, &lp);

    let out = cg.fresh_reg();
    cg.emit(format!("{out} = load i1, i1* {res}"));
    Ok(Value::new(out, LType::I1))
}

/// `mapSet(m, k, v) -> Map`.
fn map_set(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = stored_boxed_arg(cg, args, 1)?;
    let v = stored_boxed_arg(cg, args, 2)?;
    let r = cg.call(
        "i8*",
        "osprey_map_set",
        "i8*, i64, i64",
        &[&m.operand, &k.operand, &v.operand],
    );
    Ok(own_handle(cg, Value::handle(r, MAP_OWNER)))
}

/// `mapRemove(m, k) -> Map`.
fn map_remove(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let r = cg.call(
        "i8*",
        "osprey_map_remove",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    Ok(own_handle(cg, Value::handle(r, MAP_OWNER)))
}

/// `mapContains(m, k) -> bool`.
fn map_contains(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let raw = cg.call(
        "i32",
        "osprey_map_contains",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    let r = cg.fresh_reg();
    cg.emit(format!("{r} = icmp ne i32 {raw}, 0"));
    Ok(Value::new(r, LType::I1))
}

/// `mapGet(m, k) -> Result<V, _>` gated on `osprey_map_contains`.
fn map_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    runtime_map_get(cg, &m, &k)
}

/// Emit `osprey_map_merge` on two already-evaluated map handles.
pub(crate) fn merge_handles(cg: &mut Codegen, a: &Value, b: &Value) -> Value {
    combine_handles(cg, a, b, "osprey_map_merge", MAP_OWNER)
}

/// Runtime list-builder protocol — `new` → `push`* → `seal`, shared by every
/// list-producing builtin (`mapList`/`filterList`, `mapKeys`/`mapValues`).
pub(crate) fn list_builder_new(cg: &mut Codegen) -> String {
    cg.call("i8*", "osprey_list_builder_new", "", &[])
}

pub(crate) fn list_builder_push(cg: &mut Codegen, bld: &str, elem: &str) {
    cg.call_void("osprey_list_builder_push", "i8*, i64", &[bld, elem]);
}

pub(crate) fn list_builder_seal(cg: &mut Codegen, bld: &str) -> Value {
    let v = Value::handle(
        cg.call("i8*", "osprey_list_builder_seal", "i8*", &[bld]),
        LIST_OWNER,
    );
    own_handle(cg, v)
}

/// `mapKeys`/`mapValues` → a `List` built by iterating the map.
fn map_to_list(cg: &mut Codegen, args: &[Expr], take_key: bool) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let bld = list_builder_new(cg);
    let iter = cg.call("i8*", "osprey_map_iter_new", "i8*", &[&m.operand]);
    let kp = cg.fresh_reg();
    cg.emit(format!("{kp} = alloca i64"));
    let vp = cg.fresh_reg();
    cg.emit(format!("{vp} = alloca i64"));

    let cond = cg.fresh_label();
    let body = cg.fresh_label();
    let endl = cg.fresh_label();
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&cond);
    let has = cg.call(
        "i32",
        "osprey_map_iter_next",
        "i8*, i64*, i64*",
        &[&iter, &kp, &vp],
    );
    let more = cg.fresh_reg();
    cg.emit(format!("{more} = icmp ne i32 {has}, 0"));
    cg.emit(format!("br i1 {more}, label %{body}, label %{endl}"));

    cg.start_block(&body);
    let slot = if take_key { &kp } else { &vp };
    let elem = cg.fresh_reg();
    cg.emit(format!("{elem} = load i64, i64* {slot}"));
    list_builder_push(cg, &bld, &elem);
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&endl);
    Ok(list_builder_seal(cg, &bld))
}

/// `{ k: v, … }` — build a runtime map (string keys) via the map builder.
pub(crate) fn gen_map_literal(cg: &mut Codegen, entries: &[osprey_ast::MapEntry]) -> Result<Value> {
    // OSPREY_KEY_STRING = 1.
    let bld = cg.call("i8*", "osprey_map_builder_new", "i32", &["1"]);
    for e in entries {
        let k = gen_expr(cg, &e.key)?;
        let k = crate::result::unwrap(cg, k);
        // The map stores both key and value: dup before the i64 erasure
        // (see stored_boxed_arg) [GC-ARC-PERCEUS].
        crate::arc::escape_retain(cg, &k);
        let k = box_to_i64(cg, k);
        let v = gen_expr(cg, &e.value)?;
        let v = crate::result::unwrap(cg, v);
        crate::arc::escape_retain(cg, &v);
        let v = box_to_i64(cg, v);
        cg.call_void(
            "osprey_map_builder_put",
            "i8*, i64, i64",
            &[&bld, &k.operand, &v.operand],
        );
    }
    let sealed = cg.call("i8*", "osprey_map_builder_seal", "i8*", &[&bld]);
    Ok(own_handle(cg, Value::handle(sealed, MAP_OWNER)))
}

/// Shared runtime map lookup → `Result<i64, _>` (also used by `m[key]` indexing).
pub(crate) fn runtime_map_get(cg: &mut Codegen, m: &Value, k: &Value) -> Result<Value> {
    let has = cg.call(
        "i32",
        "osprey_map_contains",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    let got = cg.call(
        "i64",
        "osprey_map_get",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    result_from_flag(cg, &has, &got, "mapGet: key not found")
}
