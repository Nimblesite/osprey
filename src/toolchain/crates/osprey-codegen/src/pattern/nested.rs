//! Guard nested patterns before exposing payloads or installing arm bindings.
use super::{bind_value, finish_checked_arm, list, literal, structural, union};
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

/// Whether every slot below the top of `pattern` matches any value, so the
/// arm is decided by its outermost test alone.
pub(super) fn is_flat(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Constructor { sub_patterns, .. } => {
            sub_patterns.iter().all(Pattern::is_irrefutable)
        }
        Pattern::Structural { fields, .. } => fields.iter().all(|(_, slot)| slot.is_irrefutable()),
        _ => true,
    }
}

/// An arm whose pattern tests below its top: test and capture the whole
/// pattern, falling through to the next arm when any part fails, and bind
/// only once all of it matched ([PATTERN-NESTED]).
pub(super) fn refutable_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    value: &Value,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    refutable_slot(cg, arm, value, &arm.pattern, phi_in)
}

/// [`refutable_arm`] over one part of the scrutinee (a `Result` payload).
pub(super) fn refutable_slot(
    cg: &mut Codegen,
    arm: &MatchArm,
    value: &Value,
    pattern: &Pattern,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    let next = cg.fresh_label();
    let captured = capture(cg, value.clone(), pattern, &next)?;
    finish_checked_arm(cg, arm, phi_in, &next, false, |cg| bind(cg, captured))
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

pub(super) fn capture(
    cg: &mut Codegen,
    value: Value,
    pattern: &Pattern,
    failed: &str,
) -> Result<Vec<Capture>> {
    match pattern {
        Pattern::Binding(name) if super::result::is_result_arm(pattern) => {
            result_payload(cg, value, name, &[], &[], failed)
        }
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
        } if super::result::is_result_arm(pattern) => {
            result_payload(cg, value, name, fields, sub_patterns, failed)
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

/// A `Success`/`Error` slot: test the discriminant, then match the payload
/// at its own type. A value that is not a `Result` is a `Success` of itself
/// ([PATTERN-RESULT-AUTOWRAP]).
fn result_payload(
    cg: &mut Codegen,
    value: Value,
    name: &str,
    fields: &[String],
    patterns: &[Pattern],
    failed: &str,
) -> Result<Vec<Capture>> {
    let success = name == "Success";
    let payload = if value.result_inner.is_some() {
        let tag = crate::result::load_disc(cg, &value);
        let operator = if success { "eq" } else { "ne" };
        let condition = cg.emit_reg(format!("icmp {operator} i8 {tag}, 0"));
        guard(cg, &condition, failed);
        if success {
            crate::result::load_value(cg, &value)
        } else {
            crate::result::load_error_payload(cg, &value)
        }
    } else if success {
        value
    } else {
        // Not a Result here (an `E` the backend cannot see binds its
        // rendering): an `Error` slot never selects, as a two-level match
        // reads it, and its names bind the empty text that match binds
        // ([PATTERN-RESULT-AUTOWRAP]).
        guard(cg, "false", failed);
        return Ok(unselected(cg, patterns.first(), fields.first()));
    };
    match (patterns.first(), fields.first()) {
        (Some(pattern), _) => capture(cg, payload, pattern, failed),
        (None, Some(binder)) => Ok(vec![Capture::Value(binder.clone(), payload)]),
        (None, None) => Ok(Vec::new()),
    }
}

/// The names an arm that cannot select binds, each to empty text so the arm
/// still lowers.
fn unselected(
    cg: &mut Codegen,
    pattern: Option<&Pattern>,
    binder: Option<&String>,
) -> Vec<Capture> {
    let mut names = Vec::new();
    match pattern {
        Some(pattern) => names_of(pattern, &mut names),
        None => names.extend(binder.cloned()),
    }
    names
        .into_iter()
        .map(|name| {
            let empty = Value::new(cg.string_constant("").operand, LType::Str);
            Capture::Value(name, empty)
        })
        .collect()
}

fn names_of(pattern: &Pattern, out: &mut Vec<String>) {
    match pattern {
        Pattern::Constructor {
            fields,
            sub_patterns,
            ..
        } if sub_patterns.is_empty() => out.extend(fields.iter().cloned()),
        Pattern::Constructor { sub_patterns, .. } => {
            for slot in sub_patterns {
                names_of(slot, out);
            }
        }
        Pattern::Structural { fields, .. } => {
            for (_, slot) in fields {
                names_of(slot, out);
            }
        }
        Pattern::List { elements, rest } => {
            for slot in elements {
                names_of(slot, out);
            }
            out.extend(rest.iter().cloned());
        }
        other if !other.slot_binder().is_empty() => out.push(other.slot_binder().to_owned()),
        _ => {}
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
    if !patterns.is_empty() {
        return (patterns.to_vec(), union::BindMode::ByName);
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
    fields: &[(String, Pattern)],
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
    let (struct_ty, layout) = cg
        .record_layout(owner)
        .ok_or_else(|| CodegenError::invalid("nested row has no layout"))?;
    if !structural::row_selects(&osprey_ast::row_binders(fields), open, &layout) {
        guard(cg, "false", failed);
        return Ok(Vec::new());
    }
    let record = structural::Record {
        disc: value,
        owner,
        struct_ty: &struct_ty,
        layout: &layout,
    };
    let mut captures = Vec::new();
    for (fname, slot) in fields {
        let field = structural::row_field(cg, record, fname)
            .ok_or_else(|| CodegenError::invalid("missing nested row field"))?;
        captures.extend(capture(cg, field, slot, failed)?);
    }
    Ok(captures)
}

/// A row over an erased box binds `any` children; the checker rejects a slot
/// that would test one, so every slot here binds or ignores.
fn erased_row(
    cg: &mut Codegen,
    value: &Value,
    fields: &[(String, Pattern)],
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
        .map(|(_, slot)| slot.slot_binder())
        .filter(|name| !name.is_empty())
        .map(|name| Capture::Row(name.to_owned(), value.clone(), pattern.clone()))
        .collect())
}
