//! Records & union variants. Each constructed value is a heap block laid out as
//! `{ i64 tag, fields… }` (the leading tag is the variant index within its
//! union, `0` for a record), handed around as an `i8*` handle that carries its
//! Osprey owner type so field access and `match` can recover the layout.
//! Construction, record update, field access and anonymous object literals all
//! share this one block shape.

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
    // A `name { … }` where `name` is a bound variable is a record *update*, not
    // a constructor (the parser cannot tell them apart).
    if !cg.is_ctor(name) {
        if cg.lookup(name).is_some() {
            return gen_update(cg, name, fields);
        }
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
    // types present at this construction — the same per-instance layout an object
    // literal gets — so a later `r.field` recovers the real type instead of the
    // placeholder `T → i8*`. Generic union *variants* keep the tagged path below.
    let generic_record = !cg.ctor_type_params(name).is_empty()
        && cg.ctor_layout(name).is_some_and(|v| v.owner_is_record);
    if generic_record {
        return gen_object(cg, fields);
    }
    let view = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
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
        let v = gen_expr(cg, &fa.value)?;
        let v = crate::cast::coerce_to(cg, v, *fty)?;
        store_field(cg, &struct_ty, obj.as_str(), i + 1, *fty, &v.operand);
    }

    let handle = cg.fresh_reg();
    cg.emit(format!("{handle} = bitcast {struct_ty}* {obj} to i8*"));
    let v = Value::handle(handle, view.owner);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// `{ field: value, … }` — an anonymous object literal: the same `{ i64 tag,
/// fields… }` heap block as a named record, with a synthetic layout registered so
/// field access can recover the slots.
pub(crate) fn gen_object(cg: &mut Codegen, fields: &[FieldAssignment]) -> Result<Value> {
    let mut vals = Vec::with_capacity(fields.len());
    for fa in fields {
        let v = gen_expr(cg, &fa.value)?;
        vals.push((fa.name.clone(), v));
    }
    let mut parts = vec!["i64".to_string()];
    parts.extend(vals.iter().map(|(_, v)| v.ty.as_str().to_string()));
    let struct_ty = format!("{{ {} }}", parts.join(", "));
    let layout: Vec<(String, LType)> = vals.iter().map(|(n, v)| (n.clone(), v.ty)).collect();
    let meta = tagged_fields_meta(&layout);
    let owner = cg.register_obj_layout(layout);

    // noinit: the tag and every field are stored below before the block
    // escapes, so ARC skips its drop-safety pre-zero.
    let obj = cg.malloc_struct_noinit(&struct_ty, meta);
    store_tag(cg, &struct_ty, obj.as_str(), 0);
    for (i, (_, v)) in vals.iter().enumerate() {
        store_field(cg, &struct_ty, obj.as_str(), i + 1, v.ty, &v.operand);
    }
    let handle = cg.fresh_reg();
    cg.emit(format!("{handle} = bitcast {struct_ty}* {obj} to i8*"));
    let v = Value::handle(handle, owner);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// The layout word for an ANONYMOUS-object block `{ i64 tag, fields… }`
/// ([`crate::meta`]), from runtime value `LTypes` (named constructors carry the
/// stronger Osprey-typed `CtorView::meta` instead): the leading discriminant
/// is a scalar word; each field marks itself by its LLVM type. Generic-variant
/// slots boxed into `i64` stay unmarked — leak-safe (meta.rs [GC-ARC-PERCEUS]).
fn tagged_fields_meta(fields: &[(String, LType)]) -> i64 {
    let mut mf = Vec::with_capacity(fields.len() + 1);
    mf.push(crate::meta::MetaField::Word);
    mf.extend(
        fields
            .iter()
            .map(|(_, t)| crate::meta::MetaField::of_lty(*t)),
    );
    crate::meta::struct_meta(&mf)
}

/// The built-in HTTP response record name.
const HTTP_RESPONSE: &str = "HttpResponse";

/// `{ i64 status, i8* headers, i8* contentType, i64 streamFd, i8 isComplete,
/// i8* partialBody }` — the C `struct HttpResponse` (`runtime/http_shared.h`),
/// the one record returned across the FFI boundary. Field LLVM types in layout
/// order; `isComplete` is the C `bool`, an `i8`.
const HTTP_RESPONSE_STRUCT: &str = "{ i64, i8*, i8*, i64, i8, i8* }";
const HTTP_RESPONSE_FIELDS: [(&str, &str); 6] = [
    ("status", "i64"),
    ("headers", "i8*"),
    ("contentType", "i8*"),
    ("streamFd", "i64"),
    ("isComplete", "i8"),
    ("partialBody", "i8*"),
];

/// Construct an `HttpResponse` in the exact C layout and return the `i8*` a
/// request handler hands back to the runtime. Unlike a generic record there is
/// **no leading tag**, and the boolean `isComplete` widens to `i8`.
fn gen_http_response(cg: &mut Codegen, fields: &[FieldAssignment]) -> Result<Value> {
    // Layout word for the fixed C ABI: string pointers at words 1, 2 and 5
    // (headers / contentType / partialBody) — pinned by meta.rs unit tests.
    let meta = crate::meta::struct_meta(
        &HTTP_RESPONSE_FIELDS.map(|(_, llty)| crate::meta::MetaField::of_slot_ty(llty)),
    );
    let obj = cg.malloc_struct(HTTP_RESPONSE_STRUCT, meta);
    for (i, (fname, llty)) in HTTP_RESPONSE_FIELDS.iter().enumerate() {
        let fa = fields.iter().find(|f| &f.name == fname).ok_or_else(|| {
            CodegenError::invalid(format!("missing field `{fname}` for `{HTTP_RESPONSE}`"))
        })?;
        let v = gen_expr(cg, &fa.value)?;
        let operand = match *llty {
            // C `bool` is one byte; widen the i1.
            "i8" => {
                let b = crate::conv::as_i1(cg, v)?;
                cg.emit_reg(format!("zext i1 {} to i8", b.operand))
            }
            "i64" => crate::conv::as_i64(cg, v)?.operand,
            _ => crate::cast::coerce_to(cg, v, LType::Str)?.operand,
        };
        crate::arc::dup_store(cg, llty, &operand);
        let p = cg.fresh_reg();
        cg.emit(format!(
            "{p} = getelementptr {HTTP_RESPONSE_STRUCT}, {HTTP_RESPONSE_STRUCT}* {obj}, i32 0, i32 {i}"
        ));
        cg.emit(format!("store {llty} {operand}, {llty}* {p}"));
    }
    let handle = cg.emit_reg(format!("bitcast {HTTP_RESPONSE_STRUCT}* {obj} to i8*"));
    let v = Value::handle(handle, HTTP_RESPONSE);
    crate::arc::own(cg, &v);
    Ok(v)
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
    crate::result::make_result(cg, v, inner, disc, &errmsg)
}

/// `record { field: newValue }` — copy every field of `record` into a fresh
/// block, overriding the named ones.
pub(crate) fn gen_update(
    cg: &mut Codegen,
    record: &str,
    fields: &[FieldAssignment],
) -> Result<Value> {
    let base = cg
        .lookup(record)
        .ok_or_else(|| CodegenError::unknown(record))?;
    let owner = base
        .osp_ty
        .clone()
        .ok_or_else(|| CodegenError::invalid(format!("`{record}` is not a record")))?;
    let view = cg
        .ctor_layout(&owner)
        .ok_or_else(|| CodegenError::unknown(&owner))?;
    let struct_ty = cg
        .ctor_struct_ty(&owner)
        .ok_or_else(|| CodegenError::unknown(&owner))?;

    let src = cg.fresh_reg();
    cg.emit(format!(
        "{src} = bitcast i8* {} to {struct_ty}*",
        base.operand
    ));
    // view.meta comes from the Osprey field types (builder.rs `field_meta`),
    // which prove more than the erased LTypes visible here: an all-union field
    // set upgrades to the probe-free KIND_MASK_DIRECT. noinit: the tag and every
    // field are stored below (new value or copied from the base) before the
    // block escapes, so ARC skips its drop-safety pre-zero.
    let obj = cg.malloc_struct_noinit(&struct_ty, view.meta);
    store_tag(cg, &struct_ty, obj.as_str(), view.tag);

    for (i, (fname, fty)) in view.fields.iter().enumerate() {
        let val = match fields.iter().find(|f| &f.name == fname) {
            Some(fa) => {
                let v = gen_expr(cg, &fa.value)?;
                crate::cast::coerce_to(cg, v, *fty)?.operand
            }
            None => load_field(cg, &struct_ty, src.as_str(), i + 1, *fty),
        };
        store_field(cg, &struct_ty, obj.as_str(), i + 1, *fty, &val);
    }

    let handle = cg.fresh_reg();
    cg.emit(format!("{handle} = bitcast {struct_ty}* {obj} to i8*"));
    let v = Value::handle(handle, owner);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// `obj.field` — recover the record layout from the handle's owner type and
/// load the field.
pub(crate) fn gen_field_access(cg: &mut Codegen, target: &Expr, field: &str) -> Result<Value> {
    let tv = gen_expr(cg, target)?;
    // Use the statically-known owner (a named record or an anonymous object
    // literal) when it actually declares `field`; otherwise (a generic accessor
    // whose parameter infers to a type variable) resolve the field by name across
    // known layouts — the polymorphic field-access fallback (`find_field_owner`).
    let known = tv.osp_ty.clone().filter(|o| {
        cg.record_layout(o)
            .is_some_and(|(_, fs)| fs.iter().any(|(f, _)| f == field))
    });
    let owner = known
        .or_else(|| cg.find_field_owner(field))
        .ok_or_else(|| CodegenError::invalid(format!("field `{field}` on a non-record")))?;
    let (struct_ty, fields_layout) = cg
        .record_layout(&owner)
        .ok_or_else(|| CodegenError::unknown(&owner))?;
    let (idx, fty) = fields_layout
        .iter()
        .enumerate()
        .find_map(|(i, (f, t))| (f == field).then_some((i, *t)))
        .ok_or_else(|| CodegenError::invalid(format!("`{owner}` has no field `{field}`")))?;

    let src = cg.fresh_reg();
    cg.emit(format!(
        "{src} = bitcast i8* {} to {struct_ty}*",
        tv.operand
    ));
    let loaded = load_field(cg, &struct_ty, src.as_str(), idx + 1, fty);
    let owner = cg.ctor_field_owner(&owner, field);
    Ok(Value::new(loaded, fty).with_owner(owner))
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
    let p = cg.fresh_reg();
    cg.emit(format!(
        "{p} = getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 {idx}"
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
    let p = cg.fresh_reg();
    cg.emit(format!(
        "{p} = getelementptr {struct_ty}, {struct_ty}* {obj}, i32 0, i32 {idx}"
    ));
    let r = cg.fresh_reg();
    cg.emit(format!("{r} = load {fty}, {fty}* {p}"));
    r
}
