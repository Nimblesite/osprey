//! Render erased scalar, row and union values.
use super::{
    call_renderer, classify, comma_join, desc_operand, descriptor, ensure_to_string, owned_call,
    row_block_ty, typed, Codegen, DescKey, LType, Result, Value, TO_STRING_FN,
};
use osprey_types::numeric::Numeric;

/// Synchronous rendering borrows its payload until the call returns.
pub(crate) fn render_handle(cg: &mut Codegen, v: Value, recursive: bool) -> Result<Value> {
    let (key, payload) = if recursive {
        (typed::key(cg, &v), v)
    } else {
        let (key, payload, _) = classify(cg, v)?;
        (key, payload)
    };
    let desc = descriptor(cg, &key)?;
    let word = crate::conv::box_to_i64(cg, payload);
    let rendered = call_renderer(cg, &desc_operand(&desc), &format!("@{desc}"), &word.operand);
    let out = Value::new(rendered, LType::Str);
    crate::arc::own(cg, &out);
    Ok(out)
}

/// Emit the render function for `key`:
/// `define i8* @osp.any.str.<slug>(i8* %desc, i64 %payload)`.
pub(super) fn emit_renderer(cg: &mut Codegen, key: &DescKey, name: &str) -> Result<()> {
    let saved = cg.enter_nested_fn();
    // The body expands its own type afresh; only a type met again inside
    // that expansion calls back here.
    let showing = std::mem::take(&mut cg.anys.showing);
    let rendered = render_body(cg, key);
    cg.anys.showing = showing;
    let rendered = rendered?;
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
            let d = crate::conv::unbox_from_i64(cg, "%payload", LType::F64);
            crate::runtime::to_string_value(cg, d)
        }
        // A sized float's bits sit zero-extended in the payload word and
        // render through its own shortest-spelling function
        // [TYPE-NUMERIC-PRINT].
        DescKey::Float32 => {
            let f = crate::conv::unbox_from_i64(cg, "%payload", LType::Num(Numeric::FLOAT32));
            crate::runtime::to_string_value(cg, f)
        }
        DescKey::Float16 => {
            let h = crate::conv::unbox_from_i64(cg, "%payload", LType::Num(Numeric::FLOAT16));
            crate::runtime::to_string_value(cg, h)
        }
        DescKey::Str => {
            let text = crate::conv::unbox_from_i64(cg, "%payload", LType::Str);
            // The caller owns its rendering and the box still owns its text.
            // ARC shares the string through the normal return convention.
            if cg.ownership.on {
                Ok(crate::show::shown(cg, &text.operand, "0"))
            } else {
                Ok(text)
            }
        }
        DescKey::Row(names) => Ok(render_row(cg, &names.clone())),
        DescKey::Union(owner, key) => {
            let inferred = key
                .as_ref()
                .and_then(|key| cg.anys.union_types.get(key))
                .cloned();
            render_union(cg, owner, inferred.as_ref())
        }
        DescKey::Result => {
            let p = cg.emit_reg(format!(
                "inttoptr i64 %payload to {}*",
                crate::llty::RESULT_STRUCT
            ));
            crate::runtime::to_string_value(cg, Value::result(p, LType::Any))
        }
        DescKey::Opaque(label) => Ok(cg.string_constant(label)),
        DescKey::Typed(key) => {
            let proto = cg.anys.typed.get(key).cloned().unwrap_or_else(Value::unit);
            let whole = typed::restore(cg, "%payload", &proto);
            crate::runtime::to_string_value(cg, whole)
        }
    }
}

/// `{ x: 1, y: "a" }` — each child box rendered through the shared entry, glued
/// by one exactly-sized format call.
fn render_row(cg: &mut Codegen, names: &[String]) -> Value {
    ensure_show(cg);
    let row_ty = row_block_ty(names.len());
    let block = cg.emit_reg(format!("inttoptr i64 %payload to {row_ty}*"));
    let mut args = Vec::with_capacity(names.len());
    for i in 0..names.len() {
        let child = crate::aggregate::load_field(cg, &row_ty, &block, i, LType::Any);
        let shown = owned_call(cg, LType::Str, SHOW_FN, &child);
        args.push(format!("i8* {}", shown.operand));
    }
    crate::runtime::format_sized(cg, &crate::show::row_format(names), &args)
}

/// Emit `@osp.any.show` once: a box as it reads inside another value, which
/// is its rendering, in quotes when it holds a string.
fn ensure_show(cg: &mut Codegen) {
    ensure_to_string(cg);
    if cg.anys.show_emitted {
        return;
    }
    cg.anys.show_emitted = true;
    let saved = cg.enter_nested_fn();
    let (_, _, dt) = super::guard_box(cg);
    let kind = crate::aggregate::load_field(cg, super::DESC_TY, &dt, 0, LType::I64);
    let is_text = cg.emit_reg(format!("icmp eq i64 {kind}, {}", super::KIND_STRING));
    let quoted = cg.emit_reg(format!("zext i1 {is_text} to i64"));
    let text = owned_call(cg, LType::Str, TO_STRING_FN, "%box");
    let shown = crate::show::shown(cg, &text.operand, &quoted);
    crate::arc::epilogue(cg, Some(&shown));
    cg.emit(format!("ret i8* {}", shown.operand));
    cg.exit_nested_fn(saved, "i8*", SHOW_FN, &[(LType::Ptr, "box".into())]);
}

/// The emitted name of [`ensure_show`]'s function.
const SHOW_FN: &str = "osp.any.show";

/// `Leaf` / `Node(1, 2)` / `Circle { radius: 1.0 }` — switch on the union
/// block's leading tag and render the selected variant's declared fields.
fn render_union(
    cg: &mut Codegen,
    owner: &str,
    inferred: Option<&osprey_types::Type>,
) -> Result<Value> {
    let variants = cg.union_variants(owner).unwrap_or(&[]).to_vec();
    let block = cg.emit_reg("inttoptr i64 %payload to i64*".to_string());
    let tag = cg.emit_reg(format!("load i64, i64* {block}"));
    let end = cg.fresh_label();
    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(String, String)> = Vec::new();
    for (i, variant) in variants.iter().enumerate() {
        let hit = cg.fresh_label();
        let next = cg.fresh_label();
        let cond = cg.emit_reg(format!("icmp eq i64 {tag}, {i}"));
        cg.emit(format!("br i1 {cond}, label %{hit}, label %{next}"));
        cg.start_block(&hit);
        let s = render_variant(cg, variant, inferred)?;
        let from = cg.snapshot_to(&end);
        phi_in.push((s.operand, from));
        cg.start_block(&next);
    }
    let fallback = cg.string_constant("<union>");
    let from = cg.snapshot_to(&end);
    phi_in.push((fallback.operand, from));
    cg.start_block(&end);
    let phi = comma_join(&phi_in, |(v, b)| format!("[ {v}, %{b} ]"));
    let out = Value::new(cg.emit_reg(format!("phi i8* {phi}")), LType::Str);
    let operands = phi_in
        .into_iter()
        .map(|(operand, _)| operand)
        .collect::<Vec<_>>();
    crate::arc::move_phi_owners(cg, &operands, &out, mark);
    Ok(out)
}

/// One variant's rendering: nullary variants are their name; a payload lists
/// its fields — positional as `Name(a, b)`, named as `Name { f: a }`. Each
/// field is boxed and rendered through the shared entry, so nested shapes
/// stay truthful.
fn render_variant(
    cg: &mut Codegen,
    variant: &str,
    inferred: Option<&osprey_types::Type>,
) -> Result<Value> {
    let Some((view, struct_ty)) = cg
        .ctor_layout(variant)
        .filter(|v| !v.fields.is_empty())
        .zip(cg.ctor_struct_ty(variant))
    else {
        return Ok(cg.string_constant(&written(variant)));
    };
    let src = cg.emit_reg(format!("inttoptr i64 %payload to {struct_ty}*"));
    let positional = view
        .fields
        .first()
        .is_some_and(|(f, _)| osprey_ast::is_positional_field(f));
    let args = variant_arguments(cg, variant, &struct_ty, &src, &view.fields, inferred)?;
    let name = written(variant);
    let fmt = if positional {
        format!("{name}({})", comma_join(&args, |_| "%s".into()))
    } else {
        let holes: Vec<&String> = view.fields.iter().map(|(f, _)| f).collect();
        format!(
            "{name} {{ {} }}",
            comma_join(&holes, |f| format!("{f}: %s"))
        )
    };
    Ok(crate::runtime::format_sized(cg, &fmt, &args))
}

/// A variant as its source spells it in a pattern: `InvalidParameter`, never
/// the namespace-encoded symbol `__osp_5x…` a namespaced union stores.
fn written(variant: &str) -> String {
    osprey_ast::symbol::demangle(variant).map_or_else(
        || variant.to_owned(),
        |source| source.rsplit("::").next().unwrap_or(&source).to_owned(),
    )
}

fn variant_arguments(
    cg: &mut Codegen,
    variant: &str,
    struct_ty: &str,
    src: &str,
    fields: &[(String, LType)],
    inferred: Option<&osprey_types::Type>,
) -> Result<Vec<String>> {
    let mut source = Value::new("", LType::Ptr);
    source.inferred_type = inferred.cloned();
    let mut args = Vec::new();
    for (i, (fname, fty)) in fields.iter().enumerate() {
        let inferred = crate::aggregate::field_type(cg, &source, variant, fname);
        let loaded = crate::aggregate::load_record_field(cg, variant, struct_ty, src, i, *fty);
        let field = Value::new(loaded, *fty);
        let field = crate::aggregate::restore_field(cg, field, variant, fname, inferred.as_ref());
        args.push(format!("i8* {}", crate::show::nested(cg, field)?.operand));
    }
    Ok(args)
}
