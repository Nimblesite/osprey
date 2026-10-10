//! Render erased scalar, row and union values.
use super::{
    boxed_field, comma_join, ensure_to_string, owned_call, row_block_ty, Codegen, DescKey, LType,
    Result, Value, TO_STRING_FN,
};

/// Emit the render function for `key`:
/// `define i8* @osp.any.str.<slug>(i8* %desc, i64 %payload)`.
pub(super) fn emit_renderer(cg: &mut Codegen, key: &DescKey, name: &str) -> Result<()> {
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
fn render_union(
    cg: &mut Codegen,
    owner: &str,
    inferred: Option<&osprey_types::Type>,
) -> Result<Value> {
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
    Ok(Value::new(
        cg.emit_reg(format!("phi i8* {phi}")),
        LType::Str,
    ))
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
        return Ok(cg.string_constant(variant));
    };
    ensure_to_string(cg);
    let src = cg.emit_reg(format!("inttoptr i64 %payload to {struct_ty}*"));
    let positional = view
        .fields
        .first()
        .is_some_and(|(f, _)| osprey_ast::is_positional_field(f));
    let args = variant_arguments(cg, variant, &struct_ty, &src, &view.fields, inferred)?;
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
        let child = boxed_field(
            cg,
            struct_ty,
            src,
            variant,
            (i, fname, *fty),
            inferred.as_ref(),
        )?;
        args.push(rendered_arg(cg, &child.operand));
    }
    Ok(args)
}
