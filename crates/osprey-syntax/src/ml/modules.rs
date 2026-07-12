//! Cross-item validation for the ML module surface. These checks need sibling
//! context but remain flavor-local, before canonical AST lowering.

use super::cst::MlItem;
use crate::SyntaxError;
use osprey_ast::Position;

pub(super) fn validate(items: &[MlItem], errors: &mut Vec<SyntaxError>) {
    validate_items(items, None, errors);
}

fn validate_items(items: &[MlItem], module_ascribed: Option<bool>, errors: &mut Vec<SyntaxError>) {
    for (index, item) in items.iter().enumerate() {
        match item {
            MlItem::Namespace {
                body: Some(body), ..
            } => validate_items(body, None, errors),
            MlItem::Module {
                signature, body, ..
            } => validate_items(body, Some(signature.is_some()), errors),
            MlItem::Export {
                item: exported,
                pos,
            } => validate_export(items, index, exported, *pos, module_ascribed, errors),
            MlItem::Opaque { pos, .. } => push_error(
                errors,
                *pos,
                "an opaque representation requires exactly one 'export opaque type' declaration",
            ),
            MlItem::ValueSignature { name, pos, .. } => {
                validate_signature_follower(items, index, name, false, *pos, errors);
            }
            _ => {}
        }
    }
}

fn validate_export(
    items: &[MlItem],
    index: usize,
    exported: &MlItem,
    pos: Position,
    module_ascribed: Option<bool>,
    errors: &mut Vec<SyntaxError>,
) {
    validate_export_context(module_ascribed, pos, errors);
    if matches!(exported, MlItem::Binding { mutable: true, .. }) {
        push_error(errors, pos, "mutable cells cannot be exported");
    }
    if let MlItem::ValueSignature { name, .. } = exported {
        validate_signature_follower(items, index, name, true, pos, errors);
    }
    if let MlItem::Opaque { item, .. } = exported {
        if !matches!(item.as_ref(), MlItem::Type { .. }) {
            push_error(
                errors,
                pos,
                "'opaque' may modify only an exported type declaration",
            );
        }
    }
}

fn validate_export_context(
    module_ascribed: Option<bool>,
    pos: Position,
    errors: &mut Vec<SyntaxError>,
) {
    match module_ascribed {
        None => push_error(
            errors,
            pos,
            "namespace declarations are public by default; 'export' is valid only inside an un-ascribed module",
        ),
        Some(true) => push_error(
            errors,
            pos,
            "an ascribed module exports exactly its signature; remove redundant 'export'",
        ),
        Some(false) => {}
    }
}

fn validate_signature_follower(
    items: &[MlItem],
    index: usize,
    name: &str,
    exported_signature: bool,
    pos: Position,
    errors: &mut Vec<SyntaxError>,
) {
    let follower = items.get(index.saturating_add(1));
    if is_matching_bare(follower, name) {
        return;
    }
    let message = signature_error(follower, name, exported_signature);
    push_error(errors, pos, message);
}

fn is_matching_bare(follower: Option<&MlItem>, name: &str) -> bool {
    matches!(
        follower,
        Some(MlItem::Binding {
            name: following,
            ..
        }) if following == name
    )
}

fn signature_error(follower: Option<&MlItem>, name: &str, exported: bool) -> String {
    let double = matches!(
        follower,
        Some(MlItem::Export { item, .. })
            if matches!(item.as_ref(), MlItem::Binding { name: following, .. } if following == name)
    );
    if exported && double {
        format!(
            "'{name}' is already exported by its signature; remove 'export' from the definition"
        )
    } else {
        format!("signature for '{name}' must be immediately followed by its bare definition")
    }
}

fn push_error(errors: &mut Vec<SyntaxError>, position: Position, message: impl Into<String>) {
    errors.push(SyntaxError {
        message: message.into(),
        position,
    });
}

pub(super) fn is_exportable(item: &MlItem) -> bool {
    matches!(
        item,
        MlItem::Binding { .. }
            | MlItem::ValueSignature { .. }
            | MlItem::Type { .. }
            | MlItem::Effect { .. }
            | MlItem::Extern { .. }
            | MlItem::Module { .. }
            | MlItem::Opaque { .. }
    )
}
