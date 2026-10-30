//! Guard nested patterns before exposing payloads or installing arm bindings.
use super::{bind_value, list, literal, structural, union};
use crate::{
    builder::Codegen,
    error::{CodegenError, Result},
    llty::{LType, Value},
};
use osprey_ast::{Expr, MatchArm, Pattern};

pub(super) enum Capture {
    Value(String, Value),
    Tail(String, Value, usize),
    Row(String, Value, Pattern),
}

pub(super) fn bind(cg: &mut Codegen, captured: Vec<Capture>) -> Result<()> {
    for capture in captured {
        match capture {
            Capture::Value(name, value) => bind_value(cg, name, value),
            Capture::Row(name, value, pattern) => {
                let bound = structural::gen_structural_match(
                    cg,
                    &value,
                    &[MatchArm {
                        pattern,
                        body: Expr::Identifier(name.clone()),
                    }],
                )?;
                bind_value(cg, name, bound);
            }
            Capture::Tail(name, value, count) => {
                list::bind_list_arm(cg, &value, &[], Some(&name), count);
            }
        }
    }
    Ok(())
}

fn guard(cg: &mut Codegen, condition: &str, failed: &str) {
    let passed = cg.fresh_label();
    cg.emit(format!(
        "br i1 {condition}, label %{passed}, label %{failed}"
    ));
    cg.start_block(&passed);
}

pub(super) fn elements(
    cg: &mut Codegen,
    value: &Value,
    patterns: &[Pattern],
    failed: &str,
) -> Result<Vec<Capture>> {
    let mut captures = Vec::new();
    for (index, pattern) in patterns.iter().enumerate() {
        let element = list::element(cg, value, index);
        captures.extend(capture(cg, element, pattern, failed)?);
    }
    Ok(captures)
}

fn capture(
    cg: &mut Codegen,
    value: Value,
    pattern: &Pattern,
    failed: &str,
) -> Result<Vec<Capture>> {
    match pattern {
        Pattern::Binding(name) if cg.is_ctor(name) => {
            constructor(cg, value, name, &[], &[], failed)
        }
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => {
            Ok(vec![Capture::Value(name.clone(), value)])
        }
        Pattern::Wildcard => Ok(Vec::new()),
        Pattern::Literal(lit) => {
            let condition = literal::gen_eq(cg, &value, lit)?;
            guard(cg, &condition, failed);
            Ok(Vec::new())
        }
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } => constructor(cg, value, name, fields, sub_patterns, failed),
        Pattern::List { elements, rest } => {
            nested_list(cg, value, elements, rest.as_deref(), failed)
        }
        Pattern::Structural { fields, open } => row(cg, &value, fields, *open, failed),
    }
}

fn constructor(
    cg: &mut Codegen,
    value: Value,
    name: &str,
    fields: &[String],
    patterns: &[Pattern],
    failed: &str,
) -> Result<Vec<Capture>> {
    let view = cg
        .ctor_layout(name)
        .ok_or_else(|| CodegenError::unknown(name))?;
    let value = crate::cast::coerce_to(cg, value, LType::Ptr)?;
    let tag = union::discriminant(cg, &value, &view.owner);
    let condition = cg.emit_reg(format!("icmp eq i64 {tag}, {}", view.tag));
    guard(cg, &condition, failed);
    let struct_ty = cg
        .ctor_struct_ty(name)
        .ok_or_else(|| CodegenError::invalid("missing nested constructor layout"))?;
    let src = cg.emit_reg(format!("bitcast i8* {} to {struct_ty}*", value.operand));
    constructor_fields(
        cg,
        Payload {
            value: &value,
            name,
            ty: &struct_ty,
            src: &src,
            view: &view,
        },
        fields,
        patterns,
        failed,
    )
}

#[derive(Clone, Copy)]
struct Payload<'a> {
    value: &'a Value,
    name: &'a str,
    ty: &'a str,
    src: &'a str,
    view: &'a crate::builder::CtorView,
}

fn field_patterns(fields: &[String], patterns: &[Pattern]) -> (Vec<Pattern>, union::BindMode) {
    if fields.is_empty() {
        return (patterns.to_vec(), union::BindMode::BySlot);
    }
    let named = fields
        .iter()
        .map(|name| {
            if name.is_empty() {
                Pattern::Wildcard
            } else {
                Pattern::Binding(name.clone())
            }
        })
        .collect();
    (named, union::BindMode::ByName)
}

fn constructor_fields(
    cg: &mut Codegen,
    payload: Payload<'_>,
    fields: &[String],
    patterns: &[Pattern],
    failed: &str,
) -> Result<Vec<Capture>> {
    let (patterns, mode) = field_patterns(fields, patterns);
    let mut captures = Vec::new();
    for (column, pattern) in patterns.iter().enumerate() {
        let binder = fields.get(column).map_or("", String::as_str);
        let index = union::slot_of(payload.view, column, binder, mode)
            .ok_or_else(|| CodegenError::invalid("missing nested pattern field"))?;
        let field = union::field(
            cg,
            payload.value,
            payload.name,
            payload.ty,
            payload.src,
            payload.view,
            index,
        )
        .ok_or_else(|| CodegenError::invalid("missing nested payload"))?;
        captures.extend(capture(cg, field, pattern, failed)?);
    }
    Ok(captures)
}

fn nested_list(
    cg: &mut Codegen,
    value: Value,
    patterns: &[Pattern],
    rest: Option<&str>,
    failed: &str,
) -> Result<Vec<Capture>> {
    let value = list::runtime(cg, value)?;
    let length = cg.call("i64", "osprey_list_length", "i8*", &[&value.operand]);
    let operator = if rest.is_some() { "sge" } else { "eq" };
    let condition = cg.emit_reg(format!("icmp {operator} i64 {length}, {}", patterns.len()));
    guard(cg, &condition, failed);
    let mut captured = elements(cg, &value, patterns, failed)?;
    if let Some(name) = rest {
        captured.push(Capture::Tail(name.into(), value, patterns.len()));
    }
    Ok(captured)
}

fn row(
    cg: &mut Codegen,
    value: &Value,
    fields: &[(String, String)],
    open: bool,
    failed: &str,
) -> Result<Vec<Capture>> {
    if value.ty == LType::Any {
        return erased_row(cg, value, fields, open, failed);
    }
    let owner = value
        .osp_ty
        .as_deref()
        .ok_or_else(|| CodegenError::invalid("nested row has no owner"))?;
    let (ty, layout) = cg
        .record_layout(owner)
        .ok_or_else(|| CodegenError::invalid("nested row has no layout"))?;
    if !structural::row_selects(fields, open, &layout) {
        guard(cg, "false", failed);
        return Ok(Vec::new());
    }
    cg.with_local_scope(|cg| {
        structural::bind_row_statically(cg, value, owner, &ty, &layout, fields);
        fields
            .iter()
            .filter(|(_, name)| !name.is_empty())
            .map(|(_, name)| {
                cg.lookup(name)
                    .map(|value| Capture::Value(name.clone(), value))
                    .ok_or_else(|| CodegenError::unknown(name))
            })
            .collect()
    })
}

fn erased_row(
    cg: &mut Codegen,
    value: &Value,
    fields: &[(String, String)],
    open: bool,
    failed: &str,
) -> Result<Vec<Capture>> {
    let pattern = Pattern::Structural {
        fields: fields.to_vec(),
        open,
    };
    let matches = structural::gen_structural_match(
        cg,
        value,
        &[
            MatchArm {
                pattern: pattern.clone(),
                body: Expr::Bool(true),
            },
            MatchArm {
                pattern: Pattern::Wildcard,
                body: Expr::Bool(false),
            },
        ],
    )?;
    guard(cg, &matches.operand, failed);
    Ok(fields
        .iter()
        .filter(|(_, name)| !name.is_empty())
        .map(|(_, name)| Capture::Row(name.clone(), value.clone(), pattern.clone()))
        .collect())
}
