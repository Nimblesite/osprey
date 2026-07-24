//! List literals (`[a, b, c]`) and index access (`xs[i]`, `m[k]`). A list
//! literal lowers to a flat heap block `{ i64 length, i8* data }` where `data`
//! is a malloc'd array of element-typed slots — distinct from the runtime
//! `OspreyList` handle (the two share only their leading `i64 length`, which is
//! why `listLength` reads both). Index access bounds-checks and returns a
//! `Result<T, IndexError>`.
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

/// Tag a flat list-literal handle with its element. A scalar element records its
/// LLVM spelling (`[]i64`); a handle element (a nested list, a record) records
/// its own owner so access can recover it (`[][]i64`, `[]Point`).
fn lit_owner(elem: &Value) -> String {
    match &elem.osp_ty {
        Some(o) => format!("{LIST_LIT}{o}"),
        None => format!("{LIST_LIT}{}", elem.ty.as_str()),
    }
}

/// The element of a flat list-literal handle: its storage [`LType`] and, for a
/// handle element, the owner type to re-tag the loaded value with (so nested
/// lists / records stay indexable / field-accessible).
fn lit_elem(osp_ty: Option<&str>) -> Option<(LType, Option<String>)> {
    let suffix = osp_ty?.strip_prefix(LIST_LIT)?;
    Some(match suffix {
        "i64" => (LType::I64, None),
        "double" => (LType::Double, None),
        "i1" => (LType::I1, None),
        "i8*" => (LType::Str, None),
        other => (LType::Str, Some(other.to_string())),
    })
}

/// `[e0, e1, …]` → a flat `{ length, data }` block.
pub(crate) fn gen_list(cg: &mut Codegen, elements: &[Expr]) -> Result<Value> {
    if elements.is_empty() {
        // No data block, no elements: nothing for the drop walk to release.
        let obj = cg.malloc_struct(LIST_STRUCT, crate::meta::list_hdr_meta(false));
        crate::aggregate::store_field(cg, LIST_STRUCT, &obj, 0, LType::I64, "0");
        crate::aggregate::store_field(cg, LIST_STRUCT, &obj, 1, LType::Str, "null");
        let v = Value::handle(obj, lit_owner(&Value::new("", LType::Str)));
        crate::arc::own(cg, &v);
        return Ok(v);
    }
    // Evaluate elements; the first fixes the slot type.
    let mut vals = Vec::with_capacity(elements.len());
    for e in elements {
        vals.push(gen_expr(cg, e)?);
    }
    // The first element fixes the slot type; non-empty is guaranteed above.
    let Some(first) = vals.first() else {
        return Err(CodegenError::unsupported("empty list literal"));
    };
    let elem = match first.ty {
        LType::Double => LType::Double,
        LType::I1 => LType::I1,
        LType::Str | LType::Ptr => LType::Str,
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
    let elems_are_ptrs = matches!(elem, LType::Str | LType::Ptr);
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

/// `target[index]` — flat list-literal access (bounds-checked `Result<T, _>`) or
/// runtime-map lookup.
pub(crate) fn gen_index(cg: &mut Codegen, target: &Expr, index: &Expr) -> Result<Value> {
    let tv = gen_expr(cg, target)?;
    let iv = gen_expr(cg, index)?;

    // A runtime map handle indexes through the C map runtime.
    if tv.osp_ty.as_deref() == Some(crate::collections::MAP_OWNER) {
        let k = crate::conv::box_to_i64(cg, iv);
        return crate::collections::runtime_map_get(cg, &tv, &k);
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
    let zero = match elem {
        LType::Str | LType::Ptr => "null",
        LType::Double => "0.0",
        LType::I1 => "false",
        _ => "0",
    };
    let phi = cg.fresh_reg();
    cg.emit(format!(
        "{phi} = phi {} [ {val}, %{load_bb} ], [ {zero}, %{oob_bb} ]",
        elem.as_str()
    ));
    let disc = cg.fresh_reg();
    cg.emit(format!("{disc} = select i1 {ok}, i8 0, i8 1"));
    // `ok` is the in-bounds flag, so the message is selected on the failing path.
    let oob = cg.string_constant("index out of bounds");
    let errmsg = cg.emit_reg(format!("select i1 {ok}, i8* null, i8* {}", oob.operand));
    make_result(
        cg,
        Value::new(phi, elem).with_owner(elem_owner),
        elem,
        &disc,
        &errmsg,
    )
}
