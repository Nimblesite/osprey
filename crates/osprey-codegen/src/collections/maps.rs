//! Runtime map construction, lookup, mutation and iteration.
use super::{
    boxed_arg, combine_handles, element_word, gated_read, handle_arg, list_builder_new_of,
    list_builder_push_borrowed, list_builder_seal, list_owner, managed_flag, map_owner, own_handle,
    stored_boxed_arg, stored_element, tagged_map_elem, unboxed_arg, MAP_TAG,
};
use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::conv::box_to_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::Expr;

/// `mapSet(m, k, v) -> Map`. Both key and value are STORED; the key's kind is
/// already carried by the map's `OspreyKeyType`, so only the value's flag
/// crosses.
pub(super) fn map_set(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let (k, _) = stored_boxed_arg(cg, args, 1)?;
    // Read the value's type BEFORE the `i64` erasure: it is what the new
    // handle's tag records, and the only place it is still visible.
    let raw = unboxed_arg(cg, args, 2)?;
    let elem = crate::llty::elem_spelling(&raw).or_else(|| tagged_map_elem(&m));
    let (v, managed) = stored_element(cg, raw)?;
    let r = cg.call(
        "i8*",
        "osprey_map_set_of",
        "i8*, i64, i64, i32",
        &[&m.operand, &k.operand, &v.operand, managed],
    );
    Ok(own_handle(cg, Value::handle(r, map_owner(elem.as_deref()))))
}

/// `mapRemove(m, k) -> Map`.
pub(super) fn map_remove(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let r = cg.call(
        "i8*",
        "osprey_map_remove",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    Ok(own_handle(
        cg,
        Value::handle(r, map_owner(tagged_map_elem(&m).as_deref())),
    ))
}

/// `mapContains(m, k) -> bool`.
pub(super) fn map_contains(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let raw = cg.call(
        "i32",
        "osprey_map_contains",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    let r = cg.emit_reg(format!("icmp ne i32 {raw}, 0"));
    Ok(Value::new(r, LType::I1))
}

/// `mapGet(m, k) -> Result<V, _>` gated on `osprey_map_contains`.
/// Implements [TYPE-MAP-LOOKUP] and [BUILTIN-MAP-GET].
pub(super) fn map_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    runtime_map_get(cg, &m, &k)
}

/// `mapMerge(a, b) -> Map`, carrying whichever operand still knows the value
/// type — an empty `Map()` operand carries none.
pub(super) fn map_merge(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let a = handle_arg(cg, args, 0)?;
    let b = handle_arg(cg, args, 1)?;
    Ok(merge_handles(cg, &a, &b))
}

/// Emit `osprey_map_merge` on two already-evaluated map handles.
pub(crate) fn merge_handles(cg: &mut Codegen, a: &Value, b: &Value) -> Value {
    let owner = map_owner(tagged_map_elem(a).or_else(|| tagged_map_elem(b)).as_deref());
    combine_handles(cg, a, b, "osprey_map_merge", &owner)
}

/// `mapKeys`/`mapValues` → a `List` built by iterating the map.
pub(super) fn map_to_list(cg: &mut Codegen, args: &[Expr], take_key: bool) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    // The produced list OWNS its elements while the map still holds its own,
    // so the builder inherits the map's element kind and dups every entry it
    // copies out — without that the temporary list's death would free the
    // map's keys/values [GC-ARC-PERCEUS].
    let kind = if take_key {
        "osprey_map_key_managed"
    } else {
        "osprey_map_value_managed"
    };
    let managed = cg.call("i32", kind, "i8*", &[&m.operand]);
    let bld = list_builder_new_of(cg, &managed);
    let iter = cg.call("i8*", "osprey_map_iter_new", "i8*", &[&m.operand]);
    let kp = cg.hoist_slot("i64");
    let vp = cg.hoist_slot("i64");

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
    let more = cg.emit_reg(format!("icmp ne i32 {has}, 0"));
    cg.emit(format!("br i1 {more}, label %{body}, label %{endl}"));

    cg.start_block(&body);
    let slot = if take_key { &kp } else { &vp };
    let elem = cg.emit_reg(format!("load i64, i64* {slot}"));
    list_builder_push_borrowed(cg, &bld, &elem);
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&endl);
    // The cursor borrows the map and is dead once the walk ends; without this
    // every mapKeys/mapValues call leaks one OspreyMapIter on every backend.
    cg.call_void("osprey_map_iter_free", "i8*", &[&iter]);
    // Keys are the map's own string keys; values are whatever its tag records.
    // The produced list must say so, or its elements read back as raw words
    // [`LIST_TAG`].
    let elem = if take_key {
        Some(LType::Str.as_str().to_string())
    } else {
        tagged_map_elem(&m)
    };
    Ok(list_builder_seal(cg, &bld).with_owner(Some(list_owner(elem.as_deref()))))
}

/// `{ k: v, … }` — build a runtime string-key map [TYPE-MAP-LITERAL].
pub(crate) fn gen_map_literal(cg: &mut Codegen, entries: &[osprey_ast::MapEntry]) -> Result<Value> {
    // OSPREY_KEY_STRING = 1.
    let bld = cg.call("i8*", "osprey_map_builder_new", "i32", &["1"]);
    // The sealed handle's tag records the entries' value type; an empty
    // literal has none to record [`MAP_TAG`].
    let mut elem: Option<String> = None;
    for e in entries {
        let k = gen_expr(cg, &e.key)?;
        let k = coerce_to(cg, k, LType::Str)?;
        // The map stores both key and value: dup before the i64 erasure, and
        // hand the value's kind over so the sealed map walks it on death
        // (see stored_boxed_arg) [GC-ARC-PERCEUS].
        crate::arc::escape_retain(cg, &k);
        let k = box_to_i64(cg, k);
        // Stored, so a nested literal escapes into its slot for the same
        // reason it does in [`unboxed_arg`].
        let raw = gen_expr(cg, &e.value)?;
        let v = crate::listlit::escaping(cg, raw);
        if v.result_inner.is_some() {
            return Err(CodegenError::unsupported(
                "Result-valued map entries are not yet supported; handle the Result before storing it",
            ));
        }
        let managed = managed_flag(&v);
        crate::arc::escape_retain(cg, &v);
        elem = crate::llty::elem_spelling(&v);
        let v = box_to_i64(cg, v);
        cg.call_void(
            "osprey_map_builder_put_of",
            "i8*, i64, i64, i32",
            &[&bld, &k.operand, &v.operand, managed],
        );
    }
    let sealed = cg.call("i8*", "osprey_map_builder_seal", "i8*", &[&bld]);
    Ok(own_handle(
        cg,
        Value::handle(sealed, map_owner(elem.as_deref())),
    ))
}

/// Shared runtime map lookup → `Result<V, _>` (also used by `m[key]` indexing).
pub(crate) fn runtime_map_get(cg: &mut Codegen, m: &Value, k: &Value) -> Result<Value> {
    let (has, got) = gated_read(cg, m, k, "osprey_map_contains", "osprey_map_get");
    // The success payload is the VALUE's type, not the storage word's, so a
    // `Map<string, string>` yields `Result<string, _>` [`MAP_TAG`].
    let value = element_word(cg, m, &got, osprey_types::names::MAP, MAP_TAG);
    let inner = value.ty;
    let is_err = cg.emit_reg(format!("icmp eq i32 {has}, 0"));
    crate::result::make_result_if_err(cg, value, inner, &is_err, Some("mapGet: key not found"))
}
