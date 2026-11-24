//! Render a record or a list at its static type, the way it is written:
//! `{ code: 1, why: "empty" }`, `[1, 2]`. A string inside a value is quoted;
//! a string on its own is its text. Implements [BUILTIN-TOSTRING],
//! [BUILTIN-PRINT-FORM].

use crate::builder::Codegen;
use crate::error::Result;
use crate::llty::{LType, Value};

/// A heap value as text: a list or a record field by field, anything else
/// through its erased descriptor.
/// A type met again inside its own expansion (`type Tree = { kids:
/// List<Tree> }`) renders through its typed render function, which is
/// registered before its body, so the expansion ends.
pub(crate) fn handle(cg: &mut Codegen, v: Value) -> Result<Value> {
    let v = crate::listlit::escaping(cg, v);
    let owner = v.osp_ty.clone().unwrap_or_default();
    if !owner.is_empty() && cg.anys.showing.contains(&owner) {
        return crate::anybox::render_handle(cg, v, true);
    }
    cg.anys.showing.push(owner.clone());
    let shown = expand(cg, v, &owner);
    let _ = cg.anys.showing.pop();
    shown
}

/// A list or a record field by field, anything else through its erased
/// descriptor.
fn expand(cg: &mut Codegen, v: Value, owner: &str) -> Result<Value> {
    if crate::collections::is_list_owner(owner) {
        return list(cg, &v);
    }
    if cg.union_variants(owner).is_none() {
        if let Some((ty, fields)) = cg.record_layout(owner) {
            return record(cg, &v, owner, &ty, &fields);
        }
    }
    crate::anybox::render_handle(cg, v, false)
}

/// A value as it reads inside another: a string in quotes, the rest as
/// [`crate::runtime::to_string_value`] renders it.
pub(crate) fn nested(cg: &mut Codegen, v: Value) -> Result<Value> {
    if v.ty == LType::Str && v.result_inner.is_none() {
        return Ok(shown(cg, &v.operand, "1"));
    }
    crate::runtime::to_string_value(cg, v)
}

/// `{ code: 1, why: "empty" }`: each field read at its declared type.
fn record(
    cg: &mut Codegen,
    v: &Value,
    owner: &str,
    ty: &str,
    fields: &[(String, LType)],
) -> Result<Value> {
    let src = cg.emit_reg(format!("bitcast i8* {} to {ty}*", v.operand));
    let mut args = Vec::with_capacity(fields.len());
    for (i, (name, lty)) in fields.iter().enumerate() {
        let inferred = crate::aggregate::field_type(cg, v, owner, name);
        let loaded = crate::aggregate::load_record_field(cg, owner, ty, &src, i, *lty);
        let field = Value::new(loaded, *lty);
        let field = crate::aggregate::restore_field(cg, field, owner, name, inferred.as_ref());
        args.push(format!("i8* {}", nested(cg, field)?.operand));
    }
    let names: Vec<&String> = fields.iter().map(|(name, _)| name).collect();
    Ok(crate::runtime::format_sized(cg, &row_format(&names), &args))
}

/// The format of a row with these field names: `{ a: %s, b: %s }`, or `{}`.
pub(crate) fn row_format<S: AsRef<str>>(names: &[S]) -> String {
    if names.is_empty() {
        return "{}".to_owned();
    }
    let holes: Vec<String> = names
        .iter()
        .map(|n| format!("{}: %s", n.as_ref()))
        .collect();
    format!("{{ {} }}", holes.join(", "))
}

/// `[1, 2]`: every element rendered, joined by the runtime.
fn list(cg: &mut Codegen, l: &Value) -> Result<Value> {
    let bld = crate::collections::list_builder_new(cg);
    crate::iter::each_list_elem(cg, l, |cg, _, elem| {
        let text = nested(cg, elem)?;
        crate::iter::push_built(cg, &bld, text, "toString").map(|_| ())
    })?;
    let texts = crate::collections::list_builder_seal(cg, &bld);
    let sep = cg.string_constant(", ");
    let joined = cg.call(
        "i8*",
        "osp_string_join_list",
        "i8*, i8*",
        &[&texts.operand, &sep.operand],
    );
    let joined = Value::new(joined, LType::Str);
    crate::arc::own(cg, &joined);
    let arg = format!("i8* {}", joined.operand);
    Ok(crate::runtime::format_sized(cg, "[%s]", &[arg]))
}

/// `text` as it reads inside a value: quoted and escaped as its literal when
/// `quoted` is `1`, else a copy. Both answers are owned strings.
pub(crate) fn shown(cg: &mut Codegen, text: &str, quoted: &str) -> Value {
    let raw = cg.call("i8*", "osp_string_show", "i8*, i64", &[text, quoted]);
    let v = Value::new(raw, LType::Str);
    crate::arc::own(cg, &v);
    v
}
