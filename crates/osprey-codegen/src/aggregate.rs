//! Records & union variants. Each constructed value is a heap block laid out as
//! `{ i64 tag, fields… }` (the leading tag is the variant index within its
//! union, `0` for a record), handed around as an `i8*` handle that carries its
//! Osprey owner type so field access and `match` can recover the layout.
//! Construction, record update, field access and anonymous object literals all
//! share this one block shape.

mod debug;
mod fields;
mod update;
pub(crate) use update::gen_update;
mod http;
pub(crate) use debug::{record_fields as debug_record_fields, DebugRecord};
pub(crate) use fields::{field_type, gen_field_access};
use http::gen_http_response;
pub(crate) use http::{HTTP_RESPONSE, HTTP_RESPONSE_STRUCT};

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, FieldAssignment};

/// `Type { field: value, … }` — allocate the heap block, write the tag and each
/// declared field (in layout order), and return the owner-tagged handle.
pub(crate) fn gen_constructor(
    cg: &mut Codegen,
    name: &str,
    fields: &[FieldAssignment],
) -> Result<Value> {
    // Both frontends lower a lowercase brace head to `Expr::Update`, so a
    // constructor the program never declared is exactly that.
    if !cg.is_ctor(name) {
        return Err(CodegenError::unknown(name));
    }
    // `Success { value: x }` / `Error { message: m }` build the Result ABI block
    // `{ inner, i8 disc }` directly (disc 0 = Success), not a generic record —
    // so they interoperate with `match`, `toString` and effect handlers that
    // return `Result<…>` (e.g. an `input => Success { value: … }` handler arm).
    // The field disambiguates from a same-named *nullary* union variant (e.g.
    // `type TaskResult = Success | …`), which takes the ordinary union path.
    if (name == "Success" || name == "Error") && !fields.is_empty() {
        return gen_result_ctor(cg, name, fields);
    }
    // `HttpResponse` is handed straight to the C HTTP runtime, so it must use the
    // C `struct HttpResponse` layout (tag-free, `bool` as `i8`), not the generic
    // tagged-record block.
    if name == HTTP_RESPONSE {
        return gen_http_response(cg, fields);
    }
    // A generic *record* (`type R<T> = { … }`) is built with the concrete field
    // types present at this construction, because the declared layout types
    // every parameter slot as the machine word `T -> i64`. Generic union
    // *variants* keep the tagged path below.
    let generic_record = !cg.ctor_type_params(name).is_empty()
        && cg.ctor_layout(name).is_some_and(|v| v.owner_is_record);
    if generic_record {
        return gen_generic_record(cg, name, fields);
    }
    let view = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    if view
        .fields
        .iter()
        .any(|(field, _)| cg.ctor_field_result_inner(name, field).is_some())
    {
        return Err(result_field_unsupported());
    }
    // A payload-free union variant (`Leaf`, `None`) is one immutable value:
    // hand back the shared immortal singleton instead of a fresh heap block.
    // Records keep the heap path — `r.field` and record-update need a distinct
    // mutable-shaped block per value. [GC-ARC-PERCEUS]
    if !view.owner_is_record && view.fields.is_empty() {
        let handle = cg.nullary_singleton(name, view.tag);
        return Ok(Value::handle(handle, view.owner));
    }
    let struct_ty = cg
        .ctor_struct_ty(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    // view.meta comes from the Osprey field types (builder.rs `field_meta`),
    // which prove more than the erased LTypes visible here: an all-union field
    // set upgrades to the probe-free KIND_MASK_DIRECT. noinit: the tag and every
    // field below are stored before the block escapes, so ARC skips its
    // drop-safety pre-zero.
    let obj = cg.malloc_struct_noinit(&struct_ty, view.meta);
    store_tag(cg, &struct_ty, obj.as_str(), view.tag);

    // fields, in declared order
    for (i, (fname, fty)) in view.fields.iter().enumerate() {
        let fa = fields.iter().find(|f| &f.name == fname).ok_or_else(|| {
            CodegenError::invalid(format!("missing field `{fname}` for `{name}`"))
        })?;
        // A field slot is typed, not tagged: a list literal stored here would
        // be read back as an `OspreyList` [`crate::listlit::escaping`].
        let v = gen_expr(cg, &fa.value)?;
        let v = crate::listlit::escaping(cg, v);
        let v = crate::cast::coerce_to(cg, v, *fty)?;
        store_field(cg, &struct_ty, obj.as_str(), i + 1, *fty, &v.operand);
    }

    Ok(own_struct_handle(cg, &struct_ty, &obj, view.owner))
}

/// `{ field: value, … }` — an anonymous object literal: the same `{ i64 tag,
/// fields… }` heap block as a named record, with a synthetic layout registered so
/// field access can recover the slots.
pub(crate) fn gen_object(cg: &mut Codegen, fields: &[FieldAssignment]) -> Result<Value> {
    let vals = object_field_values(cg, fields, &field_names(fields))?;
    let owner = cg.register_obj_layout(layout_of(&vals));
    Ok(build_tagged_object(cg, owner, &vals))
}

/// A generic record instantiation. Its layout name is derived from the concrete
/// field types rather than minted per construction site: two `Envelope<int,
/// string>` values built in different places — a list element and the fallback
/// beside it — must share one owner, or the phi that joins them keeps neither,
/// `find_field_owner` falls back to the DECLARED layout whose `U` slot is a
/// machine word, and `r.metadata` reads a string pointer as an integer.
/// Implements [TYPE-GENERICS-DECL].
fn gen_generic_record(cg: &mut Codegen, name: &str, fields: &[FieldAssignment]) -> Result<Value> {
    let declared = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    let order: Vec<String> = declared.fields.iter().map(|(f, _)| f.clone()).collect();
    let vals = object_field_values(cg, fields, &order)?;
    let shape: Vec<&str> = vals.iter().map(|(_, v)| v.ty.as_str()).collect();
    let owner = cg.register_layout(format!("{name}#{}", shape.join(",")), layout_of(&vals));
    Ok(build_tagged_object(cg, owner, &vals))
}

/// The declared field order of a literal, when there is no declaration to
/// impose one.
fn field_names(fields: &[FieldAssignment]) -> Vec<String> {
    fields.iter().map(|fa| fa.name.clone()).collect()
}

/// Lower each field's value in `order`, so two constructions of one record
/// agree on their slots however the source spelled them.
fn object_field_values(
    cg: &mut Codegen,
    fields: &[FieldAssignment],
    order: &[String],
) -> Result<Vec<(String, Value)>> {
    let mut vals = Vec::with_capacity(order.len());
    for fname in order {
        let fa = fields
            .iter()
            .find(|f| &f.name == fname)
            .ok_or_else(|| CodegenError::invalid(format!("missing field `{fname}`")))?;
        let v = gen_expr(cg, &fa.value)?;
        let v = crate::listlit::escaping(cg, v);
        if v.result_inner.is_some() {
            return Err(result_field_unsupported());
        }
        vals.push((fname.clone(), v));
    }
    Ok(vals)
}

/// The registered layout of lowered field values: each slot's LLVM type plus
/// the owner tag the value carried, so field access restores it.
fn layout_of(vals: &[(String, Value)]) -> Vec<crate::builder::ObjField> {
    vals.iter()
        .map(|(n, v)| (n.clone(), v.ty, v.osp_ty.clone()))
        .collect()
}

/// Build a `{ i64 tag, fields… }` heap block under an already-registered owner.
fn build_tagged_object(cg: &mut Codegen, owner: String, vals: &[(String, Value)]) -> Value {
    let mut parts = vec!["i64".to_string()];
    parts.extend(vals.iter().map(|(_, v)| v.ty.as_str().to_string()));
    let struct_ty = format!("{{ {} }}", parts.join(", "));
    let meta = tagged_fields_meta(&layout_of(vals));
    // noinit: the tag and every field are stored below before the block
    // escapes, so ARC skips its drop-safety pre-zero.
    let obj = cg.malloc_struct_noinit(&struct_ty, meta);
    store_tag(cg, &struct_ty, obj.as_str(), 0);
    for (i, (_, v)) in vals.iter().enumerate() {
        store_field(cg, &struct_ty, obj.as_str(), i + 1, v.ty, &v.operand);
    }
    own_struct_handle(cg, &struct_ty, &obj, owner)
}

/// The layout word for an ANONYMOUS-object block `{ i64 tag, fields… }`
/// ([`crate::meta`]), from runtime value `LTypes` (named constructors carry the
/// stronger Osprey-typed `CtorView::meta` instead): the leading discriminant
/// is a scalar word; each field marks itself by its LLVM type. Generic-variant
/// slots boxed into `i64` stay unmarked — leak-safe (meta.rs [GC-ARC-PERCEUS]).
fn tagged_fields_meta(fields: &[crate::builder::ObjField]) -> i64 {
    let mut mf = Vec::with_capacity(fields.len() + 1);
    mf.push(crate::meta::MetaField::Word);
    mf.extend(
        fields
            .iter()
            .map(|(_, t, _)| crate::meta::MetaField::of_lty(*t)),
    );
    crate::meta::struct_meta(&mf)
}

/// Bitcast a freshly-built struct block to the `i8*` handle callers receive,
/// tag it with its Osprey `owner` type, and register it with ARC — the shared
/// tail every record/object constructor ends on.
fn own_struct_handle(
    cg: &mut Codegen,
    struct_ty: &str,
    obj: &str,
    owner: impl Into<String>,
) -> Value {
    let handle = cg.emit_reg(format!("bitcast {struct_ty}* {obj} to i8*"));
    let v = Value::handle(handle, owner);
    crate::arc::own(cg, &v);
    v
}

/// Build a `Success`/`Error` value in the Result ABI: the single field becomes
/// the block's success payload (slot 0), with disc `0` (Success) or `1`
/// (Error). For `Error { message: m }` the (string) message is also written to
/// the errmsg slot so the matching `Error { message }` arm and `toString` read
/// the real reason — see [`crate::result`]. Implements [ERR-PAYLOAD].
fn gen_result_ctor(cg: &mut Codegen, name: &str, fields: &[FieldAssignment]) -> Result<Value> {
    let fa = fields
        .first()
        .ok_or_else(|| CodegenError::invalid(format!("`{name}` needs one field")))?;
    let v = gen_expr(cg, &fa.value)?;
    let inner = v.ty;
    let is_error = name == "Error";
    let disc = if is_error { "1" } else { "0" };
    // The errmsg slot is `i8*`; only a string/handle message can travel there.
    let errmsg = if is_error && matches!(v.ty, LType::Str | LType::Ptr) {
        v.operand.clone()
    } else {
        crate::result::NO_MSG.to_string()
    };
    let result = crate::result::make_result(cg, v, inner, disc, &errmsg)?;
    Ok(if is_error {
        result.with_result_inner_placeholder()
    } else {
        result
    })
}

/// The heap block an update rebuilds: struct spelling, ordered slots, the
/// discriminant and the allocation meta word.
struct RecordBlock {
    struct_ty: String,
    fields: Vec<(String, LType)>,
    tag: Option<i64>,
    meta: i64,
}

/// A record owner's block — a declared constructor (its Osprey-typed
/// `CtorView::meta` proves more than the erased slots), or a registered
/// synthetic layout: an object literal or a generic-record instantiation such
/// as `Box#i64`, which construction names after its concrete field types
/// ([`gen_generic_record`]) and `ctor_layout` has never heard of. An
/// instantiation nothing in this module has built yet is derived from the
/// value's inferred record type and registered, exactly as construction would.
/// Implements [TYPE-RECORD-UPDATE].
fn record_block(
    cg: &mut Codegen,
    owner: &str,
    inferred: Option<&osprey_types::Type>,
) -> Option<RecordBlock> {
    if let Some(view) = cg.ctor_layout(owner) {
        return Some(RecordBlock {
            struct_ty: cg.ctor_struct_ty(owner)?,
            fields: view.fields,
            tag: record_has_tag(owner).then_some(view.tag),
            meta: if owner == HTTP_RESPONSE {
                http::layout_meta()
            } else {
                view.meta
            },
        });
    }
    if cg.obj_layout(owner).is_none() {
        let slots = derived_slots(cg, inferred?)?;
        let _ = cg.register_layout(owner.to_string(), slots);
    }
    let meta = tagged_fields_meta(cg.obj_layout(owner)?);
    let (struct_ty, fields) = cg.record_layout(owner)?;
    Some(RecordBlock {
        struct_ty,
        fields,
        tag: Some(0),
        meta,
    })
}

/// The slots of a generic-record instantiation from its inferred record type,
/// in declared field order — the layout `gen_generic_record` registers when it
/// builds one, derived here from the types alone.
fn derived_slots(
    cg: &Codegen,
    inferred: &osprey_types::Type,
) -> Option<Vec<crate::builder::ObjField>> {
    let (osprey_types::Type::Record { name, .. } | osprey_types::Type::Con { name, .. }) = inferred
    else {
        return None;
    };
    let declared = cg.ctor_layout(name)?;
    declared
        .fields
        .iter()
        .map(|(field, _)| {
            let ty = cg.prog.field_type(inferred, field)?;
            Some((
                field.clone(),
                crate::types::ltype_of(&ty),
                crate::types::owner_name(&cg.prog, &ty),
            ))
        })
        .collect()
}

/// Aggregate layouts do not yet carry the shape metadata needed to preserve a
/// Result field's discriminant and payload type. Rejecting is mandatory: a
/// broad pointer/scalar slot must never silently turn failure into success.
fn result_field_unsupported() -> CodegenError {
    CodegenError::unsupported(
        "Result-valued aggregate fields require a shape-aware layout; handle the Result before storing it",
    )
}

/// Store `val` (LLVM type `fty`) into the `idx`-th element of a `{TY}*` block.
pub(crate) fn store_field(
    cg: &mut Codegen,
    struct_ty: &str,
    obj: &str,
    idx: usize,
    fty: LType,
    val: &str,
) {
    // Dup-on-store: the block's drop mask releases pointer fields, so a stored
    // pointer is normally a new reference. But a freshly-produced owner this
    // region still holds is MOVED into the field instead — the Perceus
    // constructor transfer skips the dup and the region-end drop. [GC-ARC-PERCEUS]
    let moved = fty.as_str().ends_with('*')
        && val.starts_with('%')
        && crate::arc::consume_into_store(cg, val);
    if !moved {
        crate::arc::dup_store(cg, fty.as_str(), val);
    }
    let p = cg.emit_reg(format!(
        "getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 {idx}"
    ));
    cg.emit(format!("store {fty} {val}, {fty}* {p}"));
}

/// Write the leading variant tag into slot 0 of a `{ i64 tag, fields… }` block.
/// The tag is slot 0 with LLVM type `i64`, so this is `store_field` specialised —
/// named for the one job every record/variant/update block starts with.
fn store_tag(cg: &mut Codegen, struct_ty: &str, obj: &str, tag: i64) {
    store_field(cg, struct_ty, obj, 0, LType::I64, &tag.to_string());
}

/// Load the `idx`-th element of a `{TY}*` block, returning the value register.
pub(crate) fn load_field(
    cg: &mut Codegen,
    struct_ty: &str,
    obj: &str,
    idx: usize,
    fty: LType,
) -> String {
    let p = cg.emit_reg(format!(
        "getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 {idx}"
    ));
    let r = cg.emit_reg(format!("load {fty}, {fty}* {p}"));
    r
}

/// C ABI records omit Osprey's discriminant. [TYPE-RECORD-C-ABI]
pub(crate) fn record_has_tag(owner: &str) -> bool {
    owner != HTTP_RESPONSE
}

pub(crate) fn load_record_field(
    cg: &mut Codegen,
    owner: &str,
    ty: &str,
    source: &str,
    index: usize,
    field: LType,
) -> String {
    if owner == HTTP_RESPONSE && field == LType::I1 {
        http::load_bool(cg, ty, source, index)
    } else {
        load_field(
            cg,
            ty,
            source,
            index + usize::from(record_has_tag(owner)),
            field,
        )
    }
}

fn store_record_field(
    cg: &mut Codegen,
    owner: &str,
    ty: &str,
    target: &str,
    index: usize,
    field: LType,
    value: &str,
) {
    if owner == HTTP_RESPONSE && field == LType::I1 {
        http::store_bool(cg, ty, target, index, value);
    } else {
        store_field(
            cg,
            ty,
            target,
            index + usize::from(record_has_tag(owner)),
            field,
            value,
        );
    }
}
