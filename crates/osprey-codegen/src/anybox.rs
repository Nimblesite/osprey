//! The erased-`any` value representation. An erased value is a pointer to a
//! two-word box `{ i8* desc, i64 payload }` whose `desc` names a module-global
//! shape descriptor `{ i64 kind, i8* render }` — so ownership keys off the
//! shape instead of the machine word (#208, plan 0027 §6), rendering switches
//! on what the value IS instead of printing the word (finding D), and a
//! structural match narrows by comparing descriptor identity. Implements
//! [TYPE-ANY]; the narrowing half is [`crate::pattern`] ([PATTERN-STRUCTURAL]).
//!
//! A row erasure DEEP-boxes: the payload is a fresh `{ i8* … }` block holding
//! one child box per field, so every row with the same field names shares one
//! descriptor no matter how its source was laid out. That is what makes the
//! match-candidate table computable from declarations alone and keeps the
//! narrowing test a pointer comparison — no name lookup at run time.

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{comma_join, LType, Value};
use std::collections::HashMap;

/// LLVM spelling of the box.
const BOX_TY: &str = "{ i8*, i64 }";
/// LLVM spelling of a descriptor global.
const DESC_TY: &str = "{ i64, i8* }";
/// LLVM spelling of a render function: `(desc, payload) -> string`.
const RENDER_SIG: &str = "i8* (i8*, i64)";
/// The module-level `box -> string` entry every renderer chain goes through.
const TO_STRING_FN: &str = "osp.any.to_string";

/// Descriptor kinds — `desc[0]`, one per erasable shape family. Only [ROW]
/// participates in structural narrowing; every other kind declines each
/// field-naming arm, exactly as a scalar row should ([TYPE-ANY]).
const KIND_INT: i64 = 0;
const KIND_BOOL: i64 = 1;
const KIND_FLOAT: i64 = 2;
const KIND_STRING: i64 = 3;
const ROW: i64 = 4;
const KIND_UNION: i64 = 5;
const KIND_RESULT: i64 = 6;
const KIND_OPAQUE: i64 = 7;

/// The identity of a descriptor. One global per key; two erasure sites with
/// the same key share it, which is what descriptor-pointer narrowing relies
/// on.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum DescKey {
    Int,
    Bool,
    Float,
    Str,
    /// Field names in layout order — the ONLY identity a row has once erased.
    Row(Vec<String>),
    /// A union value; the variant is decided at run time by the block's tag.
    Union(String),
    /// A `Result<T, _>` block with this success-slot layout.
    ResultOf(LType),
    /// A shape rendering cannot see into: lists, maps, closures, foreign
    /// handles. `.0` is the placeholder it renders as.
    Opaque(&'static str),
}

/// Per-module erasure state carried by [`Codegen`].
#[derive(Default)]
pub(crate) struct AnyState {
    /// Descriptor key → emitted global name.
    descs: HashMap<DescKey, String>,
    /// Row-source owner → emitted deep-box function name.
    boxers: HashMap<String, String>,
    /// Whether `@osp.any.to_string` has been emitted.
    to_string_emitted: bool,
    /// Every row shape a declared record can erase to, in declaration order —
    /// the complete candidate table structural arms select from. Seeded
    /// before emission ([`seed_rows`]); erasing an anonymous record is
    /// rejected by the checker precisely so this table stays complete.
    rows: Vec<Vec<String>>,
}

/// Seed the match-candidate row table from the declared constructors: every
/// record (named or positional) contributes its field names in layout order.
/// Two records with the same names are ONE row — closed-row unification makes
/// them interchangeable, so their erasures must be indistinguishable too.
pub(crate) fn seed_rows(cg: &mut Codegen) {
    let mut rows: Vec<Vec<String>> = cg
        .prog
        .ctors
        .values()
        .filter(|c| c.owner_is_record && !c.fields.is_empty())
        .map(|c| c.fields.iter().map(|(f, _)| f.clone()).collect())
        .collect();
    rows.sort();
    rows.dedup();
    cg.anys.rows = rows;
}

/// The declared row shapes a structural pattern can select: exact name-set
/// equality for a closed pattern, superset for an open one ([TYPE-ROW]).
pub(crate) fn candidate_rows(cg: &Codegen, names: &[String], open: bool) -> Vec<Vec<String>> {
    let wanted: std::collections::BTreeSet<&str> = names.iter().map(String::as_str).collect();
    cg.anys
        .rows
        .iter()
        .filter(|row| {
            let have: std::collections::BTreeSet<&str> = row.iter().map(String::as_str).collect();
            if open {
                wanted.is_subset(&have)
            } else {
                wanted == have
            }
        })
        .cloned()
        .collect()
}

/// The LLVM constant expression for a descriptor global as an `i8*`.
pub(crate) fn desc_operand(name: &str) -> String {
    format!("bitcast ({DESC_TY}* @{name} to i8*)")
}

/// The descriptor global for `key`, emitting it (and its render function) on
/// first use. Registration precedes the renderer so a self-referential row
/// terminates.
pub(crate) fn descriptor(cg: &mut Codegen, key: &DescKey) -> Result<String> {
    if let Some(name) = cg.anys.descs.get(key) {
        return Ok(name.clone());
    }
    let slug = key_slug(key);
    let name = format!("osp.any.desc.{slug}");
    let render = format!("osp.any.str.{slug}");
    let _ = cg.anys.descs.insert(key.clone(), name.clone());
    cg.add_global(format!(
        "@{name} = private constant {DESC_TY} {{ i64 {}, i8* bitcast ({RENDER_SIG}* @{render} to i8*) }}",
        key_kind(key)
    ));
    emit_renderer(cg, key, &render)?;
    Ok(name)
}

fn key_kind(key: &DescKey) -> i64 {
    match key {
        DescKey::Int => KIND_INT,
        DescKey::Bool => KIND_BOOL,
        DescKey::Float => KIND_FLOAT,
        DescKey::Str => KIND_STRING,
        DescKey::Row(_) => ROW,
        DescKey::Union(_) => KIND_UNION,
        DescKey::ResultOf(_) => KIND_RESULT,
        DescKey::Opaque(_) => KIND_OPAQUE,
    }
}

/// A deterministic, LLVM-identifier-safe slug for a key. Field and type names
/// are Osprey identifiers, so joining with `.` stays inside LLVM's unquoted
/// alphabet; determinism is what keeps the two flavors byte-identical
/// ([FLAVOR-IR-EQUIV]).
fn key_slug(key: &DescKey) -> String {
    match key {
        DescKey::Int => "int".into(),
        DescKey::Bool => "bool".into(),
        DescKey::Float => "float".into(),
        DescKey::Str => "string".into(),
        DescKey::Row(names) => format!("row.{}", names.join(".")),
        DescKey::Union(owner) => format!("union.{owner}"),
        DescKey::ResultOf(inner) => format!("result.{}", inner.as_str().replace('*', "p")),
        DescKey::Opaque(label) => format!("opaque.{}", label.trim_matches(['<', '>'])),
    }
}

/// The LLVM struct spelling of a deep-boxed row payload: one `i8*` child box
/// per field.
pub(crate) fn row_block_ty(fields: usize) -> String {
    format!("{{ {} }}", vec!["i8*"; fields].join(", "))
}

/// Render an erased value: `@osp.any.to_string(box)`, owned by the current
/// region. The one entry point `toString`/`print`/interpolation use.
pub(crate) fn any_to_string(cg: &mut Codegen, v: &Value) -> Value {
    ensure_to_string(cg);
    owned_call(cg, LType::Str, TO_STRING_FN, &v.operand)
}

/// `call i8* @<callee>(i8* <arg>)` whose +1 result the current region owns —
/// the shared shape of every emitted `any` helper call.
fn owned_call(cg: &mut Codegen, ret: LType, callee: &str, arg: &str) -> Value {
    let reg = cg.emit_reg(format!("call i8* @{callee}(i8* {arg})"));
    let out = Value::new(reg, ret);
    crate::arc::own(cg, &out);
    out
}

/// Emit `@osp.any.to_string` once: load the descriptor and payload, then tail
/// into the descriptor's render function. A null box (a zeroed slot that never
/// held a value) renders as `null` rather than faulting.
fn ensure_to_string(cg: &mut Codegen) {
    if cg.anys.to_string_emitted {
        return;
    }
    cg.anys.to_string_emitted = true;
    let saved = cg.enter_nested_fn();
    let null_lbl = cg.fresh_label();
    let body_lbl = cg.fresh_label();
    let isnull = cg.emit_reg("icmp eq i8* %box, null");
    cg.emit(format!(
        "br i1 {isnull}, label %{null_lbl}, label %{body_lbl}"
    ));
    cg.start_block(&null_lbl);
    let none = cg.string_constant("null");
    cg.emit(format!("ret i8* {}", none.operand));
    cg.start_block(&body_lbl);
    let (desc, payload) = load_box(cg, "%box");
    let dt = cg.emit_reg(format!("bitcast i8* {desc} to {DESC_TY}*"));
    let rp = cg.emit_reg(format!(
        "getelementptr {DESC_TY}, {DESC_TY}* {dt}, i32 0, i32 1"
    ));
    let raw = cg.emit_reg(format!("load i8*, i8** {rp}"));
    let f = cg.emit_reg(format!("bitcast i8* {raw} to {RENDER_SIG}*"));
    let s = cg.emit_reg(format!("call i8* {f}(i8* {desc}, i64 {payload})"));
    cg.emit(format!("ret i8* {s}"));
    cg.exit_nested_fn(saved, "i8*", TO_STRING_FN, &[(LType::Ptr, "box".into())]);
}

/// Load a box's `(desc, payload)` pair from its `i8*` operand.
fn load_box(cg: &mut Codegen, operand: &str) -> (String, String) {
    let bt = cg.emit_reg(format!("bitcast i8* {operand} to {BOX_TY}*"));
    let dp = cg.emit_reg(format!(
        "getelementptr {BOX_TY}, {BOX_TY}* {bt}, i32 0, i32 0"
    ));
    let desc = cg.emit_reg(format!("load i8*, i8** {dp}"));
    let pp = cg.emit_reg(format!(
        "getelementptr {BOX_TY}, {BOX_TY}* {bt}, i32 0, i32 1"
    ));
    let payload = cg.emit_reg(format!("load i64, i64* {pp}"));
    (desc, payload)
}

/// The `(desc, payload)` of a box value in the CURRENT function — the
/// structural-match entry ([`crate::pattern`]).
pub(crate) fn open_box(cg: &mut Codegen, v: &Value) -> (String, String) {
    load_box(cg, &v.operand)
}

/// Box a value into its erased representation, transferring a freshly-owned
/// payload into the box (Perceus constructor transfer) or retaining a
/// borrowed one — the balance §6 proved a word-keyed rule can never strike.
pub(crate) fn box_any(cg: &mut Codegen, v: Value) -> Result<Value> {
    if v.ty == LType::Any {
        return Ok(v);
    }
    let v = crate::listlit::escaping(cg, v);
    let (key, payload, managed) = classify(cg, v)?;
    let desc = descriptor(cg, &key)?;
    Ok(build_box(cg, &desc, payload, managed))
}

/// Allocate and fill one `{ desc, payload }` box; the payload word is masked
/// managed exactly when it holds a heap pointer, so every backend's drop walk
/// releases through the box with machinery that already exists
/// [GC-ARC-PERCEUS].
fn build_box(cg: &mut Codegen, desc: &str, payload: Value, managed: bool) -> Value {
    let payload_meta = if managed {
        crate::meta::MetaField::PtrManaged
    } else {
        crate::meta::MetaField::Word
    };
    let meta = crate::meta::struct_meta(&[crate::meta::MetaField::PtrOpaque, payload_meta]);
    let obj = cg.malloc_struct_noinit(BOX_TY, meta);
    let dp = cg.emit_reg(format!(
        "getelementptr {BOX_TY}, {BOX_TY}* {obj}, i32 0, i32 0"
    ));
    cg.emit(format!("store i8* {}, i8** {dp}", desc_operand(desc)));
    if managed && !crate::arc::consume_into_store(cg, &payload.operand) {
        crate::arc::dup_store(cg, "i8*", &payload.operand);
    }
    let word = crate::conv::box_to_i64(cg, payload);
    let pp = cg.emit_reg(format!(
        "getelementptr {BOX_TY}, {BOX_TY}* {obj}, i32 0, i32 1"
    ));
    cg.emit(format!("store i64 {}, i64* {pp}", word.operand));
    let handle = cg.emit_reg(format!("bitcast {BOX_TY}* {obj} to i8*"));
    let out = Value::new(handle, LType::Any);
    crate::arc::own(cg, &out);
    out
}

/// The descriptor key, normalized payload and payload managedness of a value
/// crossing into `any`. Rows are deep-boxed here; a `Result` keeps its block.
fn classify(cg: &mut Codegen, v: Value) -> Result<(DescKey, Value, bool)> {
    if v.result_inner.is_some() {
        let inner = v.result_inner.unwrap_or(LType::I64);
        return Ok((DescKey::ResultOf(inner), v, true));
    }
    Ok(match v.ty {
        LType::I64 | LType::I32 => (DescKey::Int, v, false),
        LType::I1 => (DescKey::Bool, v, false),
        LType::Double => (DescKey::Float, v, false),
        LType::Str => (DescKey::Str, v, true),
        LType::Ptr => return classify_handle(cg, v),
        // `Any` returns before classify; every LType is covered above.
        LType::Any => (DescKey::Opaque("<any>"), v, true),
    })
}

/// Classify a `Ptr` value by its owner tag. A record deep-boxes; a union
/// keeps its tagged block (the variant is runtime data); everything the
/// renderer cannot truthfully walk becomes a named opaque.
fn classify_handle(cg: &mut Codegen, v: Value) -> Result<(DescKey, Value, bool)> {
    let owner = v.osp_ty.clone().unwrap_or_default();
    if is_opaque_owner(&owner) {
        return Ok((DescKey::Opaque(opaque_label(&owner)), v, true));
    }
    if cg.union_variants(&owner).is_some() {
        return Ok((DescKey::Union(owner), v, true));
    }
    if let Some((_, fields)) = cg.record_layout(&owner) {
        let names: Vec<String> = fields.iter().map(|(f, _)| f.clone()).collect();
        if !cg.anys.rows.contains(&names) {
            // The checker rejects erasing an anonymous record precisely so
            // the candidate table stays complete; reaching here means it
            // leaked one through.
            return Err(CodegenError::invalid(format!(
                "cannot erase `{owner}`: its row {{ {} }} is not a declared record",
                names.join(", ")
            )));
        }
        let block = call_row_boxer(cg, &owner, &v)?;
        return Ok((DescKey::Row(names), block, true));
    }
    Ok((DescKey::Opaque(opaque_label(&owner)), v, true))
}

/// Owners whose heap layout is not the `{ i64 tag, fields… }` record block —
/// erasing one must never be walked as a row.
fn is_opaque_owner(owner: &str) -> bool {
    owner.is_empty()
        || owner == "HttpResponse"
        || owner == crate::collections::LIST_OWNER
        || owner == crate::collections::MAP_OWNER
        || owner.starts_with("[]")
        || owner.starts_with(crate::collections::LIST_TAG)
        || owner.starts_with(crate::gpu::GPU_TAG)
        || owner.starts_with("Map#")
        || owner == "GpuBuffer"
}

/// The rendering placeholder for an opaque owner. Truthful about the KIND of
/// value without pretending to know its contents — never the raw word.
fn opaque_label(owner: &str) -> &'static str {
    if owner.starts_with("[]") || owner.starts_with(crate::collections::LIST_TAG) {
        "<list>"
    } else if owner == crate::collections::MAP_OWNER || owner.starts_with("Map#") {
        "<map>"
    } else if owner.starts_with(crate::gpu::GPU_TAG) || owner == "GpuBuffer" {
        "<gpu>"
    } else if owner == crate::collections::LIST_OWNER {
        "<list>"
    } else {
        "<handle>"
    }
}

/// Call (emitting on first use) the deep-box function for `owner`'s layout:
/// `i8* @osp.any.boxrow.<owner>(i8* src)` returns a fresh row block whose
/// slots hold one child box per field. A function rather than inline code so
/// a recursive record type terminates at emission time.
fn call_row_boxer(cg: &mut Codegen, owner: &str, v: &Value) -> Result<Value> {
    let name = ensure_row_boxer(cg, owner)?;
    Ok(owned_call(cg, LType::Ptr, &name, &v.operand))
}

fn ensure_row_boxer(cg: &mut Codegen, owner: &str) -> Result<String> {
    if let Some(name) = cg.anys.boxers.get(owner) {
        return Ok(name.clone());
    }
    let name = format!("osp.any.boxrow.{owner}");
    let _ = cg.anys.boxers.insert(owner.to_string(), name.clone());
    let Some((src_ty, fields)) = cg.record_layout(owner) else {
        return Err(CodegenError::unknown(owner));
    };
    let saved = cg.enter_nested_fn();
    let row_ty = row_block_ty(fields.len());
    let meta = crate::meta::struct_meta(&field_metas(fields.len()));
    let block = cg.malloc_struct_noinit(&row_ty, meta);
    let src = cg.emit_reg(format!("bitcast i8* %src to {src_ty}*"));
    for (i, (fname, fty)) in fields.iter().enumerate() {
        let child = boxed_field(cg, &src_ty, &src, owner, i, fname, *fty)?;
        crate::aggregate::store_field(cg, &row_ty, &block, i, LType::Any, &child.operand);
    }
    let handle = cg.emit_reg(format!("bitcast {row_ty}* {block} to i8*"));
    let ret = Value::new(handle, LType::Ptr);
    crate::arc::own(cg, &ret);
    crate::arc::epilogue(cg, Some(&ret));
    cg.emit(format!("ret i8* {}", ret.operand));
    cg.exit_nested_fn(saved, "i8*", &name, &[(LType::Ptr, "src".into())]);
    Ok(name)
}

/// Load field `i` of a `{ i64 tag, fields… }` block and box it — the shared
/// step of deep-boxing a row and rendering a union variant's payload.
fn boxed_field(
    cg: &mut Codegen,
    struct_ty: &str,
    src: &str,
    owner: &str,
    i: usize,
    fname: &str,
    fty: LType,
) -> Result<Value> {
    let loaded = crate::aggregate::load_field(cg, struct_ty, src, i + 1, fty);
    let field_owner = cg.ctor_field_owner(owner, fname);
    box_any(cg, Value::new(loaded, fty).with_owner(field_owner))
}

/// The meta fields of a deep-boxed row block: every slot is a managed child
/// box.
fn field_metas(fields: usize) -> Vec<crate::meta::MetaField> {
    vec![crate::meta::MetaField::PtrManaged; fields]
}

/// Emit the render function for `key`:
/// `define i8* @osp.any.str.<slug>(i8* %desc, i64 %payload)`.
fn emit_renderer(cg: &mut Codegen, key: &DescKey, name: &str) -> Result<()> {
    let saved = cg.enter_nested_fn();
    let rendered = render_body(cg, key)?;
    crate::arc::epilogue(cg, Some(&rendered));
    cg.emit(format!("ret i8* {}", rendered.operand));
    cg.exit_nested_fn(
        saved,
        "i8*",
        name,
        &[(LType::Ptr, "desc".into()), (LType::I64, "payload".into())],
    );
    Ok(())
}

/// The body of a render function: `%payload` restored to the shape the kind
/// promises, rendered through the SAME code paths a concrete value uses —
/// one renderer per shape, zero parallel formatting logic.
fn render_body(cg: &mut Codegen, key: &DescKey) -> Result<Value> {
    match key {
        DescKey::Int => crate::runtime::to_string_value(cg, Value::new("%payload", LType::I64)),
        DescKey::Bool => {
            let b = crate::conv::unbox_from_i64(cg, "%payload", LType::I1);
            crate::runtime::to_string_value(cg, b)
        }
        DescKey::Float => {
            let d = crate::conv::unbox_from_i64(cg, "%payload", LType::Double);
            crate::runtime::to_string_value(cg, d)
        }
        DescKey::Str => {
            // The payload stays owned by the box; the epilogue's borrowed-
            // return retain hands the caller its own +1, so its release can
            // never free the string under the box. A second retain here
            // leaked one reference per rendering.
            Ok(crate::conv::unbox_from_i64(cg, "%payload", LType::Str))
        }
        DescKey::Row(names) => Ok(render_row(cg, &names.clone())),
        DescKey::Union(owner) => render_union(cg, &owner.clone()),
        DescKey::ResultOf(inner) => {
            let p = cg.emit_reg(format!(
                "inttoptr i64 %payload to {}*",
                crate::llty::result_struct_ty(*inner)
            ));
            crate::runtime::to_string_value(cg, Value::result(p, *inner))
        }
        DescKey::Opaque(label) => Ok(cg.string_constant(label)),
    }
}

/// `{ x: 1, y: 2 }` — each child box rendered through the shared entry, glued
/// by one exactly-sized format call.
fn render_row(cg: &mut Codegen, names: &[String]) -> Value {
    ensure_to_string(cg);
    let row_ty = row_block_ty(names.len());
    let block = cg.emit_reg(format!("inttoptr i64 %payload to {row_ty}*"));
    let mut args = Vec::with_capacity(names.len());
    for i in 0..names.len() {
        let child = crate::aggregate::load_field(cg, &row_ty, &block, i, LType::Any);
        args.push(rendered_arg(cg, &child));
    }
    let fmt = format!("{{ {} }}", comma_join(names, |n| format!("{n}: %s")));
    crate::runtime::format_sized(cg, &fmt, &args)
}

/// Render an already-boxed child through the shared entry, as one `i8*`
/// argument for a format call.
fn rendered_arg(cg: &mut Codegen, child: &str) -> String {
    let s = owned_call(cg, LType::Str, TO_STRING_FN, child);
    format!("i8* {}", s.operand)
}

/// `Leaf` / `Node(1, 2)` / `Circle { radius: 1.0 }` — switch on the union
/// block's leading tag and render the selected variant's declared fields.
fn render_union(cg: &mut Codegen, owner: &str) -> Result<Value> {
    let variants = cg.union_variants(owner).unwrap_or(&[]).to_vec();
    let block = cg.emit_reg("inttoptr i64 %payload to i64*".to_string());
    let tag = cg.emit_reg(format!("load i64, i64* {block}"));
    let end = cg.fresh_label();
    let mut phi_in: Vec<(String, String)> = Vec::new();
    for (i, variant) in variants.iter().enumerate() {
        let hit = cg.fresh_label();
        let next = cg.fresh_label();
        let cond = cg.emit_reg(format!("icmp eq i64 {tag}, {i}"));
        cg.emit(format!("br i1 {cond}, label %{hit}, label %{next}"));
        cg.start_block(&hit);
        let s = render_variant(cg, variant)?;
        let from = cg.snapshot_to(&end);
        phi_in.push((s.operand, from));
        cg.start_block(&next);
    }
    let fallback = cg.string_constant("<union>");
    let from = cg.snapshot_to(&end);
    phi_in.push((fallback.operand, from));
    cg.start_block(&end);
    let phi = comma_join(&phi_in, |(v, b)| format!("[ {v}, %{b} ]"));
    Ok(Value::new(
        cg.emit_reg(format!("phi i8* {phi}")),
        LType::Str,
    ))
}

/// One variant's rendering: nullary variants are their name; a payload lists
/// its fields — positional as `Name(a, b)`, named as `Name { f: a }`. Each
/// field is boxed and rendered through the shared entry, so nested shapes
/// stay truthful.
fn render_variant(cg: &mut Codegen, variant: &str) -> Result<Value> {
    let Some((view, struct_ty)) = cg
        .ctor_layout(variant)
        .filter(|v| !v.fields.is_empty())
        .zip(cg.ctor_struct_ty(variant))
    else {
        return Ok(cg.string_constant(variant));
    };
    ensure_to_string(cg);
    let src = cg.emit_reg(format!("inttoptr i64 %payload to {struct_ty}*"));
    let positional = view
        .fields
        .first()
        .is_some_and(|(f, _)| osprey_ast::is_positional_field(f));
    let mut args = Vec::new();
    for (i, (fname, fty)) in view.fields.iter().enumerate() {
        let child = boxed_field(cg, &struct_ty, &src, variant, i, fname, *fty)?;
        args.push(rendered_arg(cg, &child.operand));
    }
    let fmt = if positional {
        format!("{variant}({})", comma_join(&args, |_| "%s".into()))
    } else {
        let holes: Vec<&String> = view.fields.iter().map(|(f, _)| f).collect();
        format!(
            "{variant} {{ {} }}",
            comma_join(&holes, |f| format!("{f}: %s"))
        )
    };
    Ok(crate::runtime::format_sized(cg, &fmt, &args))
}
