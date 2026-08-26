//! List literals (`[a, b, c]`) and index access (`xs[i]`, `m[k]`). A list
//! literal lowers to a flat heap block `{ i64 length, i8* data }` where `data`
//! is a malloc'd array of element-typed slots — distinct from the runtime
//! `OspreyList` handle (the two share only their leading `i64 length`, which is
//! why `listLength` reads both). Index access bounds-checks and returns a
//! `Result<T, Error>`.
//!
//! Implements the indexing spellings of [BUILTIN-LIST-GET] (`list[index]`, the
//! `get(list, index)` equivalent) and [BUILTIN-MAP-GET] (`map[key]`, which
//! delegates to the map runtime) — docs/specs/0012-Built-InFunctions.md.

use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::result::make_result;
use osprey_ast::Expr;

/// Owner-tag prefix marking a flat list-literal handle; the suffix is the
/// element type's LLVM spelling, so index access can reload it.
const LIST_LIT: &str = "[]";
const LIST_STRUCT: &str = "{ i64, i8* }";

/// The owner tag for a `{ i64 length, char **items }` block minted by the C
/// string runtime (`osp_string_lines` / `split` / `words`). That struct IS the
/// flat literal layout with `i8*` elements, NOT an `OspreyList` — tagging it
/// `List` claimed a runtime handle it never was, so `forEachList(lines(s), f)`
/// handed `osprey_list_get` a foreign header and segfaulted.
pub(crate) const STRING_LIST_OWNER: &str = "[]i8*";

/// Tag a flat list-literal handle with its element. A scalar element records its
/// LLVM spelling (`[]i64`); a handle element (a nested list, a record) records
/// its own owner so access can recover it (`[][]i64`, `[]Point`).
fn lit_owner(elem: &Value) -> String {
    // Every container spells its element the same way, so a literal and the
    // runtime list it converts into agree on what a slot holds
    // ([`crate::llty::elem_spelling`]). A literal is recognised BY its tag
    // ([`is_lit`]), so an element with no spelling still records the storage
    // width it will be reloaded at.
    lit_owner_of(
        &crate::llty::elem_spelling(elem).unwrap_or_else(|| LType::I64.as_str().to_string()),
    )
}

/// A flat literal's owner tag for an already-chosen element spelling.
fn lit_owner_of(spelling: &str) -> String {
    format!("{LIST_LIT}{spelling}")
}

/// The element of a flat list-literal handle: its storage [`LType`] and, for a
/// handle element, the owner type to re-tag the loaded value with (so nested
/// lists / records stay indexable / field-accessible).
fn lit_elem(osp_ty: Option<&str>) -> Option<(LType, Option<String>)> {
    let suffix = osp_ty?.strip_prefix(LIST_LIT)?;
    Some(crate::llty::elem_of_spelling(suffix))
}

/// The element count of a flat list-literal handle, or `None` when `v` is not
/// one. The literal layout is **not** an `OspreyList`, so a receiver-directed
/// `length` / `isEmpty` ([`crate::collections::gen_receiver_directed`]) must
/// read the leading `i64` here. Falling through to the string runtime instead
/// ran `osp_strlen` over the length word's own bytes, so `length([1, 2, 3])`
/// answered `1` — the NUL-free byte count of little-endian `3`.
pub(crate) fn lit_length(cg: &mut Codegen, v: &Value) -> Option<Value> {
    if !is_lit(v) {
        return None;
    }
    let len = crate::aggregate::load_field(cg, LIST_STRUCT, &v.operand, 0, LType::I64);
    Some(Value::new(len, LType::I64))
}

/// `true` when `v` is a flat list-literal handle rather than an `OspreyList`.
/// The two layouts answer to different runtimes, so any builtin that hands a
/// list to the `osprey_list_*` C API must route a literal through
/// [`to_runtime_list`] first — see [`lit_length`] for what happens when it does
/// not.
pub(crate) fn is_lit(v: &Value) -> bool {
    lit_elem(v.osp_ty.as_deref()).is_some()
}

/// The element tag inference resolved for the list literal at `position`, when
/// it resolved to something concrete. A still-polymorphic element has no
/// representation to record.
pub(crate) fn inferred_elem(
    cg: &Codegen,
    position: Option<osprey_ast::Position>,
) -> Option<String> {
    crate::types::elem_tag(&cg.prog, cg.prog.list_elem_type(position))
}

/// `[e0, e1, …]` → a flat `{ length, data }` block. `position` is the literal's
/// own source position, which an EMPTY literal needs: with no element to lower,
/// the element type inference published there is the only thing that can tag
/// the handle, and an untagged empty list reads back as `int` however it was
/// declared ([GPU-BUFFER-ELEM]).
pub(crate) fn gen_list(
    cg: &mut Codegen,
    elements: &[Expr],
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    if elements.is_empty() {
        // No data block, no elements: nothing for the drop walk to release.
        let obj = cg.malloc_struct(LIST_STRUCT, crate::meta::list_hdr_meta(false));
        crate::aggregate::store_field(cg, LIST_STRUCT, &obj, 0, LType::I64, "0");
        crate::aggregate::store_field(cg, LIST_STRUCT, &obj, 1, LType::Str, "null");
        // An empty literal has no element to read a spelling off, so inference
        // supplies one; `i8*` is the historical fallback when even that is
        // still polymorphic.
        let elem = inferred_elem(cg, position).unwrap_or_else(|| LType::Str.as_str().to_string());
        let v = Value::handle(obj, lit_owner_of(&elem));
        crate::arc::own(cg, &v);
        return Ok(v);
    }
    // Evaluate elements; the first fixes the slot type. A nested literal
    // ESCAPES into its slot: the flat layout is a codegen-local optimization
    // whose tag rides on the value, and the container it lands in may later be
    // described only by its TYPE — `List<List<int>>` promises runtime lists, so
    // a slot holding a `{ length, data }` block would be handed to
    // `osprey_list_*` as an `OspreyList` and segfault ([`escaping`]).
    let mut vals = Vec::with_capacity(elements.len());
    for e in elements {
        let v = gen_expr(cg, e)?;
        vals.push(escaping(cg, v));
    }
    // The first element fixes the slot type; non-empty is guaranteed above.
    let Some(first) = vals.first() else {
        return Err(CodegenError::unsupported("empty list literal"));
    };
    let elem = match first.ty {
        LType::Double => LType::Double,
        LType::I1 => LType::I1,
        LType::Str | LType::Ptr => LType::Str,
        // An erased box keeps its own slot type: folding it into `I64` here
        // turned the box POINTER into an integer word, tagged the block an
        // integer list and marked its slots unmanaged, so the element neither
        // rendered through its descriptor nor was ever released.
        LType::Any => LType::Any,
        _ => LType::I64,
    };
    // A handle element (nested list / record) carries its own owner so access can
    // recover it; scalars carry none.
    let elem_owner = first.osp_ty.clone();
    let n = elements.len();
    let data = cg.heap_alloc(&(n * 8).to_string());
    let arr = cg.fresh_reg();
    cg.emit(format!("{arr} = bitcast i8* {data} to {}*", elem.as_str()));
    for (i, v) in vals.into_iter().enumerate() {
        let v = coerce_to(cg, v, elem)?;
        // The header's drop releases pointer elements, so each store is a new
        // reference [GC-ARC-PERCEUS].
        crate::arc::dup_store(cg, elem.as_str(), &v.operand);
        let slot = cg.fresh_reg();
        cg.emit(format!(
            "{slot} = getelementptr {}, {}* {arr}, i64 {i}",
            elem.as_str(),
            elem.as_str()
        ));
        cg.emit(format!(
            "store {} {}, {}* {slot}",
            elem.as_str(),
            v.operand,
            elem.as_str()
        ));
    }
    // The header's kind tells the ARC drop walk whether data[0..len) holds
    // managed pointers (string/handle elements) or scalars ([`crate::meta`]).
    let elems_are_ptrs = elem.is_managed_ptr();
    let obj = cg.malloc_struct(LIST_STRUCT, crate::meta::list_hdr_meta(elems_are_ptrs));
    crate::aggregate::store_field(cg, LIST_STRUCT, &obj, 0, LType::I64, &n.to_string());
    // The data array is the header's OWN allocation (the LIST_HDR drop frees
    // it exactly once) — store it without a dup, unlike user-value fields.
    let dp = cg.emit_reg(format!(
        "getelementptr {LIST_STRUCT}, {LIST_STRUCT}* {obj}, i32 0, i32 1"
    ));
    cg.emit(format!("store i8* {data}, i8** {dp}"));
    let v = Value::handle(obj, lit_owner(&Value::new("", elem).with_owner(elem_owner)));
    crate::arc::own(cg, &v);
    Ok(v)
}

/// Rebuild a flat list-literal handle as a runtime `OspreyList`, or return the
/// value untouched when it is not a literal.
///
/// The two layouts share ONLY their leading `i64 length`. Handing a literal
/// straight to the list runtime therefore makes `osprey_list_get` read
/// `shift` / `tail_count` / `root` / `tail` / `offset` out of the bytes that
/// follow the length — the data POINTER — and dereference them: every one of
/// `forEachList([1,2], f)`, `listAppend([1,2], 3)`, `listReverse([1,2])` and
/// `listContains([1,2], 1)` segfaulted. `listLength` appeared to work only
/// because `length` is the first field of both layouts.
///
/// Elements are BORROWED from the literal, which keeps its own reference and
/// stays alive for the rest of the region, so the builder dups the ones it
/// keeps ([`crate::collections::list_builder_push_borrowed`]). Every builtin
/// that speaks to the list runtime funnels through the two argument helpers
/// that call this, so the conversion is expressed once. [BUILTIN-LIST-GET]
pub(crate) fn to_runtime_list(cg: &mut Codegen, v: Value) -> Value {
    let Some((elem, _)) = lit_elem(v.osp_ty.as_deref()) else {
        return v;
    };
    let len = crate::aggregate::load_field(cg, LIST_STRUCT, &v.operand, 0, LType::I64);
    let data = crate::aggregate::load_field(cg, LIST_STRUCT, &v.operand, 1, LType::Str);
    let managed = if elem.is_managed_ptr() { "1" } else { "0" };
    let bld = crate::collections::list_builder_new_of(cg, managed);
    let arr = cg.emit_reg(format!("bitcast i8* {data} to {}*", elem.as_str()));
    let lp = crate::loops::open_range_loop(cg, "0", &len);
    let slot = cg.emit_reg(format!(
        "getelementptr {0}, {0}* {arr}, i64 {1}",
        elem.as_str(),
        lp.i
    ));
    let raw = cg.emit_reg(format!("load {0}, {0}* {slot}", elem.as_str()));
    // The builder's slots are uniform i64 words, so a double or an i1 element
    // is widened the same way every other collection element is.
    let word = crate::conv::box_to_i64(cg, Value::new(raw, elem));
    crate::collections::list_builder_push_borrowed(cg, &bld, &word.operand);
    crate::loops::close_range_loop(cg, &lp);
    // The literal knew its element type; the runtime list keeps that knowledge
    // in its owner tag rather than losing it to the uniform `i64` element ABI
    // ([`crate::collections::LIST_TAG`]).
    let sealed = crate::collections::list_builder_seal(cg, &bld);
    sealed.with_owner(Some(crate::collections::list_owner(
        crate::llty::elem_of_tag(&v, LIST_LIT).as_deref(),
    )))
}

/// Normalize a list value that is about to ESCAPE the scope which knows its
/// representation — a function return, a record field, an object literal, a
/// lambda's return slot, or a channel send.
///
/// The flat layout is a codegen-local optimization whose element tag rides on
/// the VALUE (`[]double`), never on the type: `List<T>` has no owner tag
/// ([`crate::types::owner_name`]), so a receiver seeing only `List<T>` reads a
/// literal's `{ length, data }` header as an `OspreyList` — the exact
/// misreading [`to_runtime_list`] documents. `fn xs() = [1, 2, 3]` followed by
/// `listGet(xs(), 0)` therefore segfaulted, as did a record field holding a
/// literal, while the same literal used in place worked.
///
/// Converting at the escape keeps the fast path where the tag is visible (a
/// literal consumed in the scope that built it, `toGpu([1, 2, 3])`) and pays
/// for the runtime layout only when the value outlives that knowledge.
pub(crate) fn escaping(cg: &mut Codegen, v: Value) -> Value {
    if is_lit(&v) {
        return to_runtime_list(cg, v);
    }
    v
}

/// `target[index]` — flat list-literal access (bounds-checked `Result<T, _>`) or
/// runtime-map lookup.
pub(crate) fn gen_index(cg: &mut Codegen, target: &Expr, index: &Expr) -> Result<Value> {
    let tv = gen_expr(cg, target)?;
    let iv = gen_expr(cg, index)?;

    // A runtime map handle indexes through the C map runtime.
    if tv
        .osp_ty
        .as_deref()
        .is_some_and(crate::collections::is_map_owner)
    {
        let key = crate::cast::coerce_to(cg, iv, LType::Str)?;
        let k = crate::conv::box_to_i64(cg, key);
        return crate::collections::runtime_map_get(cg, &tv, &k);
    }

    // A runtime list handle indexes through the list runtime. `xs[i]` is
    // [BUILTIN-LIST-GET]'s other spelling, so it must accept every list the
    // named form does — a nested row read back out of a matrix is a runtime
    // handle, not the flat literal it was written as ([`escaping`]).
    if tv
        .osp_ty
        .as_deref()
        .is_some_and(crate::collections::is_list_owner)
    {
        let index = crate::conv::as_i64(cg, iv)?;
        return crate::collections::runtime_list_get(cg, &tv, &index);
    }

    let (elem, elem_owner) = lit_elem(tv.osp_ty.as_deref())
        .ok_or_else(|| CodegenError::unsupported("index of a non-list/map value"))?;
    let idx = crate::conv::as_i64(cg, iv)?;

    let len = crate::aggregate::load_field(cg, LIST_STRUCT, &tv.operand, 0, LType::I64);
    let data = crate::aggregate::load_field(cg, LIST_STRUCT, &tv.operand, 1, LType::Str);

    // bounds: 0 <= idx < length
    let ge0 = cg.fresh_reg();
    cg.emit(format!("{ge0} = icmp sge i64 {}, 0", idx.operand));
    let lt = cg.fresh_reg();
    cg.emit(format!("{lt} = icmp slt i64 {}, {len}", idx.operand));
    let ok = cg.fresh_reg();
    cg.emit(format!("{ok} = and i1 {ge0}, {lt}"));

    // Load only on the in-bounds path — the OOB / empty (`data == null`) path
    // must not dereference.
    let load_bb = cg.fresh_label();
    let oob_bb = cg.fresh_label();
    let cont = cg.fresh_label();
    cg.emit(format!("br i1 {ok}, label %{load_bb}, label %{oob_bb}"));

    cg.start_block(&load_bb);
    let arr = cg.fresh_reg();
    cg.emit(format!("{arr} = bitcast i8* {data} to {}*", elem.as_str()));
    let slot = cg.fresh_reg();
    cg.emit(format!(
        "{slot} = getelementptr {}, {}* {arr}, i64 {}",
        elem.as_str(),
        elem.as_str(),
        idx.operand
    ));
    let val = cg.fresh_reg();
    cg.emit(format!(
        "{val} = load {}, {}* {slot}",
        elem.as_str(),
        elem.as_str()
    ));
    cg.emit(format!("br label %{cont}"));

    cg.start_block(&oob_bb);
    cg.emit(format!("br label %{cont}"));

    cg.start_block(&cont);
    let zero = crate::llty::zero_literal(elem);
    let phi = cg.fresh_reg();
    cg.emit(format!(
        "{phi} = phi {} [ {val}, %{load_bb} ], [ {zero}, %{oob_bb} ]",
        elem.as_str()
    ));
    let disc = cg.fresh_reg();
    cg.emit(format!("{disc} = select i1 {ok}, i8 0, i8 1"));
    // `ok` is the in-bounds flag, so the message is selected on the failing path.
    let oob = cg.string_constant(crate::collections::INDEX_OOB);
    let errmsg = cg.emit_reg(format!("select i1 {ok}, i8* null, i8* {}", oob.operand));
    make_result(
        cg,
        Value::new(phi, elem).with_owner(elem_owner),
        elem,
        &disc,
        &errmsg,
    )
}
